//! Writes a document back to Markdown.
//!
//! Blocks that were not edited are copied from the source together with the
//! whitespace between them. Everything else is written in the normalized
//! style. The care in this module goes into the seams: a rewritten block must
//! not change what the untouched text next to it means.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use super::parse::BOM;
use super::prefix::normalize_newlines;
use super::write_inline::{InlineCtx, InlineWriter, write_alt, write_destination, write_title};
use crate::model::{Align, Block, BlockKind, Document, ImageData, TableData};

/// Which blocks are written anew.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Only edited blocks. Everything else is copied from the source.
    Mixed,
    /// All blocks, in the normalized style.
    Normalized,
}

/// The two kinds of lists. Bullet and task items share one list.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Family {
    Bullet,
    Ordered,
}

fn family(kind: &BlockKind) -> Option<Family> {
    match kind {
        BlockKind::BulletItem | BlockKind::TaskItem { .. } => Some(Family::Bullet),
        BlockKind::OrderedItem { .. } => Some(Family::Ordered),
        _ => None,
    }
}

/// The marker characters of a list item.
#[derive(Clone, Copy)]
struct ListStyle {
    bullet: char,
    delimiter: char,
}

impl Default for ListStyle {
    fn default() -> Self {
        Self {
            bullet: '-',
            delimiter: '.',
        }
    }
}

/// The list item written last, which the next item may continue.
#[derive(Clone, Copy)]
struct Previous {
    family: Family,
    style: ListStyle,
    number: u32,
}

fn ordinal(kind: &BlockKind) -> u32 {
    match kind {
        BlockKind::OrderedItem { number } => *number,
        _ => 0,
    }
}

/// Picks the marker of an ordered item so that a parser reads its number
/// back: an item that does not continue the count of the item above starts a
/// new list, which Markdown expresses by switching between `1.` and `1)`.
/// Returns `true` if the item starts a new list that way.
fn settle_ordered(style: &mut ListStyle, kind: &BlockKind, previous: Option<Previous>) -> bool {
    let BlockKind::OrderedItem { number } = kind else {
        return false;
    };
    let restarts = previous.is_some_and(|previous| {
        previous.family == Family::Ordered
            && previous.style.delimiter == style.delimiter
            && previous.number.checked_add(1) != Some(*number)
    });
    if restarts {
        style.delimiter = if style.delimiter == '.' { ')' } else { '.' };
    }
    restarts
}

/// Reads the marker of the list item that `text` starts with.
fn source_marker(text: &str) -> Option<(Family, char)> {
    let text = text.trim_start_matches([' ', '\t']);
    let first = text.chars().next()?;
    if matches!(first, '-' | '+' | '*') {
        return Some((Family::Bullet, first));
    }
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    match text.as_bytes().get(digits) {
        Some(delimiter @ (b'.' | b')')) if digits > 0 => {
            Some((Family::Ordered, *delimiter as char))
        }
        _ => None,
    }
}

fn is_blank(text: &str) -> bool {
    text.bytes().all(|b| b.is_ascii_whitespace())
}

fn line_breaks(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut count = 0;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' => {
                count += 1;
                if bytes.get(index + 1) == Some(&b'\n') {
                    index += 1;
                }
            }
            b'\n' => count += 1,
            _ => {}
        }
        index += 1;
    }
    count
}

/// Returns `true` if most line endings of `source` are `\r\n`.
fn uses_crlf(source: &str) -> bool {
    let crlf = source.matches("\r\n").count();
    let lf = source.matches('\n').count() - crlf;
    crlf > lf
}

fn has_dirt(block: &Block) -> bool {
    block.dirty || block.children.iter().any(has_dirt)
}

/// The source range of a block that can be copied as it is.
fn clean_origin<'a>(block: &'a Block, source: &str) -> Option<&'a Range<usize>> {
    let origin = block.origin.as_ref()?;
    (!has_dirt(block) && source.get(origin.clone()).is_some()).then_some(origin)
}

/// Returns `true` if the document defines link references or footnotes, in
/// which case `[text]` anywhere can be a link.
fn has_definitions(blocks: &[Block]) -> bool {
    blocks.iter().any(|block| {
        (matches!(block.kind, BlockKind::Raw) && block.content.text.contains("]:"))
            || has_definitions(&block.children)
    })
}

