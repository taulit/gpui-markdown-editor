//! Parses Markdown into the document model.
//!
//! `pulldown-cmark` does the Markdown work. This module maps its events to
//! blocks, keeps what the model has no node for as raw source, and assigns
//! every top-level block the byte range it was parsed from.

use std::iter::Peekable;
use std::ops::Range;
use std::sync::Arc;

use gpui::SharedString;
use pulldown_cmark::{
    Alignment, CodeBlockKind, Event, LinkType, OffsetIter, Options, Parser, Tag, TagEnd,
};

use super::inline::{Escaped, InlineBuilder};
use super::prefix::{self, Container};
use crate::extension::Extensions;
use crate::model::{
    Align, Block, BlockId, BlockKind, Cell, Document, ImageData, Inline, Mark, TableData,
};

/// How deep quotes and list items may nest before the rest is kept as raw
/// source. Parsing and writing recurse once per level, so without a limit a
/// line of two thousand `>` would overflow the stack.
const MAX_DEPTH: usize = 64;

/// The byte order mark some editors put at the start of a file.
pub(crate) const BOM: &str = "\u{feff}";

/// The Markdown dialect: CommonMark with GFM tables, task lists and
/// strikethrough. Footnotes, math and definition lists are recognized so that
/// they can be kept as raw source instead of being mangled as paragraphs.
fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_MATH
        | Options::ENABLE_DEFINITION_LIST
}

/// Parses `source` into a document.
pub(crate) fn parse(source: &str, extensions: Extensions) -> Document {
    let bom = if source.starts_with(BOM) {
        BOM.len()
    } else {
        0
    };
    let front_matter = front_matter_end(&source[bom..]).map(|end| bom..bom + end);
    let body = front_matter.as_ref().map_or(bom, |range| range.end);

    let mut parser = BlockParser::new(source, body, &extensions);
    let mut blocks = Vec::new();
    if let Some(range) = front_matter {
        blocks.push(parser.new_block(BlockKind::Raw, Inline::default(), Vec::new(), range));
    }
    while let Some((event, range)) = parser.next() {
        parser.block(event, range, &mut blocks);
    }
    let blocks = parser.tile(blocks, bom);
    let next_id = parser.next_id;
    Document {
        blocks,
        source: Arc::from(source),
        next_id,
        extensions,
    }
}

/// Finds YAML front matter at the very start of `text` and returns the offset
/// behind its closing line.
///
/// Front matter is a `---` line, at least one line of content that does not
/// start with a blank line, and a closing `---` or `...` line.
fn front_matter_end(text: &str) -> Option<usize> {
    const LINE_END: [char; 4] = ['\n', '\r', ' ', '\t'];
    let mut lines = text.split_inclusive('\n');
    let first = lines.next()?;
    if first.trim_end_matches(LINE_END) != "---" || !first.ends_with('\n') {
        return None;
    }
    let mut offset = first.len();
    for (index, line) in lines.enumerate() {
        let content = line.trim_end_matches(LINE_END);
        let closing = content == "---" || content == "...";
        if index == 0 && (content.is_empty() || closing) {
            return None;
        }
        if closing {
            return Some(offset + content.len());
        }
        offset += line.len();
    }
    None
}

type Ev<'s> = (Event<'s>, Range<usize>);

/// What ends a run of inline events.
#[derive(Clone, Copy, PartialEq, Eq)]
enum InlineEnd {
    /// The end tag of the paragraph, heading or table cell.
    Tag,
    /// The first event that is not inline. Tight list items hold their text
    /// without a paragraph around it.
    Implicit,
}

/// The inline content of one block and what else the parser learned from it.
struct Para {
    inline: Inline,
    /// The state of a task list marker in front of the text.
    task: Option<bool>,
    /// Set when the block consists of exactly one inline image.
    image: Option<ImageData>,
    /// `true` when the block consists of exactly one display math span.
    display_math: bool,
    /// `true` when the text would be an HTML block if it stood on its own:
    /// it is a single tag, or starts with a tag such as `<div>`. Text like
    /// that is only a paragraph where an HTML block cannot start, directly
    /// below a link reference definition for example.
    html_block: bool,
    /// Offset behind the last consumed event.
    end: usize,
}

/// What a start tag did, so that its end tag can undo it.
enum Opened {
    Mark,
    Link(Range<usize>),
    Nothing,
}

struct BlockParser<'s, 'e> {
    source: &'s str,
    /// Offset of the text given to pulldown-cmark within `source`.
    base: usize,
    events: Peekable<OffsetIter<'s>>,
    extensions: &'e Extensions,
    next_id: u64,
    /// The quotes and list items around the current position.
    containers: Vec<Container>,
    /// Start of the line last asked for and the offset up to which it holds.
    line_cache: (usize, usize),
}

impl<'s, 'e> BlockParser<'s, 'e> {
    fn new(source: &'s str, base: usize, extensions: &'e Extensions) -> Self {
        let events = Parser::new_ext(&source[base..], options())
            .into_offset_iter()
            .peekable();
        Self {
            source,
            base,
            events,
            extensions,
            next_id: 0,
            containers: Vec::new(),
            line_cache: (base, base),
        }
    }

    fn next(&mut self) -> Option<Ev<'s>> {
        let base = self.base;
        self.events
            .next()
            .map(|(event, range)| (event, range.start + base..range.end + base))
    }

    fn peek(&mut self) -> Option<&Event<'s>> {
        self.events.peek().map(|(event, _)| event)
    }

    fn alloc_id(&mut self) -> BlockId {
        let id = BlockId(self.next_id);
        self.next_id += 1;
        id
    }

    fn new_block(
        &mut self,
        kind: BlockKind,
        content: Inline,
        children: Vec<Block>,
        range: Range<usize>,
    ) -> Block {
        Block {
            id: self.alloc_id(),
            kind,
            content,
            children,
            origin: Some(range),
            dirty: false,
        }
    }

    /// A block that keeps its source. Top-level raw blocks get their text
    /// when the final source ranges are known, see [`Self::tile`].
    fn raw_block(&mut self, range: Range<usize>) -> Block {
        let content = if self.containers.is_empty() {
            Inline::default()
        } else {
            Inline::plain(self.raw_block_text(range.clone()))
        };
        self.new_block(BlockKind::Raw, content, Vec::new(), range)
    }

    /// Source text without the prefixes of the enclosing containers.
    fn raw_text(&self, range: Range<usize>) -> String {
        prefix::strip(self.source, &self.containers, range)
    }

    /// The text of a nested raw block: its source without container
    /// prefixes and without blank lines around it.
    fn raw_block_text(&self, range: Range<usize>) -> String {
        let stripped = self.raw_text(trim_range(self.source, range));
        stripped
            .trim_matches(|c: char| c.is_ascii_whitespace())
            .to_string()
    }

    /// Source text of an inline element that is kept as written. Like any
    /// soft line break, a line break inside it becomes a space. That keeps
    /// the span on one line, where its text cannot be taken for block syntax
    /// when the block is written again.
    fn raw_inline(&self, range: Range<usize>) -> String {
        let Some(text) = self.source.get(range.clone()) else {
            return String::new();
        };
        if !text.contains(['\n', '\r']) {
            return text.to_string();
        }
        let stripped = self.raw_text(range);
        let mut out = String::with_capacity(stripped.len());
        for (index, line) in stripped.split('\n').enumerate() {
            if index > 0 {
                out.truncate(out.trim_end_matches([' ', '\t']).len());
                out.push(' ');
            }
            out.push_str(line.trim_start_matches([' ', '\t']));
        }
        out
    }

    /// Start of the line containing `pos`. Positions are asked for in
    /// document order, so the scan continues where the last one stopped.
    fn line_start(&mut self, pos: usize) -> usize {
        let (start, upto) = self.line_cache;
        if pos < upto {
            return prefix::line_start(self.source, pos);
        }
        let start = self.source.as_bytes()[upto..pos]
            .iter()
            .rposition(|b| matches!(b, b'\n' | b'\r'))
            .map_or(start, |i| upto + i + 1);
        self.line_cache = (start, pos);
        start
    }

    /// Consumes events up to the end tag of the element whose start tag was
    /// just read. Returns the largest end offset seen.
    fn skip_element(&mut self) -> usize {
        let mut depth = 0usize;
        let mut end = 0;
        while let Some((event, range)) = self.next() {
            match event {
                Event::Start(_) => depth += 1,
                Event::End(_) if depth == 0 => break,
                Event::End(_) => {
                    depth -= 1;
                    end = end.max(range.end);
                }
                _ => end = end.max(range.end),
            }
        }
        end
    }

    /// Like [`Self::skip_element`], collecting the plain text on the way.
    fn skip_element_text(&mut self) -> String {
        let mut depth = 0usize;
        let mut text = String::new();
        while let Some((event, _)) = self.next() {
            match event {
                Event::Start(_) => depth += 1,
                Event::End(_) if depth == 0 => break,
                Event::End(_) => depth -= 1,
                Event::Text(t) | Event::Code(t) | Event::InlineMath(t) | Event::DisplayMath(t) => {
                    text.push_str(&t)
                }
                Event::SoftBreak | Event::HardBreak => text.push(' '),
                _ => {}
            }
        }
        text
    }

    /// Handles one block-level event and appends the resulting blocks.
    /// Returns the offset behind the block.
    fn block(&mut self, event: Event<'s>, range: Range<usize>, out: &mut Vec<Block>) -> usize {
        match event {
            Event::Start(Tag::Paragraph) => {
                let para = self.inline(None, InlineEnd::Tag, Some(range.start), false);
                let block = self.paragraph_block(para, range.clone());
                out.push(block);
            }
            Event::Start(Tag::Heading { level, .. }) => {
                let para = self.inline(None, InlineEnd::Tag, Some(range.start), false);
                let block = self.new_block(
                    BlockKind::Heading(level as u8),
                    para.inline,
                    Vec::new(),
                    range.clone(),
                );
                out.push(block);
            }
            Event::Start(Tag::BlockQuote(_)) => self.quote(range.clone(), out),
            Event::Start(Tag::CodeBlock(kind)) => {
                let mut code = String::new();
                while let Some((event, _)) = self.next() {
                    match event {
                        Event::Text(text) => code.push_str(&text),
                        Event::End(TagEnd::CodeBlock) => break,
                        _ => {}
                    }
                }
                // The line break in front of the closing fence is not content.
                if code.ends_with('\n') {
                    code.pop();
                }
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => {
                        let info = info.trim();
                        (!info.is_empty()).then(|| SharedString::from(info.to_string()))
                    }
                    CodeBlockKind::Indented => None,
                };
                let block = self.new_block(
                    BlockKind::CodeBlock { lang },
                    Inline::plain(code),
                    Vec::new(),
                    range.clone(),
                );
                out.push(block);
            }
            Event::Start(Tag::HtmlBlock) => {
                while let Some((event, _)) = self.next() {
                    if matches!(event, Event::End(TagEnd::HtmlBlock)) {
                        break;
                    }
                }
                let block = self.raw_block(range.clone());
                out.push(block);
            }
            Event::Start(Tag::List(start)) => {
                let mut index = 0u64;
                // The range of a list can reach beyond its last item, over
                // text that the caller still has to look at.
                let mut end = range.start;
                while let Some((event, item_range)) = self.next() {
                    match event {
                        Event::Start(Tag::Item) => {
                            let number = start.map(|first| first.saturating_add(index));
                            end = end.max(item_range.end);
                            self.item(item_range, number, out);
                            index += 1;
                        }
                        Event::End(TagEnd::List(_)) => break,
                        _ => {}
                    }
                }
                return end.min(range.end);
            }
            Event::Start(Tag::Item) => self.item(range.clone(), None, out),
            Event::Start(Tag::Table(alignments)) => {
                let block = self.table(&alignments, range.clone());
                out.push(block);
            }
            Event::Start(
                Tag::FootnoteDefinition(_)
                | Tag::DefinitionList
                | Tag::DefinitionListTitle
                | Tag::DefinitionListDefinition
                | Tag::MetadataBlock(_),
            ) => {
                // The range of a definition list can reach into the blocks
                // behind it, so the end is taken from its content.
                let inner_end = self.skip_element();
                let end = if inner_end > range.start {
                    inner_end.min(range.end)
                } else {
                    range.end
                };
                let block = self.raw_block(range.start..end);
                out.push(block);
                return end;
            }
            Event::Rule => {
                let block = self.new_block(
                    BlockKind::Rule,
                    Inline::default(),
                    Vec::new(),
                    range.clone(),
                );
                out.push(block);
            }
            Event::Html(_) => {
                let block = self.raw_block(range.clone());
                out.push(block);
            }
            Event::End(_) => {}
            other => {
                // Inline content without a paragraph around it.
                let start = implicit_start(self.source, range.start);
                let para = self.inline(Some((other, range)), InlineEnd::Implicit, None, false);
                let end = para.end;
                let block = self.paragraph_block(para, start..end);
                out.push(block);
                return end;
            }
        }
        range.end
    }

    fn paragraph_block(&mut self, para: Para, range: Range<usize>) -> Block {
        if let Some(image) = para.image {
            return self.new_block(
                BlockKind::Image(image),
                Inline::default(),
                Vec::new(),
                range,
            );
        }
        if para.display_math || para.html_block {
            // Unlike other raw blocks this one may be indented like code, so
            // it must not take its indentation along.
            let content = Inline::plain(self.raw_block_text(range.clone()));
            return self.new_block(BlockKind::Raw, content, Vec::new(), range);
        }
        self.new_block(BlockKind::Paragraph, para.inline, Vec::new(), range)
    }

    fn quote(&mut self, range: Range<usize>, out: &mut Vec<Block>) {
        let marker = skip_whitespace(self.source, range.start, range.end);
        if is_callout(self.source, marker) || self.containers.len() >= MAX_DEPTH {
            // Callouts (`> [!note]`) depend on their line structure, which
            // the quote model does not keep.
            self.skip_element();
            let block = self.raw_block(range);
            out.push(block);
            return;
        }
        let line_start = self.line_start(marker);
        let container = prefix::quote_container(self.source, &self.containers, line_start, marker);
        self.containers.push(container);
        let (content, _, children) = self.container_body(container.content_byte, range.end);
        self.containers.pop();
        let block = self.new_block(BlockKind::Quote, content, children, range);
        out.push(block);
    }

    fn item(&mut self, range: Range<usize>, number: Option<u64>, out: &mut Vec<Block>) {
        if self.containers.len() >= MAX_DEPTH {
            self.skip_element();
            let block = self.raw_block(range);
            out.push(block);
            return;
        }
        // Behind a tab, the range of an item can start in front of the quote
        // marker it follows.
        let bytes = self.source.as_bytes();
        let mut marker = skip_whitespace(self.source, range.start, range.end);
        while marker < range.end && matches!(bytes[marker], b'>' | b' ' | b'\t') {
            marker += 1;
        }
        let line_start = self.line_start(marker);
        let (container, marker_end) =
            prefix::item_container(self.source, &self.containers, line_start, marker);
        self.containers.push(container);
        let (content, task, children) = self.container_body(marker_end, range.end);
        self.containers.pop();
        let kind = match (task, number) {
            (Some(checked), _) => BlockKind::TaskItem { checked },
            (None, Some(number)) => BlockKind::OrderedItem {
                number: u32::try_from(number).unwrap_or(u32::MAX),
            },
            (None, None) => BlockKind::BulletItem,
        };
        let block = self.new_block(kind, content, children, marker..range.end);
        out.push(block);
    }

    /// Parses the inside of a quote or list item up to its end tag. The first
    /// paragraph becomes the content, everything else a child block.
    fn container_body(&mut self, start: usize, end: usize) -> (Inline, Option<bool>, Vec<Block>) {
        let mut content = Inline::default();
        let mut task = None;
        let mut children = Vec::new();
        let mut first = true;
        let mut cursor = start;
        while let Some((event, range)) = self.next() {
            match event {
                Event::End(TagEnd::Item | TagEnd::BlockQuote(_)) => break,
                Event::TaskListMarker(checked) if first => {
                    task = Some(checked);
                    cursor = cursor.max(range.end);
                    continue;
                }
                _ => {}
            }
            let gap_end = if is_inline(&event) {
                implicit_start(self.source, range.start)
            } else {
                range.start
            };
            // In a loose list the task marker comes after the start of the
            // paragraph, although it stands in front of it.
            let task_paragraph = first
                && matches!(event, Event::Start(Tag::Paragraph))
                && matches!(self.peek(), Some(Event::TaskListMarker(_)));
            if !task_paragraph {
                // pulldown-cmark drops the marker event when the text behind
                // it is not a paragraph. The marker is still in the source.
                let dropped_marker = (first && task.is_none())
                    .then(|| self.source.get(cursor..gap_end))
                    .flatten()
                    .and_then(|gap| match gap.trim_matches([' ', '\t']) {
                        "[ ]" => Some(false),
                        "[x]" | "[X]" => Some(true),
                        _ => None,
                    });
                match dropped_marker {
                    Some(checked) => task = Some(checked),
                    None => self.gap(cursor, gap_end, &mut children),
                }
            }
            let block_end = if first && children.is_empty() {
                match event {
                    Event::Start(Tag::Paragraph) => {
                        let para = self.inline(None, InlineEnd::Tag, Some(range.start), false);
                        task = task.or(para.task);
                        // Behind a task marker any text is a paragraph.
                        if para.html_block && task.is_none() {
                            let block = self.paragraph_block(para, range.clone());
                            children.push(block);
                        } else {
                            content = para.inline;
                        }
                        range.end
                    }
                    event if is_inline(&event) => {
                        let start = gap_end;
                        let para =
                            self.inline(Some((event, range)), InlineEnd::Implicit, None, false);
                        task = task.or(para.task);
                        let end = para.end;
                        // Behind a task marker any text is a paragraph.
                        if para.html_block && task.is_none() {
                            let block = self.paragraph_block(para, start..end);
                            children.push(block);
                        } else {
                            content = para.inline;
                        }
                        end
                    }
                    event => self.block(event, range, &mut children),
                }
            } else {
                self.block(event, range, &mut children)
            };
            first = false;
            cursor = cursor.max(block_end);
        }
        self.gap(cursor, end, &mut children);
        for child in &mut children {
            child.origin = None;
        }
        (content, task, children)
    }

    /// Keeps source between two nested blocks that produced no event, which
    /// is where link reference definitions live.
    fn gap(&mut self, from: usize, to: usize, out: &mut Vec<Block>) {
        let Some(text) = self.source.get(from..to) else {
            return;
        };
        if text.bytes().all(|b| b.is_ascii_whitespace() || b == b'>') {
            return;
        }
        let mut stripped = self.raw_text(from..to);
        // Lines at the end that hold nothing but quote markers lead up to
        // the next block.
        while let Some(line) = stripped.rsplit('\n').next()
            && stripped.contains('\n')
            && line.bytes().all(|b| b.is_ascii_whitespace() || b == b'>')
        {
            stripped.truncate(stripped.len() - line.len() - 1);
        }
        let trimmed = stripped.trim_matches(|c: char| c.is_ascii_whitespace());
        if trimmed.is_empty() {
            return;
        }
        // Text on the line where a raw block ends belongs to that block.
        let same_line = from > 0
            && !matches!(self.source.as_bytes()[from - 1], b'\n' | b'\r')
            && !stripped[..stripped.len() - stripped.trim_start().len()].contains('\n');
        if same_line
            && let Some(last) = out.last_mut()
            && matches!(last.kind, BlockKind::Raw)
        {
            if stripped.starts_with([' ', '\t']) {
                last.content.text.push(' ');
            }
            last.content.text.push_str(trimmed);
            return;
        }
        let content = Inline::plain(trimmed);
        let block = self.new_block(BlockKind::Raw, content, Vec::new(), from..to);
        out.push(block);
    }

    fn table(&mut self, alignments: &[Alignment], range: Range<usize>) -> Block {
        let align: Vec<Align> = alignments
            .iter()
            .map(|alignment| match alignment {
                Alignment::None => Align::None,
                Alignment::Left => Align::Left,
                Alignment::Center => Align::Center,
                Alignment::Right => Align::Right,
            })
            .collect();
        let mut rows = Vec::new();
        while let Some((event, _)) = self.next() {
            match event {
                Event::Start(Tag::TableHead | Tag::TableRow) => {
                    let mut row = Vec::with_capacity(align.len());
                    while let Some((event, cell_range)) = self.next() {
                        match event {
                            Event::Start(Tag::TableCell) => {
                                let para =
                                    self.inline(None, InlineEnd::Tag, Some(cell_range.start), true);
                                row.push(Cell {
                                    id: self.alloc_id(),
                                    content: para.inline,
                                });
                            }
                            Event::End(TagEnd::TableHead | TagEnd::TableRow) => break,
                            _ => {}
                        }
                    }
                    row.truncate(align.len());
                    while row.len() < align.len() {
                        row.push(Cell {
                            id: self.alloc_id(),
                            content: Inline::default(),
                        });
                    }
                    rows.push(row);
                }
                Event::End(TagEnd::Table) => break,
                _ => {}
            }
        }
        self.new_block(
            BlockKind::Table(TableData { align, rows }),
            Inline::default(),
            Vec::new(),
            range,
        )
    }

    /// Reads inline events into the content of one block.
    ///
    /// `first` is an event that was already taken from the stream. `start` is
    /// the offset where the block's text begins, if known.
    fn inline(
        &mut self,
        first: Option<Ev<'s>>,
        until: InlineEnd,
        start: Option<usize>,
        in_table: bool,
    ) -> Para {
        let source = self.source;
        let mut builder = InlineBuilder::new(self.extensions);
        let mut opened: Vec<Opened> = Vec::new();
        let mut task = None;
        let mut image = None;
        let mut images = 0usize;
        let mut display_math = 0usize;
        let mut tags = 0usize;
        let mut starts_html_block = false;
        let mut others = 0usize;
        let mut prev_end = start;
        let mut end = start.unwrap_or(0);
        let mut pending = first;
        loop {
            let (event, range) = match pending.take() {
                Some(event) => event,
                None => {
                    if until == InlineEnd::Implicit && !self.peek().is_some_and(is_inline) {
                        break;
                    }
                    match self.next() {
                        Some(event) => event,
                        None => break,
                    }
                }
            };
            end = end.max(range.end);
            let starts_element = matches!(event, Event::Start(_));
            match event {
                Event::End(TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::TableCell) => break,
                Event::Text(text) => {
                    others += 1;
                    let escaped = if source.get(range.clone()) != Some(&*text) {
                        Escaped::All
                    } else if range.start > 0
                        && source.as_bytes()[range.start - 1] == b'\\'
                        && prev_end.is_none_or(|prev| prev < range.start)
                        // In a table `\|` only keeps the pipe from ending the cell.
                        && !(in_table && text.starts_with('|'))
                    {
                        Escaped::First
                    } else {
                        Escaped::None
                    };
                    builder.text(&text, escaped);
                }
                Event::Code(code) => {
                    others += 1;
                    builder.code(&code);
                }
                Event::SoftBreak => {
                    others += 1;
                    builder.soft_break();
                }
                Event::HardBreak => {
                    others += 1;
                    builder.hard_break();
                }
                Event::InlineHtml(html) => {
                    if images + display_math + tags + others == 0 {
                        starts_html_block = starts_block(&html);
                    }
                    tags += 1;
                    match &*html {
                        "<u>" => builder.underline_open(),
                        "</u>" => {
                            if !builder.underline_close() {
                                builder.raw("</u>");
                            }
                        }
                        _ => builder.raw(&self.raw_inline(range.clone())),
                    }
                }
                Event::Html(_) | Event::InlineMath(_) | Event::FootnoteReference(_) => {
                    others += 1;
                    builder.raw(&self.raw_inline(range.clone()));
                }
                Event::DisplayMath(_) => {
                    display_math += 1;
                    builder.raw(&self.raw_inline(range.clone()));
                }
                Event::TaskListMarker(checked) => task = Some(checked),
                Event::Rule => {}
                Event::Start(tag) => match tag {
                    Tag::Emphasis => {
                        others += 1;
                        builder.open(Mark::Italic);
                        opened.push(Opened::Mark);
                    }
                    Tag::Strong => {
                        others += 1;
                        builder.open(Mark::Bold);
                        opened.push(Opened::Mark);
                    }
                    Tag::Strikethrough => {
                        others += 1;
                        builder.open(Mark::Strike);
                        opened.push(Opened::Mark);
                    }
                    // A link inside a link (an autolink as link text) has no
                    // meaning of its own: the outer link wins.
                    Tag::Link { .. } if opened.iter().any(|o| matches!(o, Opened::Link(_))) => {
                        opened.push(Opened::Nothing);
                    }
                    Tag::Link {
                        link_type: LinkType::Inline | LinkType::Autolink | LinkType::Email,
                        dest_url,
                        title,
                        ..
                    } => {
                        others += 1;
                        builder.open(Mark::Link {
                            url: shared(&dest_url),
                            title: (!title.is_empty()).then(|| shared(&title)),
                        });
                        opened.push(Opened::Link(range.clone()));
                    }
                    Tag::Link { .. } => {
                        // Reference-style links depend on a definition
                        // elsewhere and are kept as written.
                        others += 1;
                        end = end.max(self.skip_element());
                        builder.raw(&self.raw_inline(range.clone()));
                    }
                    Tag::Image {
                        link_type,
                        dest_url,
                        title,
                        ..
                    } => {
                        let alt = self.skip_element_text();
                        builder.raw(&self.raw_inline(range.clone()));
                        if link_type == LinkType::Inline {
                            images += 1;
                            image = Some(image_data(&alt, &dest_url, &title));
                        } else {
                            others += 1;
                        }
                    }
                    _ => opened.push(Opened::Nothing),
                },
                Event::End(_) => match opened.pop() {
                    Some(Opened::Mark) => {
                        builder.close();
                    }
                    Some(Opened::Link(link_range)) => {
                        // A link without text has nothing to carry the mark.
                        if !builder.close() {
                            builder.raw(&self.raw_inline(link_range));
                        }
                    }
                    Some(Opened::Nothing) | None => {}
                },
            }
            // The range of a start tag spans the whole element.
            prev_end = Some(if starts_element {
                range.start
            } else {
                range.end
            });
        }
        let only = |count: usize| count == 1 && images + display_math + tags + others == 1;
        Para {
            inline: builder.finish(),
            task,
            image: image.filter(|_| only(images)),
            display_math: only(display_math),
            html_block: only(tags) || starts_html_block,
            end,
        }
    }

    /// Turns the raw event ranges of the top-level blocks into source ranges
    /// that tile the document: every range starts at the beginning of its
    /// first line and ends behind its last visible character, so the text
    /// between two blocks is whitespace only.
    ///
    /// Source that no block covers (link reference definitions) becomes a raw
    /// block of its own.
    fn tile(&mut self, blocks: Vec<Block>, start: usize) -> Vec<Block> {
        let source = self.source;
        let bytes = source.as_bytes();
        let len = bytes.len();
        let starts: Vec<usize> = blocks
            .iter()
            .map(|block| block.origin.as_ref().map_or(0, |origin| origin.start))
            .collect();
        let mut out = Vec::with_capacity(blocks.len());
        let mut cursor = start;
        for (index, mut block) in blocks.into_iter().enumerate() {
            let raw = block.origin.take().unwrap_or(cursor..cursor);
            let mut begin = raw.start.clamp(cursor, len);
            let limit = starts
                .get(index + 1)
                .map_or(len, |next| (*next).clamp(begin, len));
            let mut end = raw.end.clamp(begin, limit);
            while begin < end && bytes[begin].is_ascii_whitespace() {
                begin += 1;
            }
            while end > begin && bytes[end - 1].is_ascii_whitespace() {
                end -= 1;
            }
            cursor = self.gap_block(cursor, begin, &mut out);
            // Two raw blocks on one line (pulldown-cmark reports a footnote
            // definition behind another one that way) are one block.
            if matches!(block.kind, BlockKind::Raw)
                && begin < end
                && let Some(last) = out.last_mut()
                && matches!(last.kind, BlockKind::Raw)
                && let Some(origin) = &mut last.origin
                && origin.end == cursor
                && !source[cursor..begin].contains(['\n', '\r'])
            {
                origin.end = end;
                last.content = Inline::plain(prefix::raw_block_text(&source[origin.clone()]));
                cursor = end;
                continue;
            }
            while begin > cursor && matches!(bytes[begin - 1], b' ' | b'\t') {
                begin -= 1;
            }
            if matches!(block.kind, BlockKind::Raw) && block.content.is_empty() {
                block.content = Inline::plain(prefix::raw_block_text(&source[begin..end]));
            }
            block.origin = Some(begin..end);
            cursor = end;
            out.push(block);
        }
        self.gap_block(cursor, len, &mut out);
        out
    }

    /// Adds a raw block for the text in `from..to` unless it is blank.
    /// Returns the offset behind the text that is covered now.
    fn gap_block(&mut self, from: usize, to: usize, out: &mut Vec<Block>) -> usize {
        let source = self.source;
        let bytes = source.as_bytes();
        let (mut begin, mut end) = (from, to);
        while begin < end && bytes[begin].is_ascii_whitespace() {
            begin += 1;
        }
        if begin >= end {
            return from;
        }
        while end > begin && bytes[end - 1].is_ascii_whitespace() {
            end -= 1;
        }
        // Text on the line where a raw block ends belongs to that block.
        if let Some(last) = out.last_mut()
            && matches!(last.kind, BlockKind::Raw)
            && let Some(origin) = &mut last.origin
            && origin.end == from
            && !source[from..begin].contains(['\n', '\r'])
        {
            origin.end = end;
            last.content = Inline::plain(prefix::raw_block_text(&source[origin.clone()]));
            return end;
        }
        while begin > from && matches!(bytes[begin - 1], b' ' | b'\t') {
            begin -= 1;
        }
        let content = Inline::plain(prefix::raw_block_text(&source[begin..end]));
        let block = self.new_block(BlockKind::Raw, content, Vec::new(), begin..end);
        out.push(block);
        end
    }
}