/// Returns `true` if `text` is a fenced code block without a closing fence.
/// Such a block swallows everything that is written behind it.
fn is_unclosed_fence(text: &str) -> bool {
    let first = text.lines().next().unwrap_or("");
    let indent = first.len() - first.trim_start_matches(' ').len();
    if indent > 3 {
        return false;
    }
    let first = &first[indent..];
    let Some(fence) = first.chars().next().filter(|c| matches!(c, '`' | '~')) else {
        return false;
    };
    let length = first.chars().take_while(|c| *c == fence).count();
    if length < 3 {
        return false;
    }
    let mut lines = text.lines();
    lines.next();
    match lines.next_back() {
        Some(last) => {
            let last = last.trim();
            !(last.chars().all(|c| c == fence) && last.chars().count() >= length)
        }
        None => true,
    }
}

/// Serializes `document`.
pub(crate) fn serialize(document: &Document, mode: Mode) -> String {
    let source = &*document.source;
    let blocks = &document.blocks;
    let bom = if source.starts_with(BOM) { BOM } else { "" };
    if blocks.is_empty() {
        return if is_blank(&source[bom.len()..]) {
            source.to_string()
        } else {
            bom.to_string()
        };
    }
    let crlf = uses_crlf(source);
    let eol = if crlf { "\r\n" } else { "\n" };
    let mixed = mode == Mode::Mixed;

    let origin_of = |index: usize| {
        blocks[index]
            .origin
            .as_ref()
            .filter(|origin| source.get((*origin).clone()).is_some())
    };
    // Two blocks are neighbours in the source if only whitespace lies
    // between them.
    let source_gap = |index: usize| -> Option<&str> {
        let previous = origin_of(index.checked_sub(1)?)?;
        let current = origin_of(index)?;
        source
            .get(previous.end..current.start)
            .filter(|gap| is_blank(gap))
    };

    let mut copied: Vec<bool> = blocks
        .iter()
        .map(|block| mixed && clean_origin(block, source).is_some())
        .collect();
    if mixed {
        for index in 0..blocks.len() {
            if !copied[index] {
                continue;
            }
            let Some(origin) = origin_of(index) else {
                continue;
            };
            let text = &source[origin.clone()];
            let followed =
                index + 1 < blocks.len() && !(copied[index + 1] && source_gap(index + 1).is_some());
            let swallows = followed
                && matches!(blocks[index].kind, BlockKind::CodeBlock { .. })
                && is_unclosed_fence(text);
            if swallows {
                copied[index] = false;
            }
        }
    }

    let rewrites = copied.iter().any(|copied| !copied);
    let writer = Writer {
        inline: InlineWriter {
            extensions: &document.extensions,
            escape_brackets: rewrites && has_definitions(blocks),
        },
    };

    let mut out = String::with_capacity(source.len() + 64);
    match origin_of(0) {
        Some(origin) if mixed && is_blank(&source[bom.len()..origin.start]) => {
            out.push_str(&source[..origin.start]);
        }
        _ => out.push_str(bom),
    }

    let mut previous: Option<Previous> = None;
    for (index, block) in blocks.iter().enumerate() {
        // Indentation means something else behind a list item that was not
        // there before: the block would become part of the item.
        if copied[index]
            && index > 0
            && !(copied[index - 1] && source_gap(index).is_some())
            && family(&blocks[index - 1].kind).is_some()
            && origin_of(index)
                .is_some_and(|origin| source[origin.clone()].starts_with([' ', '\t']))
        {
            copied[index] = false;
        }
        let block_family = family(&block.kind);
        let continues = previous.filter(|previous| Some(previous.family) == block_family);

        // The marker of this item, known before the gap is chosen because
        // the gap depends on what the block starts with.
        let mut style = ListStyle::default();
        let mut restarts = false;
        let own_marker = origin_of(index)
            .and_then(|origin| source_marker(&source[origin.clone()]))
            .filter(|(marker_family, _)| Some(*marker_family) == block_family);
        if let Some(block_family) = block_family {
            let marker = if copied[index] {
                own_marker.map(|(_, marker)| marker)
            } else if let Some((_, marker)) = own_marker.filter(|_| mixed) {
                // An edited item keeps its marker, so the list stays whole.
                Some(marker)
            } else if let Some(previous) = continues {
                style = previous.style;
                None
            } else if mixed {
                // A new first item takes the marker of the list below it.
                blocks[index + 1..]
                    .iter()
                    .zip(&copied[index + 1..])
                    .take_while(|(next, _)| family(&next.kind) == Some(block_family))
                    .find(|(_, copied)| **copied)
                    .and_then(|(next, _)| next.origin.as_ref())
                    .and_then(|origin| source_marker(source.get(origin.clone())?))
                    .map(|(_, marker)| marker)
            } else {
                None
            };
            match (block_family, marker) {
                (Family::Bullet, Some(marker)) => style.bullet = marker,
                (Family::Ordered, Some(marker)) => style.delimiter = marker,
                (_, None) => {}
            }
            if !copied[index] {
                restarts = settle_ordered(&mut style, &block.kind, continues);
            }
        }

        if index > 0 {
            let before = &blocks[index - 1];
            let tight = continues.is_some() && !restarts;
            let normalized = if tight { 1 } else { 2 };
            // Spaces at the end of a rewritten block's last line are gone.
            let own_line = |gap: &'_ str| gap.len() - gap.trim_start_matches([' ', '\t']).len();
            let gap = source_gap(index).filter(|_| mixed).map(|gap| {
                if copied[index - 1] {
                    gap
                } else {
                    &gap[own_line(gap)..]
                }
            });
            let kept = match gap {
                Some(gap) if copied[index - 1] && copied[index] => Some(gap),
                Some(gap) if line_breaks(gap) >= 2 => Some(gap),
                Some(gap)
                    if line_breaks(gap) == 1
                        // `---` in the first line, directly followed by
                        // text, can be read as the start of front matter.
                        && !(index == 1 && matches!(before.kind, BlockKind::Rule))
                        && tight_ok(
                            before,
                            block,
                            origin_of(index)
                                .filter(|_| copied[index])
                                .map(|origin| &source[origin.clone()]),
                        ) =>
                {
                    Some(gap)
                }
                _ => None,
            };
            match kept {
                Some(gap) => out.push_str(gap),
                None => {
                    // Text directly below another block was read in that
                    // context, where `2) text` cannot start a list. Behind
                    // the blank line it gets now, it could.
                    let indented = origin_of(index)
                        .is_some_and(|origin| source[origin.clone()].starts_with([' ', '\t']));
                    let context_bound = indented
                        || matches!(
                            block.kind,
                            BlockKind::Paragraph
                                | BlockKind::Image(_)
                                | BlockKind::Heading(_)
                                | BlockKind::Table(_)
                                | BlockKind::Raw
                        );
                    if gap.is_some() && context_bound {
                        copied[index] = false;
                    }
                    for _ in 0..normalized {
                        out.push_str(eol);
                    }
                }
            }
        }

        match origin_of(index).filter(|_| copied[index]) {
            Some(origin) => out.push_str(&source[origin.clone()]),
            None => {
                let text = writer.block(block, style);
                if crlf {
                    out.push_str(&text.replace('\n', "\r\n"));
                } else {
                    out.push_str(&text);
                }
            }
        }
        previous = block_family.map(|family| Previous {
            family,
            style,
            number: ordinal(&block.kind),
        });
    }

    let last = blocks.len() - 1;
    match origin_of(last) {
        Some(origin) if mixed && is_blank(&source[origin.end..]) => {
            let rest = &source[origin.end..];
            if copied[last] {
                out.push_str(rest);
            } else {
                out.push_str(rest.trim_start_matches([' ', '\t']));
            }
        }
        _ => {
            let body = &source[bom.len()..];
            if is_blank(body) || body.ends_with(['\n', '\r']) {
                out.push_str(eol);
            }
        }
    }
    out
}

/// Decides whether `block` may directly follow `before` without a blank line
/// in between. `source_text` is the source of `block` if it is copied, which
/// can differ from the normalized form (a setext heading, an indented code
/// block).
fn tight_ok(before: &Block, block: &Block, source_text: Option<&str>) -> bool {
    use BlockKind::*;
    // Nothing continues a heading, a rule or a code block, also not at the
    // end of a list item or quote.
    fn ends_closed(block: &Block) -> bool {
        match block.children.last() {
            Some(last) => ends_closed(last),
            None => matches!(block.kind, Heading(_) | Rule | CodeBlock { .. }),
        }
    }
    if ends_closed(before) {
        return true;
    }
    let container = match before.kind {
        Heading(_) | Rule | CodeBlock { .. } => return true,
        // A table takes any following line as a row, HTML runs up to the
        // next blank line.
        Table(_) | Raw | Custom(_) => return false,
        Paragraph | Image(_) => false,
        Quote | BulletItem | OrderedItem { .. } | TaskItem { .. } => true,
    };
    let first = source_text.map(|text| text.trim_start_matches([' ', '\t']));
    // What follows must not be taken for a continuation of the text above.
    match &block.kind {
        Heading(_) => match first {
            Some(text) => text.starts_with('#'),
            None => !block.content.text.trim_end_matches('\n').contains('\n'),
        },
        CodeBlock { .. } => {
            first.is_none_or(|text| text.starts_with("```") || text.starts_with("~~~"))
        }
        Quote => !matches!(before.kind, Quote),
        Rule => match first {
            Some(text) => container || text.starts_with(['*', '_']),
            None => container,
        },
        BulletItem => {
            let delimiter_row = match first {
                Some(text) => is_delimiter_line(text.lines().next().unwrap_or("")),
                None => delimiter_like(block),
            };
            container || (has_text(block) && !delimiter_row)
        }
        TaskItem { .. } => true,
        OrderedItem { number } => {
            let starts_at_one = match first {
                Some(text) => text.starts_with("1.") || text.starts_with("1)"),
                None => *number == 1,
            };
            container || (starts_at_one && has_text(block))
        }
        _ => false,
    }
}