/// Returns `true` for events that belong to the text of a block.
fn is_inline(event: &Event<'_>) -> bool {
    match event {
        Event::Text(_)
        | Event::Code(_)
        | Event::InlineMath(_)
        | Event::DisplayMath(_)
        | Event::InlineHtml(_)
        | Event::FootnoteReference(_)
        | Event::SoftBreak
        | Event::HardBreak
        | Event::TaskListMarker(_) => true,
        Event::Start(tag) => matches!(
            tag,
            Tag::Emphasis
                | Tag::Strong
                | Tag::Strikethrough
                | Tag::Superscript
                | Tag::Subscript
                | Tag::Link { .. }
                | Tag::Image { .. }
        ),
        Event::End(tag) => matches!(
            tag,
            TagEnd::Emphasis
                | TagEnd::Strong
                | TagEnd::Strikethrough
                | TagEnd::Superscript
                | TagEnd::Subscript
                | TagEnd::Link
                | TagEnd::Image
        ),
        Event::Html(_) | Event::Rule => false,
    }
}

/// Returns `true` if a line that starts with this HTML tag starts an HTML
/// block, whatever follows the tag (start conditions 1 to 6 of CommonMark).
fn starts_block(tag: &str) -> bool {
    const BLOCK_TAGS: [&str; 66] = [
        "address",
        "article",
        "aside",
        "base",
        "basefont",
        "blockquote",
        "body",
        "caption",
        "center",
        "col",
        "colgroup",
        "dd",
        "details",
        "dialog",
        "dir",
        "div",
        "dl",
        "dt",
        "fieldset",
        "figcaption",
        "figure",
        "footer",
        "form",
        "frame",
        "frameset",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "head",
        "header",
        "hr",
        "html",
        "iframe",
        "legend",
        "li",
        "link",
        "main",
        "menu",
        "menuitem",
        "nav",
        "noframes",
        "ol",
        "optgroup",
        "option",
        "p",
        "param",
        "search",
        "section",
        "summary",
        "table",
        "tbody",
        "td",
        "tfoot",
        "th",
        "thead",
        "title",
        "tr",
        "track",
        "ul",
        "pre",
        "script",
        "style",
        "textarea",
    ];
    let Some(rest) = tag.strip_prefix('<') else {
        return false;
    };
    if rest.starts_with(['!', '?']) {
        return true;
    }
    let rest = rest.strip_prefix('/').unwrap_or(rest);
    let name = rest
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric())
        .count();
    BLOCK_TAGS
        .iter()
        .any(|block| rest[..name].eq_ignore_ascii_case(block))
}