struct Writer<'a> {
    inline: InlineWriter<'a>,
}

impl Writer<'_> {
    /// Writes sibling blocks with a blank line between them, except between
    /// items of the same list.
    fn sequence(&self, blocks: &[Block]) -> String {
        let mut out = String::new();
        let mut previous: Option<Previous> = None;
        for (index, block) in blocks.iter().enumerate() {
            let block_family = family(&block.kind);
            let continues = previous.filter(|previous| Some(previous.family) == block_family);
            let mut style = continues.map_or_else(ListStyle::default, |previous| previous.style);
            let restarts = settle_ordered(&mut style, &block.kind, continues);
            if index > 0 {
                let tight = continues.is_some() && !restarts;
                out.push_str(if tight { "\n" } else { "\n\n" });
            }
            out.push_str(&self.block(block, style));
            previous = block_family.map(|family| Previous {
                family,
                style,
                number: ordinal(&block.kind),
            });
        }
        out
    }

    /// Writes one block in the normalized style, with `\n` line endings and
    /// without a trailing line break.
    fn block(&self, block: &Block, style: ListStyle) -> String {
        let line = InlineCtx {
            line_start: true,
            ..InlineCtx::default()
        };
        match &block.kind {
            BlockKind::Paragraph => self.inline.write(&block.content, line),
            BlockKind::Heading(level) => self.heading(block, *level),
            BlockKind::BulletItem => {
                let marker = format!("{} ", style.bullet);
                self.item(block, marker, 2, true, Some(style.bullet))
            }
            BlockKind::TaskItem { checked } => {
                let mark = if *checked { 'x' } else { ' ' };
                self.item(block, format!("{} [{mark}] ", style.bullet), 2, false, None)
            }
            BlockKind::OrderedItem { number } => {
                let marker = format!("{}{} ", (*number).min(999_999_999), style.delimiter);
                let indent = marker.len();
                self.item(block, marker, indent, true, None)
            }
            BlockKind::Quote => {
                let ctx = InlineCtx {
                    quote: true,
                    ..line
                };
                let mut body = self.inline.write(&block.content, ctx);
                if !block.children.is_empty() {
                    if !body.is_empty() {
                        body.push_str("\n\n");
                    }
                    body.push_str(&self.sequence(&block.children));
                }
                let mut out = String::with_capacity(body.len() + 16);
                for (index, text) in body.split('\n').enumerate() {
                    if index > 0 {
                        out.push('\n');
                    }
                    out.push('>');
                    if !text.is_empty() {
                        out.push(' ');
                        out.push_str(text);
                    }
                }
                out
            }
            BlockKind::CodeBlock { lang } => {
                code_block(&block.content.text, lang.as_ref().map(|lang| lang.as_ref()))
            }
            BlockKind::Table(table) => self.table(table),
            BlockKind::Image(image) => image_markdown(image),
            BlockKind::Rule => "---".to_string(),
            BlockKind::Raw | BlockKind::Custom(_) => normalize_newlines(&block.content.text),
        }
    }

    fn heading(&self, block: &Block, level: u8) -> String {
        let level = level.clamp(1, 6);
        let multiline = block.content.text.trim_end_matches('\n').contains('\n');
        if multiline && level <= 2 {
            // Only the setext form can hold a line break.
            let ctx = InlineCtx {
                line_start: true,
                ..InlineCtx::default()
            };
            let text = self.inline.write(&block.content, ctx);
            let underline = if level == 1 { "===" } else { "---" };
            return format!("{text}\n{underline}");
        }
        let ctx = InlineCtx {
            single_line: true,
            heading: true,
            ..InlineCtx::default()
        };
        let text = self.inline.write(&block.content, ctx);
        let mut out = "#".repeat(level as usize);
        if !text.is_empty() {
            out.push(' ');
            out.push_str(&text);
        }
        out
    }

    fn item(
        &self,
        block: &Block,
        marker: String,
        indent: usize,
        plain: bool,
        bullet: Option<char>,
    ) -> String {
        let ctx = InlineCtx {
            line_start: true,
            list_item: plain,
            bullet,
            ..InlineCtx::default()
        };
        let mut body = self.inline.write(&block.content, ctx);
        if let Some(first) = block.children.first() {
            let children = self.sequence(&block.children);
            if body.is_empty() {
                // Without text of its own the item starts on the next line.
                body.push('\n');
            } else if interrupts_paragraph(first) {
                body.push('\n');
            } else {
                body.push_str("\n\n");
            }
            body.push_str(&children);
        }
        let mut out = marker;
        // An empty task keeps the space behind its box: `- [ ]` at the very
        // end of a file is not read as a task.
        if plain && (body.is_empty() || body.starts_with('\n')) {
            out.truncate(out.trim_end().len());
        }
        let padding = " ".repeat(indent);
        for (index, text) in body.split('\n').enumerate() {
            if index > 0 {
                out.push('\n');
                if !text.is_empty() {
                    out.push_str(&padding);
                }
            }
            out.push_str(text);
        }
        out
    }

    fn table(&self, table: &TableData) -> String {
        let columns = table
            .rows
            .iter()
            .map(Vec::len)
            .chain([table.align.len()])
            .max()
            .unwrap_or(0);
        if columns == 0 {
            return String::new();
        }
        let ctx = InlineCtx {
            single_line: true,
            table: true,
            ..InlineCtx::default()
        };
        let mut rows: Vec<Vec<String>> = table
            .rows
            .iter()
            .map(|row| {
                (0..columns)
                    .map(|column| {
                        row.get(column)
                            .map(|cell| self.inline.write(&cell.content, ctx))
                            .unwrap_or_default()
                    })
                    .collect()
            })
            .collect();
        if rows.is_empty() {
            rows.push(vec![String::new(); columns]);
        }
        let widths: Vec<usize> = (0..columns)
            .map(|column| {
                rows.iter()
                    .map(|row| display_width(&row[column]))
                    .max()
                    .unwrap_or(0)
                    .max(3)
            })
            .collect();
        let align = |column: usize| table.align.get(column).copied().unwrap_or_default();

        let mut out = String::new();
        for (index, row) in rows.iter().enumerate() {
            if index > 0 {
                out.push('\n');
            }
            out.push('|');
            for (column, cell) in row.iter().enumerate() {
                let space = widths[column] - display_width(cell);
                let (left, right) = match align(column) {
                    Align::None | Align::Left => (0, space),
                    Align::Right => (space, 0),
                    Align::Center => (space / 2, space - space / 2),
                };
                out.push(' ');
                out.extend(std::iter::repeat_n(' ', left));
                out.push_str(cell);
                out.extend(std::iter::repeat_n(' ', right));
                out.push_str(" |");
            }
            if index == 0 {
                out.push_str("\n|");
                for (column, width) in widths.iter().enumerate() {
                    let (left, right) = match align(column) {
                        Align::None => ('-', '-'),
                        Align::Left => (':', '-'),
                        Align::Right => ('-', ':'),
                        Align::Center => (':', ':'),
                    };
                    out.push(' ');
                    out.push(left);
                    out.extend(std::iter::repeat_n('-', width - 2));
                    out.push(right);
                    out.push_str(" |");
                }
            }
        }
        out
    }
}

/// Returns `true` if the item `- | -` could be the delimiter row of a table
/// whose header is the line above.
fn delimiter_like(block: &Block) -> bool {
    is_delimiter_line(&block.content.text)
}

fn is_delimiter_line(line: &str) -> bool {
    line.bytes()
        .all(|b| matches!(b, b'-' | b':' | b'|' | b' ' | b'\t'))
}

/// Returns `true` if the block has text that is written. Line breaks alone
/// are not: an item with nothing but a line break is written as an empty
/// item, which below a paragraph is a setext underline.
fn has_text(block: &Block) -> bool {
    !block.content.text.trim().is_empty()
}

/// Returns `true` if the item can directly follow the text of its parent
/// without being read as a continuation of that text.
fn interrupts_paragraph(block: &Block) -> bool {
    match block.kind {
        BlockKind::BulletItem => has_text(block) && !delimiter_like(block),
        BlockKind::TaskItem { .. } => true,
        BlockKind::OrderedItem { number } => number == 1 && has_text(block),
        _ => false,
    }
}

fn code_block(code: &str, lang: Option<&str>) -> String {
    let code = normalize_newlines(code);
    let lang = lang.unwrap_or("").replace(['\n', '\r'], " ");
    let lang = lang.trim();
    // An info string with a backtick is only possible behind tildes.
    let fence_char = if lang.contains('`') { '~' } else { '`' };
    let mut longest = 0;
    let mut current = 0;
    for ch in code.chars() {
        current = if ch == fence_char { current + 1 } else { 0 };
        longest = longest.max(current);
    }
    let fence: String = std::iter::repeat_n(fence_char, (longest + 1).max(3)).collect();
    let mut out = format!("{fence}{lang}\n");
    if !code.is_empty() {
        out.push_str(&code);
        out.push('\n');
    }
    out.push_str(&fence);
    out
}