fn shared(text: &str) -> SharedString {
    SharedString::from(text.to_string())
}

/// Builds image attributes. A numeric suffix of the alternative text is the
/// display width, as Obsidian writes it: `![alt|400](src)`.
fn image_data(alt: &str, src: &str, title: &str) -> ImageData {
    let (alt, width) = match alt.rsplit_once('|') {
        Some((rest, digits))
            if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) =>
        {
            match digits.parse::<u32>() {
                Ok(width) => (rest, Some(width)),
                Err(_) => (alt, None),
            }
        }
        _ => (alt, None),
    };
    ImageData {
        src: shared(src),
        alt: shared(alt),
        title: (!title.is_empty()).then(|| shared(title)),
        width,
    }
}

/// Where text without a paragraph around it starts. The range of its first
/// event leaves out a backslash that escapes the first character.
fn implicit_start(source: &str, start: usize) -> usize {
    match start.checked_sub(1) {
        Some(before) if source.as_bytes()[before] == b'\\' => before,
        _ => start,
    }
}

/// Returns the first offset in `from..to` that is not ASCII whitespace.
fn skip_whitespace(source: &str, from: usize, to: usize) -> usize {
    let bytes = source.as_bytes();
    let to = to.min(bytes.len());
    let mut pos = from.min(to);
    while pos < to && bytes[pos].is_ascii_whitespace() {
        pos += 1;
    }
    pos
}

/// Shrinks `range` so that it neither starts nor ends with ASCII whitespace.
fn trim_range(source: &str, range: Range<usize>) -> Range<usize> {
    let bytes = source.as_bytes();
    let mut end = range.end.min(bytes.len());
    let start = skip_whitespace(source, range.start, end);
    while end > start && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    start..end
}

/// Returns `true` if the quote whose `>` is at `marker` is a callout such as
/// `> [!note] Title`.
fn is_callout(source: &str, marker: usize) -> bool {
    let Some(rest) = source.get(marker..).and_then(|s| s.strip_prefix('>')) else {
        return false;
    };
    let Some(rest) = rest.trim_start_matches([' ', '\t']).strip_prefix("[!") else {
        return false;
    };
    let name = rest
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        .count();
    name > 0 && rest.as_bytes().get(name) == Some(&b']')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_front_matter() {
        assert_eq!(front_matter_end("---\ntitle: x\n---\n\ntext"), Some(16));
        assert_eq!(front_matter_end("---\r\na: b\r\n...\r\n"), Some(14));
        assert_eq!(front_matter_end("---\na: b\n---"), Some(12));
        assert_eq!(front_matter_end("---\n\na\n---\n"), None);
        assert_eq!(front_matter_end("---\n---\n"), None);
        assert_eq!(front_matter_end("---\nno end\n"), None);
        assert_eq!(front_matter_end("text\n---\na\n---\n"), None);
        assert_eq!(front_matter_end("---"), None);
    }

    #[test]
    fn splits_image_width() {
        assert_eq!(image_data("alt|400", "a.png", "").width, Some(400));
        assert_eq!(image_data("alt|400", "a.png", "").alt.as_ref(), "alt");
        assert_eq!(image_data("a|b", "a.png", "").width, None);
        assert_eq!(image_data("a|b", "a.png", "").alt.as_ref(), "a|b");
        assert_eq!(image_data("|99999999999", "a.png", "").width, None);
    }

    #[test]
    fn detects_callouts() {
        assert!(is_callout("> [!note] Title", 0));
        assert!(is_callout(">[!warning]-", 0));
        assert!(!is_callout("> [!] x", 0));
        assert!(!is_callout("> text", 0));
    }
}