fn image_markdown(image: &ImageData) -> String {
    let mut out = format!("![{}", write_alt(image.alt.as_ref()));
    if let Some(width) = image.width {
        out.push('|');
        out.push_str(&width.to_string());
    }
    out.push_str("](");
    out.push_str(&write_destination(image.src.as_ref()));
    if let Some(title) = &image.title {
        out.push(' ');
        out.push_str(&write_title(title.as_ref()));
    }
    out.push(')');
    out
}

/// The number of terminal columns `text` takes up: one per grapheme, two for
/// East Asian wide characters and emoji.
fn display_width(text: &str) -> usize {
    text.graphemes(true)
        .map(|grapheme| {
            let code = grapheme.chars().next().map_or(0, |c| c as u32);
            let wide = matches!(
                code,
                0x1100..=0x115F
                    | 0x2E80..=0xA4CF
                    | 0xAC00..=0xD7A3
                    | 0xF900..=0xFAFF
                    | 0xFE30..=0xFE4F
                    | 0xFF00..=0xFF60
                    | 0xFFE0..=0xFFE6
                    | 0x1F300..=0x1FAFF
                    | 0x20000..=0x3FFFD
            );
            if wide { 2 } else { 1 }
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_fences() {
        assert_eq!(code_block("a", Some("rust")), "```rust\na\n```");
        assert_eq!(code_block("", None), "```\n```");
        assert_eq!(code_block("```\nx\n```", None), "````\n```\nx\n```\n````");
        assert_eq!(code_block("a", Some("a`b")), "~~~a`b\na\n~~~");
    }

    #[test]
    fn unclosed_fences() {
        assert!(is_unclosed_fence("```\ncode"));
        assert!(is_unclosed_fence("```"));
        assert!(!is_unclosed_fence("```\ncode\n```"));
        assert!(!is_unclosed_fence("~~~\ncode\n~~~~"));
        assert!(is_unclosed_fence("````\ncode\n```"));
        assert!(!is_unclosed_fence("    indented"));
    }

    #[test]
    fn widths() {
        assert_eq!(display_width("abc"), 3);
        assert_eq!(display_width("äöü"), 3);
        assert_eq!(display_width("日本"), 4);
        assert_eq!(display_width("e\u{301}"), 1);
    }

    #[test]
    fn markers() {
        assert!(matches!(
            source_marker("  * a"),
            Some((Family::Bullet, '*'))
        ));
        assert!(matches!(
            source_marker("12) a"),
            Some((Family::Ordered, ')'))
        ));
        assert!(source_marker("text").is_none());
    }
}
