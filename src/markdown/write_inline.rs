//! Writes [`Inline`] content as Markdown in the normalized style.
//!
//! The writer works in four steps: it cuts the text into segments with a
//! constant set of marks, nests the marks into open and close tokens, moves
//! whitespace out of emphasis (Markdown does not accept `** bold **`) and
//! finally writes the tokens, escaping literal characters only where they
//! would be read as syntax.

use std::ops::Range;

use super::inline::extension_matches;
use crate::extension::Extensions;
use crate::model::{Inline, Mark};

/// Where inline content is written.
#[derive(Clone, Copy, Default)]
pub(crate) struct InlineCtx {
    /// The content starts a line on which block syntax is recognized.
    pub line_start: bool,
    /// Line breaks cannot be written (ATX headings, table cells).
    pub single_line: bool,
    /// The content is a table cell: `|` ends the cell.
    pub table: bool,
    /// The content is an ATX heading: trailing `#` would be dropped.
    pub heading: bool,
    /// The content follows a list marker: `[ ]` would make it a task.
    pub list_item: bool,
    /// The content follows a quote marker: `[!note]` would make it a callout.
    pub quote: bool,
    /// The content follows this bullet: `- --` and `* **` would be rules.
    pub bullet: Option<char>,
}

/// Writes inline content. One writer serves a whole document.
pub(crate) struct InlineWriter<'a> {
    pub extensions: &'a Extensions,
    /// The document has link reference or footnote definitions, so any
    /// `[text]` may be a link. Both brackets are escaped then: a stray `]`
    /// can keep pulldown-cmark from resolving a reference in front of it.
    pub escape_brackets: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Style {
    Link(usize),
    Bold,
    Italic,
    Strike,
    Underline,
}

impl Style {
    /// Emphasis delimiters only work next to text, not next to whitespace.
    fn flanking(self) -> bool {
        matches!(self, Style::Bold | Style::Italic | Style::Strike)
    }

    /// Nesting order of marks that cover the same text, outermost first.
    fn order(self) -> u8 {
        match self {
            Style::Link(_) => 0,
            Style::Bold => 1,
            Style::Italic => 2,
            Style::Strike => 3,
            Style::Underline => 4,
        }
    }
}

enum AtomKind {
    Code,
    Raw,
    /// The Markdown an extension wrote for its mark.
    Extension(String),
}

/// A span that is written as a unit and cannot contain other marks.
struct Atom {
    range: Range<usize>,
    kind: AtomKind,
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Open(Style),
    Close(Style),
    Text {
        range: Range<usize>,
        highlight: bool,
    },
    Atom(usize),
}

/// How one character of literal text is written.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Esc {
    None,
    Backslash,
    /// As a numeric character reference.
    Entity,
    /// `\|` in a table cell. Unlike a backslash escape this does not hide
    /// the character from extensions.
    TablePipe,
    /// A hard break written with two spaces at the end of the line.
    SpaceBreak,
}

/// A stretch of output that the parser will read as one run of literal text.
#[derive(Default)]
struct Run {
    /// The text as the parser will decode it.
    text: String,
    /// One entry per byte of `text`. Only the entry of the first byte of a
    /// character is used.
    esc: Vec<Esc>,
    /// Ranges that are meant to be extension syntax.
    expected: Vec<Range<usize>>,
    /// Ranges written by extensions, which must stay as they are.
    locked: Vec<Range<usize>>,
}

impl Run {
    fn push(&mut self, ch: char, esc: Esc) {
        self.text.push(ch);
        self.esc.extend(std::iter::repeat_n(esc, ch.len_utf8()));
    }

    fn push_str(&mut self, text: &str) {
        self.text.push_str(text);
        self.esc.extend(std::iter::repeat_n(Esc::None, text.len()));
    }

    fn render(&self, out: &mut String, hard_break: &str) {
        for (index, ch) in self.text.char_indices() {
            match self.esc[index] {
                Esc::None if ch == '\n' => out.push_str(hard_break),
                Esc::None => out.push(ch),
                Esc::Backslash => {
                    out.push('\\');
                    out.push(ch);
                }
                Esc::Entity => {
                    out.push_str("&#");
                    out.push_str(&(ch as u32).to_string());
                    out.push(';');
                }
                Esc::TablePipe => out.push_str("\\|"),
                Esc::SpaceBreak => out.push_str("  \n"),
            }
        }
    }
}

enum Piece {
    Syntax(String),
    Run(Run),
}

/// How a character counts for the flanking rules of emphasis.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Space,
    Punct,
    Other,
    /// Not whitespace, a letter, a digit or ASCII punctuation. Whether it is
    /// punctuation depends on its Unicode category, which is not known here,
    /// so both readings are considered.
    Unknown,
}

fn class(ch: char) -> Class {
    if ch.is_whitespace() {
        Class::Space
    } else if ch.is_alphanumeric() {
        Class::Other
    } else if ch.is_ascii_punctuation() {
        Class::Punct
    } else {
        Class::Unknown
    }
}

/// Whether a delimiter run between two characters can open and close
/// emphasis.
fn flanking(byte: u8, before: Class, after: Class) -> (bool, bool) {
    let readings = |class: Class| match class {
        Class::Unknown => [Class::Punct, Class::Other],
        known => [known, known],
    };
    let mut result = (false, false);
    for before in readings(before) {
        for after in readings(after) {
            let left = after != Class::Space && (after != Class::Punct || before != Class::Other);
            let right = before != Class::Space && (before != Class::Punct || after != Class::Other);
            let (open, close) = match byte {
                b'_' => (
                    left && (!right || before == Class::Punct),
                    right && (!left || after == Class::Punct),
                ),
                b'$' => (after != Class::Space, before != Class::Space),
                _ => (left, right),
            };
            result.0 |= open;
            result.1 |= close;
        }
    }
    result
}

/// A run of `*`, `_`, `~` or `$` in literal text.
struct DelimRun {
    tok: usize,
    pos: usize,
    len: usize,
    ch: char,
    can_open: bool,
    can_close: bool,
    escape: bool,
}

struct Emit<'t> {
    text: &'t str,
    toks: &'t [Tok],
    atoms: &'t [Atom],
    ctx: InlineCtx,
    /// The part of the text that is written: hard breaks at its edges are
    /// left out where Markdown has no way to express them.
    lo: usize,
    hi: usize,
    delims: Vec<DelimRun>,
    next_delim: usize,
    /// Escape every literal `[`.
    brackets: bool,
    link_depth: usize,
    pieces: Vec<Piece>,
}

impl Emit<'_> {
    fn run(&mut self) -> &mut Run {
        if !matches!(self.pieces.last(), Some(Piece::Run(_))) {
            self.pieces.push(Piece::Run(Run::default()));
        }
        match self.pieces.last_mut() {
            Some(Piece::Run(run)) => run,
            _ => unreachable!("a run was pushed above"),
        }
    }

    fn syntax(&mut self, text: impl Into<String>) {
        self.pieces.push(Piece::Syntax(text.into()));
    }
}

impl InlineWriter<'_> {
    /// Writes `inline` as Markdown without a trailing line break.
    pub(crate) fn write(&self, inline: &Inline, ctx: InlineCtx) -> String {
        let text = inline.text.as_str();
        if text.is_empty() {
            return String::new();
        }
        let (toks, atoms, links) = self.tokens(inline);
        let backslash_breaks = !ctx.single_line;
        let toks = merge_text(expel_whitespace(text, toks, backslash_breaks));

        // A hard break at the end of the line cannot be written, one at the
        // start only with a backslash.
        let lo = if ctx.single_line {
            text.len() - text.trim_start_matches('\n').len()
        } else {
            0
        };
        let hi = match toks.last() {
            Some(Tok::Text { .. }) => text.trim_end_matches('\n').len().max(lo),
            // Behind the break comes syntax such as the end of a link.
            _ => text.len(),
        };
        let has_style = |wanted: &[Style]| {
            toks.iter()
                .any(|t| matches!(t, Tok::Open(s) if wanted.contains(s)))
        };
        let delims = delimiter_runs(
            text,
            &toks,
            &atoms,
            backslash_breaks,
            has_style(&[Style::Bold, Style::Italic]),
            has_style(&[Style::Strike]),
            // Math wins over code spans, so a `$` in either can be a partner.
            atoms.iter().any(|a| {
                matches!(a.kind, AtomKind::Raw | AtomKind::Code)
                    && text[a.range.clone()].contains('$')
            }),
        );
        let brackets = self.escape_brackets
            || toks.iter().any(
                |t| matches!(t, Tok::Text { range, .. } if text[range.clone()].contains("](")),
            );
        let mut emit = Emit {
            text,
            toks: &toks,
            atoms: &atoms,
            ctx,
            lo,
            hi,
            delims,
            next_delim: 0,
            brackets,
            link_depth: 0,
            pieces: Vec::new(),
        };

        let mut index = 0;
        while index < toks.len() {
            match &toks[index] {
                Tok::Open(Style::Link(link)) => {
                    let (url, title) = links[*link];
                    if let Some(Tok::Text {
                        range,
                        highlight: false,
                    }) = toks.get(index + 1)
                        && toks.get(index + 2) == Some(&Tok::Close(Style::Link(*link)))
                        && title.is_none()
                        && &text[range.clone()] == url
                        && is_autolink(url)
                    {
                        emit.syntax(format!("<{url}>"));
                        index += 3;
                        continue;
                    }
                    emit.syntax("[");
                    emit.link_depth += 1;
                }
                Tok::Close(Style::Link(link)) => {
                    let (url, title) = links[*link];
                    let mut close = String::from("](");
                    close.push_str(&write_destination(url));
                    if let Some(title) = title {
                        close.push(' ');
                        close.push_str(&write_title(title));
                    }
                    close.push(')');
                    emit.syntax(close);
                    emit.link_depth = emit.link_depth.saturating_sub(1);
                }
                Tok::Open(Style::Bold) | Tok::Close(Style::Bold) => emit.syntax("**"),
                Tok::Open(Style::Italic) | Tok::Close(Style::Italic) => emit.syntax("*"),
                Tok::Open(Style::Strike) | Tok::Close(Style::Strike) => emit.syntax("~~"),
                Tok::Open(Style::Underline) => emit.syntax("<u>"),
                Tok::Close(Style::Underline) => emit.syntax("</u>"),
                Tok::Text { range, highlight } => {
                    self.literal(&mut emit, index, range.clone(), *highlight);
                }
                Tok::Atom(atom) => {
                    let atom = &atoms[*atom];
                    let covered = &text[atom.range.clone()];
                    match &atom.kind {
                        AtomKind::Code => emit.syntax(write_code(covered, ctx.table)),
                        AtomKind::Raw => {
                            // At the start of a line, HTML begins an HTML
                            // block and `[^1]:` a footnote. Not so if the
                            // line is indented like code, which cannot
                            // interrupt a paragraph.
                            let after_break = matches!(
                                emit.pieces.last(),
                                Some(Piece::Run(run)) if run.text.ends_with('\n')
                            );
                            if after_break && !ctx.single_line {
                                emit.syntax(format!("    {covered}"));
                            } else {
                                emit.syntax(covered);
                            }
                        }
                        AtomKind::Extension(markdown) => {
                            let table = ctx.table;
                            let run = emit.run();
                            let start = run.text.len();
                            let mut prev = None;
                            let mut chars = markdown.char_indices().peekable();
                            while let Some((offset, ch)) = chars.next() {
                                let next = chars.peek().map(|(_, next)| *next);
                                let esc = if table && ch == '|' {
                                    Esc::TablePipe
                                } else if offset > 0
                                    && atom_needs_escape(&markdown[offset..], prev, ch, next)
                                {
                                    Esc::Backslash
                                } else {
                                    Esc::None
                                };
                                run.push(if ch == '\n' { ' ' } else { ch }, esc);
                                prev = Some(ch);
                            }
                            let range = start..run.text.len();
                            run.expected.push(range.clone());
                            run.locked.push(range);
                        }
                    }
                }
            }
            index += 1;
        }

        let hard_break = if ctx.single_line { " " } else { "\\\n" };
        let mut out = String::with_capacity(text.len() + 16);
        for piece in &mut emit.pieces {
            match piece {
                Piece::Syntax(syntax) => out.push_str(syntax),
                Piece::Run(run) => {
                    self.escape_extension_syntax(run);
                    run.render(&mut out, hard_break);
                }
            }
        }
        out
    }

    /// Cuts the text at mark boundaries and nests the marks.
    #[allow(clippy::type_complexity)]
    fn tokens<'i>(
        &self,
        inline: &'i Inline,
    ) -> (Vec<Tok>, Vec<Atom>, Vec<(&'i str, Option<&'i str>)>) {
        let text = inline.text.as_str();
        let len = text.len();
        let clip = |range: &Range<usize>| {
            let start = text.floor_char_boundary(range.start.min(len));
            let end = text.floor_char_boundary(range.end.min(len));
            (start < end).then_some(start..end)
        };

        let mut atoms: Vec<Atom> = Vec::new();
        let mut styled: Vec<(Range<usize>, Style)> = Vec::new();
        let mut highlights: Vec<Range<usize>> = Vec::new();
        let mut links: Vec<(&str, Option<&str>)> = Vec::new();
        for span in &inline.marks {
            let Some(range) = clip(&span.range) else {
                continue;
            };
            match &span.mark {
                Mark::Bold => styled.push((range, Style::Bold)),
                Mark::Italic => styled.push((range, Style::Italic)),
                Mark::Strike => styled.push((range, Style::Strike)),
                Mark::Underline => styled.push((range, Style::Underline)),
                Mark::Link { url, title } => {
                    links.push((url.as_ref(), title.as_ref().map(|t| t.as_ref())));
                    styled.push((range, Style::Link(links.len() - 1)));
                }
                Mark::Highlight => highlights.push(range),
                Mark::Code => atoms.push(Atom {
                    range,
                    kind: AtomKind::Code,
                }),
                Mark::Raw => atoms.push(Atom {
                    range,
                    kind: AtomKind::Raw,
                }),
                Mark::Custom { .. } => {
                    let covered = &text[range.clone()];
                    let written = self
                        .extensions
                        .iter()
                        .find_map(|extension| extension.serialize_mark(&span.mark, covered));
                    // Without an extension that knows the mark, the text is
                    // written as plain text.
                    if let Some(markdown) = written {
                        atoms.push(Atom {
                            range,
                            kind: AtomKind::Extension(markdown),
                        });
                    }
                }
            }
        }
        if atoms.is_empty() && styled.is_empty() && highlights.is_empty() {
            let tok = Tok::Text {
                range: 0..len,
                highlight: false,
            };
            return (vec![tok], atoms, links);
        }

        // Atoms must not overlap: the first one wins.
        atoms.sort_by_key(|atom| atom.range.start);
        let mut atom_end = 0;
        atoms.retain(|atom| {
            let keep = atom.range.start >= atom_end;
            if keep {
                atom_end = atom.range.end;
            }
            keep
        });
        let styled = union_styles(styled);
        highlights.sort_by_key(|range| range.start);
        let mut merged: Vec<Range<usize>> = Vec::new();
        for range in highlights {
            match merged.last_mut() {
                Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
                _ => merged.push(range),
            }
        }
        let highlights = merged;

        let mut bounds: Vec<usize> = vec![0, len];
        for (range, _) in &styled {
            bounds.extend([range.start, range.end]);
        }
        for range in &highlights {
            bounds.extend([range.start, range.end]);
        }
        for atom in &atoms {
            bounds.extend([atom.range.start, atom.range.end]);
        }
        bounds.sort_unstable();
        bounds.dedup();
        // Marks cannot start or end inside an atom.
        let mut atom_index = 0;
        bounds.retain(|bound| {
            while atoms
                .get(atom_index)
                .is_some_and(|atom| atom.range.end <= *bound)
            {
                atom_index += 1;
            }
            !atoms
                .get(atom_index)
                .is_some_and(|atom| atom.range.start < *bound)
        });

        let mut toks = Vec::new();
        let mut stack: Vec<(Style, usize)> = Vec::new();
        let mut active: Vec<(Style, usize)> = Vec::new();
        let mut next_styled = 0;
        let mut highlight_index = 0;
        let mut atom_index = 0;
        for window in bounds.windows(2) {
            let (start, end) = (window[0], window[1]);
            active.retain(|(_, style_end)| *style_end >= end);
            while let Some((range, style)) = styled.get(next_styled)
                && range.start <= start
            {
                if range.end >= end {
                    active.push((*style, range.end));
                }
                next_styled += 1;
            }
            while highlights
                .get(highlight_index)
                .is_some_and(|range| range.end <= start)
            {
                highlight_index += 1;
            }
            let highlight = highlights
                .get(highlight_index)
                .is_some_and(|range| range.start <= start && range.end >= end);
            while atoms
                .get(atom_index)
                .is_some_and(|atom| atom.range.end <= start)
            {
                atom_index += 1;
            }
            let atom = atoms
                .get(atom_index)
                .is_some_and(|atom| atom.range.start == start);

            // Close what ends here, and everything nested inside it.
            let keep = stack
                .iter()
                .position(|(style, _)| !active.iter().any(|(a, _)| a == style))
                .unwrap_or(stack.len());
            while stack.len() > keep {
                if let Some((style, _)) = stack.pop() {
                    toks.push(Tok::Close(style));
                }
            }
            // Open what starts here, the mark that ends last outermost.
            let mut opening: Vec<(Style, usize)> = active
                .iter()
                .filter(|(style, _)| !stack.iter().any(|(s, _)| s == style))
                .copied()
                .collect();
            opening.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.order().cmp(&b.0.order())));
            for entry in opening {
                toks.push(Tok::Open(entry.0));
                stack.push(entry);
            }
            toks.push(if atom {
                Tok::Atom(atom_index)
            } else {
                Tok::Text {
                    range: start..end,
                    highlight,
                }
            });
        }
        while let Some((style, _)) = stack.pop() {
            toks.push(Tok::Close(style));
        }
        (toks, atoms, links)
    }

    /// Writes one token of literal text into the current run.
    fn literal(&self, emit: &mut Emit<'_>, tok: usize, range: Range<usize>, highlight: bool) {
        let text = emit.text;
        let ctx = emit.ctx;
        let first_tok = tok == 0;
        let last_tok = tok + 1 == emit.toks.len();
        let token = &text[range.clone()];

        // Highlighting needs text at both ends and cannot contain `==`.
        let core = highlight
            .then(|| {
                let trimmed = token.trim();
                let start = range.start + (token.len() - token.trim_start().len());
                (trimmed, start..start + trimmed.len())
            })
            .filter(|(core, _)| {
                !core.is_empty()
                    && !core.contains('\n')
                    && !core.contains("==")
                    && !core.starts_with('=')
                    && !core.ends_with('=')
            })
            .map(|(_, core_range)| core_range);

        // The closing sequence of an ATX heading: `# title ##`.
        let heading_hash = (ctx.heading && last_tok)
            .then(|| {
                let body = token.trim_end_matches([' ', '\t', '\n']);
                let hashes = body.len() - body.trim_end_matches('#').len();
                let start = body.len() - hashes;
                let exposed = if start == 0 {
                    first_tok
                } else {
                    // A line break is written as a space in a heading.
                    body[..start].ends_with([' ', '\t', '\n'])
                };
                (hashes > 0 && exposed).then_some(range.start + start)
            })
            .flatten();
        let bang_before_link = token.ends_with('!')
            && match emit.toks.get(tok + 1) {
                Some(Tok::Open(Style::Link(_))) => true,
                Some(Tok::Atom(atom)) => {
                    let atom = &emit.atoms[*atom];
                    matches!(atom.kind, AtomKind::Raw) && text[atom.range.clone()].starts_with('[')
                }
                _ => false,
            };

        let after_bracket = tok
            .checked_sub(1)
            .and_then(|before| emit.toks.get(before))
            .is_some_and(|before| match before {
                Tok::Atom(atom) => {
                    let atom = &emit.atoms[*atom];
                    match &atom.kind {
                        AtomKind::Extension(markdown) => markdown.ends_with(']'),
                        AtomKind::Raw => text[atom.range.clone()].ends_with(']'),
                        AtomKind::Code => false,
                    }
                }
                _ => false,
            });
        // `[[Note]](x)` and `[ref](x)` would turn into an inline link.
        let paren_after_bracket = after_bracket && token.starts_with('(');
        // `[^1]: x` at the start of a line would define a footnote.
        let colon_after_bracket = after_bracket && tok == 1 && token.starts_with(':');

        // Inside link text, brackets that pair up are fine as they are, and
        // have to stay that way: `[ref][\[x\]](url)` would turn the link
        // text into the label of a reference link.
        let (mut escape_open_bracket, mut escape_close_bracket) = if emit.link_depth > 0 {
            let unbalanced = !brackets_balanced(token);
            (unbalanced, unbalanced)
        } else {
            (emit.brackets, self.escape_brackets)
        };

        // pulldown-cmark does not read a backslash in front of a line that
        // starts with `](` as a hard break.
        let break_before_link_end =
            matches!(emit.toks.get(tok + 1), Some(Tok::Close(Style::Link(_))));

        let mut line_escape: Option<usize> = None;
        let mut highlight_start = None;
        let mut prev: Option<char> = None;
        let mut chars = token.char_indices().peekable();
        while let Some((offset, ch)) = chars.next() {
            let pos = range.start + offset;
            let next = chars.peek().map(|(_, next)| *next);
            if pos < emit.lo || pos >= emit.hi {
                prev = Some(ch);
                continue;
            }
            if core.as_ref().is_some_and(|core| core.start == pos) {
                let run = emit.run();
                highlight_start = Some(run.text.len());
                run.push_str("==");
            }

            let inline_start = first_tok && pos == emit.lo;
            let line_start = if inline_start {
                ctx.line_start
            } else {
                prev == Some('\n') && !ctx.single_line
            };
            if line_start || inline_start {
                let line_end = token[offset..]
                    .find('\n')
                    .map_or(range.end, |i| pos + i)
                    .min(emit.hi);
                let line = &text[pos..line_end];
                let complete = line_end < range.end || last_tok;
                line_escape = if line_start {
                    line_start_escape(line, complete, inline_start)
                } else {
                    None
                };
                if inline_start && line_escape.is_none() {
                    let rule_with_bullet = complete
                        && ctx.bullet.is_some_and(|bullet| {
                            line.starts_with(bullet)
                                && line.chars().all(|c| c == bullet || c == ' ' || c == '\t')
                        });
                    let marker = (ctx.list_item && starts_with_task_marker(line))
                        || rule_with_bullet
                        || (ctx.quote && line.starts_with("[!"))
                        // The label of a link reference definition can span lines.
                        || (line_start && line.starts_with('[') && text[pos..].contains("]:"));
                    line_escape = marker.then_some(0);
                }
                line_escape = line_escape.map(|at| pos + at);
                if ch == '[' && line_escape == Some(pos) && emit.link_depth > 0 {
                    // With its partner escaped, a `]` would end the link.
                    escape_open_bracket = true;
                    escape_close_bracket = true;
                }
            }

            let mut esc = Esc::None;
            let mut out = ch;
            match ch {
                '\\' => {
                    if next.is_none_or(|c| c.is_ascii_punctuation() || c == '\n') {
                        esc = Esc::Backslash;
                    }
                }
                '`' => esc = Esc::Backslash,
                '<' => {
                    let tag_like = match next {
                        Some(c) => c.is_ascii_alphabetic() || matches!(c, '/' | '!' | '?'),
                        None => !last_tok,
                    };
                    if tag_like {
                        esc = Esc::Backslash;
                    }
                }
                '&' => {
                    if is_entity_like(&text[pos..range.end]) {
                        esc = Esc::Backslash;
                    }
                }
                '[' => {
                    if escape_open_bracket {
                        esc = Esc::Backslash;
                    }
                }
                ']' => {
                    if escape_close_bracket {
                        esc = Esc::Backslash;
                    }
                }
                '(' => {
                    if offset == 0 && paren_after_bracket {
                        esc = Esc::Backslash;
                    }
                }
                ':' => {
                    if offset == 0 && colon_after_bracket {
                        esc = Esc::Backslash;
                    }
                }
                '!' => {
                    if next.is_none() && bang_before_link {
                        esc = Esc::Backslash;
                    }
                }
                '|' => {
                    if ctx.table {
                        esc = Esc::TablePipe;
                    }
                }
                ' ' | '\t' => {
                    // Whitespace at the edges of a line is not content.
                    let leading = inline_start || (prev == Some('\n') && !ctx.single_line);
                    let trailing = last_tok && pos + 1 == emit.hi;
                    if leading || trailing {
                        esc = Esc::Entity;
                    }
                }
                '\r' => esc = Esc::Entity,
                // A byte order mark would be dropped at the start of a file.
                '\u{feff}' if inline_start => esc = Esc::Entity,
                '\n' => {
                    if ctx.single_line {
                        out = ' ';
                    } else if next.is_none() && break_before_link_end {
                        esc = Esc::SpaceBreak;
                    }
                }
                _ => {}
            }
            while emit
                .delims
                .get(emit.next_delim)
                .is_some_and(|d| (d.tok, d.pos + d.len) <= (tok, pos))
            {
                emit.next_delim += 1;
            }
            if emit
                .delims
                .get(emit.next_delim)
                .is_some_and(|d| d.tok == tok && d.pos <= pos && d.escape)
            {
                esc = Esc::Backslash;
            }
            if line_escape == Some(pos) || heading_hash == Some(pos) {
                esc = Esc::Backslash;
            }
            let run = emit.run();
            if esc == Esc::Entity
                && run.text.ends_with('\\')
                && let Some(last @ Esc::None) = run.esc.last_mut()
            {
                // A backslash in front of `&#32;` would escape the `&`.
                *last = Esc::Backslash;
            }
            run.push(out, esc);

            if let Some(core) = &core
                && core.end == pos + ch.len_utf8()
                && let Some(start) = highlight_start.take()
            {
                let run = emit.run();
                run.push_str("==");
                let end = run.text.len();
                run.expected.push(start..end);
            }
            prev = Some(ch);
        }
    }

    /// Escapes text that the registered extensions would read as their
    /// syntax although it is plain text in the model.
    fn escape_extension_syntax(&self, run: &mut Run) {
        if self.extensions.is_empty() || run.text.is_empty() {
            return;
        }
        // Every round escapes at least one character, so this ends. The
        // limit guards against extensions that do not honour escapes.
        for _ in 0..16 {
            let escaped: Vec<usize> = run
                .text
                .char_indices()
                .filter(|(index, _)| matches!(run.esc[*index], Esc::Backslash | Esc::Entity))
                .map(|(index, _)| index)
                .collect();
            let mut progress = false;
            for found in extension_matches(self.extensions, &run.text, &escaped) {
                if run.expected.contains(&found.range) {
                    continue;
                }
                let at = found.range.start;
                let escapable = run.text.as_bytes()[at].is_ascii_punctuation()
                    && run.esc[at] == Esc::None
                    && !run.locked.iter().any(|locked| locked.contains(&at));
                if escapable {
                    run.esc[at] = Esc::Backslash;
                    progress = true;
                }
            }
            if !progress {
                break;
            }
        }
    }
}

/// Decides whether a character inside the Markdown written by an extension
/// would be read as other inline syntax. The parser hands extensions the
/// text with escapes resolved, so escaping here does not hide the syntax
/// from the extension. The first character is left alone: an escaped start
/// is how literal text is told apart from extension syntax.
fn atom_needs_escape(rest: &str, prev: Option<char>, ch: char, next: Option<char>) -> bool {
    match ch {
        '`' | '*' | '~' | '$' => true,
        '_' => {
            !(prev.is_some_and(char::is_alphanumeric) && next.is_some_and(char::is_alphanumeric))
        }
        '<' => next.is_some_and(|c| c.is_ascii_alphabetic() || matches!(c, '/' | '!' | '?')),
        '&' => is_entity_like(rest),
        '\\' => next.is_none_or(|c| c.is_ascii_punctuation()),
        _ => false,
    }
}

/// Sorts styled spans and unites spans of the same style. Links cannot
/// overlap each other, so a link that starts inside another one is cut.
fn union_styles(mut styled: Vec<(Range<usize>, Style)>) -> Vec<(Range<usize>, Style)> {
    styled.sort_by_key(|(range, _)| range.start);
    let mut out: Vec<(Range<usize>, Style)> = Vec::with_capacity(styled.len());
    let mut last: [Option<usize>; 4] = [None; 4];
    let mut link_end = 0;
    for (mut range, style) in styled {
        let slot = match style {
            Style::Bold => 0,
            Style::Italic => 1,
            Style::Strike => 2,
            Style::Underline => 3,
            Style::Link(_) => {
                range.start = range.start.max(link_end);
                if range.start < range.end {
                    link_end = range.end;
                    out.push((range, style));
                }
                continue;
            }
        };
        if let Some(index) = last[slot]
            && range.start <= out[index].0.end
        {
            out[index].0.end = out[index].0.end.max(range.end);
            continue;
        }
        last[slot] = Some(out.len());
        out.push((range, style));
    }
    // Cutting links can disturb the order.
    out.sort_by_key(|(range, _)| range.start);
    out
}

/// Moves whitespace at the inner edges of emphasis outside of it and drops
/// emphasis that is left empty.
///
/// With `backslash_breaks`, a hard break is written as `\` and a line break.
/// Emphasis can start with it, unless a letter or digit stands in front.
fn expel_whitespace(text: &str, toks: Vec<Tok>, backslash_breaks: bool) -> Vec<Tok> {
    let mut out: Vec<Tok> = Vec::with_capacity(toks.len());
    for tok in toks {
        match tok {
            Tok::Text { range, highlight } => {
                let token = &text[range.clone()];
                let lead = token.len() - token.trim_start().len();
                let after_open = matches!(out.last(), Some(Tok::Open(style)) if style.flanking());
                let break_stays = backslash_breaks
                    && token.starts_with('\n')
                    && !out
                        .iter()
                        .rev()
                        .find(|tok| !matches!(tok, Tok::Open(style) if style.flanking()))
                        .is_some_and(|tok| match tok {
                            Tok::Text { range, .. } => {
                                text[range.clone()].chars().next_back().is_some_and(|ch| {
                                    matches!(class(ch), Class::Other | Class::Unknown)
                                })
                            }
                            _ => false,
                        });
                if lead == 0 || !after_open || break_stays {
                    out.push(Tok::Text { range, highlight });
                    continue;
                }
                let mut opens = Vec::new();
                while matches!(out.last(), Some(Tok::Open(style)) if style.flanking()) {
                    opens.extend(out.pop());
                }
                out.push(Tok::Text {
                    range: range.start..range.start + lead,
                    highlight,
                });
                out.extend(opens.into_iter().rev());
                if lead < token.len() {
                    out.push(Tok::Text {
                        range: range.start + lead..range.end,
                        highlight,
                    });
                }
            }
            Tok::Close(style) if style.flanking() => {
                let mut tail = Vec::new();
                while let Some(Tok::Text { range, highlight }) = out.last_mut() {
                    let token = &text[range.clone()];
                    let trail = token.len() - token.trim_end().len();
                    if trail == 0 {
                        break;
                    }
                    if trail == token.len() {
                        tail.extend(out.pop());
                        continue;
                    }
                    let split = range.end - trail;
                    tail.push(Tok::Text {
                        range: split..range.end,
                        highlight: *highlight,
                    });
                    range.end = split;
                    break;
                }
                if out.last() == Some(&Tok::Open(style)) {
                    out.pop();
                } else {
                    out.push(Tok::Close(style));
                }
                out.extend(tail.into_iter().rev());
            }
            other => out.push(other),
        }
    }
    out
}

/// Joins text tokens that ended up next to each other.
fn merge_text(toks: Vec<Tok>) -> Vec<Tok> {
    let mut out: Vec<Tok> = Vec::with_capacity(toks.len());
    for tok in toks {
        if let Tok::Text { range, highlight } = &tok
            && let Some(Tok::Text {
                range: last,
                highlight: last_highlight,
            }) = out.last_mut()
            && last.end == range.start
            && last_highlight == highlight
        {
            last.end = range.end;
            continue;
        }
        out.push(tok);
    }
    out
}

/// Finds the runs of emphasis-like characters in literal text and decides
/// which of them have to be escaped.
///
/// A run is escaped if it could open emphasis and a later run could close
/// it, or the other way round. With marks of the same character present
/// (`*` for bold and italic, `~` for strikethrough), the written delimiters
/// are possible partners as well.
fn delimiter_runs(
    text: &str,
    toks: &[Tok],
    atoms: &[Atom],
    backslash_breaks: bool,
    star_marks: bool,
    tilde_marks: bool,
    dollar_atoms: bool,
) -> Vec<DelimRun> {
    // The character a neighbouring token puts next to a text token. Syntax
    // counts as punctuation, the edges of the text as whitespace.
    let neighbour = |index: Option<usize>, last: bool| match index.and_then(|i| toks.get(i)) {
        None => Class::Space,
        Some(Tok::Atom(atom)) => {
            let atom = &atoms[*atom];
            let written = match &atom.kind {
                AtomKind::Extension(markdown) => markdown.as_str(),
                AtomKind::Raw => &text[atom.range.clone()],
                AtomKind::Code => "`",
            };
            let ch = if last {
                written.chars().next_back()
            } else {
                written.chars().next()
            };
            ch.map_or(Class::Punct, class)
        }
        Some(_) => Class::Punct,
    };
    let mut runs: Vec<DelimRun> = Vec::new();
    for (tok, token) in toks.iter().enumerate() {
        let Tok::Text { range, .. } = token else {
            continue;
        };
        let token = &text[range.clone()];
        let bytes = token.as_bytes();
        let mut offset = 0;
        while offset < bytes.len() {
            let byte = bytes[offset];
            if !matches!(byte, b'*' | b'_' | b'~' | b'$') {
                offset += 1;
                continue;
            }
            let len = bytes[offset..].iter().take_while(|b| **b == byte).count();
            let before = match token[..offset].chars().next_back() {
                Some(ch) => class(ch),
                None => neighbour(tok.checked_sub(1), true),
            };
            let after = match token[offset + len..].chars().next() {
                // A hard break is written as a backslash at the end of the line.
                Some('\n') if backslash_breaks => Class::Punct,
                Some(ch) => class(ch),
                None => neighbour(Some(tok + 1), false),
            };
            let (can_open, can_close) = flanking(byte, before, after);
            runs.push(DelimRun {
                tok,
                pos: range.start + offset,
                len,
                ch: byte as char,
                can_open,
                can_close,
                escape: false,
            });
            offset += len;
        }
    }
    for ch in ['*', '_', '~', '$'] {
        let marks = match ch {
            '*' => star_marks,
            '~' => tilde_marks,
            '$' => dollar_atoms,
            _ => false,
        };
        let mut opener_before = false;
        for run in runs.iter_mut().filter(|run| run.ch == ch) {
            // `$$$$` is empty display math all by itself.
            let pairs_with_itself = ch == '$' && run.len >= 2;
            run.escape = pairs_with_itself || (run.can_close && (opener_before || marks));
            opener_before |= run.can_open;
        }
        let mut closer_after = false;
        for run in runs.iter_mut().rev().filter(|run| run.ch == ch) {
            run.escape |= run.can_open && (closer_after || marks);
            closer_after |= run.can_close;
        }
    }
    runs
}

/// Checks whether a line of text would be read as block syntax and returns
/// the offset of the character to escape.
///
/// `complete` tells that the line ends with this text. `first` is `true` for
/// the first line of a block, which cannot be a setext underline or the
/// delimiter row of a table.
pub(crate) fn line_start_escape(line: &str, complete: bool, first: bool) -> Option<usize> {
    let bytes = line.as_bytes();
    let first_byte = *bytes.first()?;
    let space_at = |index: usize| match bytes.get(index) {
        Some(b' ' | b'\t') => true,
        Some(_) => false,
        None => complete,
    };
    let body = line.trim_end_matches([' ', '\t']);
    let only = |allowed: &[u8]| !body.is_empty() && body.bytes().all(|b| allowed.contains(&b));
    let count = |wanted: u8| body.bytes().filter(|b| *b == wanted).count();
    match first_byte {
        b'#' => {
            let hashes = bytes.iter().take_while(|b| **b == b'#').count();
            (hashes <= 6 && space_at(hashes)).then_some(0)
        }
        b'>' => Some(0),
        b'-' | b'+' | b'*' | b'_' | b'=' => {
            let marker = matches!(first_byte, b'-' | b'+' | b'*') && space_at(1);
            let rule = complete
                && matches!(first_byte, b'-' | b'*' | b'_')
                && only(&[first_byte, b' ', b'\t'])
                && count(first_byte) >= 3;
            // `-` and `=` under a line of text make that line a heading.
            let underline =
                complete && !first && matches!(first_byte, b'-' | b'=') && only(&[first_byte]);
            let table = complete && !first && is_delimiter_row(body);
            (marker || rule || underline || table).then_some(0)
        }
        b'0'..=b'9' => {
            let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
            let delimiter = matches!(bytes.get(digits), Some(b'.' | b')'));
            (digits <= 9 && delimiter && space_at(digits + 1)).then_some(digits)
        }
        // pulldown-cmark reads any line that starts with `:` below a
        // paragraph as a definition, even behind a blank line.
        b':' => Some(0),
        b'|' => (complete && !first && is_delimiter_row(body)).then_some(0),
        b'[' => line.contains("]:").then_some(0),
        b'~' => line.starts_with("~~~").then_some(0),
        _ => None,
    }
}

fn brackets_balanced(text: &str) -> bool {
    let mut depth = 0usize;
    for byte in text.bytes() {
        match byte {
            b'[' => depth += 1,
            b']' => match depth.checked_sub(1) {
                Some(less) => depth = less,
                None => return false,
            },
            _ => {}
        }
    }
    depth == 0
}

/// Returns `true` for a line like `|---|:-:|` that turns the line above it
/// into a table header.
fn is_delimiter_row(line: &str) -> bool {
    line.contains('-')
        && line.contains('|')
        && line
            .bytes()
            .all(|b| matches!(b, b'-' | b':' | b'|' | b' ' | b'\t'))
}

fn starts_with_task_marker(line: &str) -> bool {
    let bytes = line.as_bytes();
    bytes.len() >= 3
        && bytes[0] == b'['
        && matches!(bytes[1], b' ' | b'x' | b'X')
        && bytes[2] == b']'
        && matches!(bytes.get(3), None | Some(b' ' | b'\t'))
}

/// Returns `true` if `text` starts with something shaped like a character
/// reference (`&amp;`, `&#38;`, `&#x26;`).
pub(crate) fn is_entity_like(text: &str) -> bool {
    let Some(rest) = text.strip_prefix('&') else {
        return false;
    };
    let bytes = rest.as_bytes();
    let (skip, max, allowed): (usize, usize, fn(&u8) -> bool) = match bytes {
        [b'#', b'x' | b'X', ..] => (2, 6, u8::is_ascii_hexdigit),
        [b'#', ..] => (1, 7, u8::is_ascii_digit),
        _ => (0, 32, u8::is_ascii_alphanumeric),
    };
    let name = bytes[skip..].iter().take_while(|b| allowed(b)).count();
    name > 0 && name <= max && bytes.get(skip + name) == Some(&b';')
}

/// Writes a code span with a fence that is longer than any backtick run in
/// the code.
fn write_code(code: &str, table: bool) -> String {
    let code = code.replace('\n', " ");
    let mut longest = 0;
    let mut current = 0;
    for ch in code.chars() {
        current = if ch == '`' { current + 1 } else { 0 };
        longest = longest.max(current);
    }
    let fence = "`".repeat(longest + 1);
    // The parser strips one space on each side if both are there.
    let pad = code.starts_with('`')
        || code.ends_with('`')
        || (code.starts_with(' ') && code.ends_with(' ') && !code.trim_matches(' ').is_empty());
    let code = if table {
        code.replace('|', "\\|")
    } else {
        code
    };
    let pad = if pad { " " } else { "" };
    format!("{fence}{pad}{code}{pad}{fence}")
}

/// Returns `true` if `<url>` is read as an autolink with this very URL.
fn is_autolink(url: &str) -> bool {
    if url
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '<' | '>' | '\\' | '&'))
    {
        return false;
    }
    if let Some((scheme, _)) = url.split_once(':') {
        let bytes = scheme.as_bytes();
        return (2..=32).contains(&bytes.len())
            && bytes[0].is_ascii_alphabetic()
            && bytes
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'.' | b'-'));
    }
    // An email address.
    let Some((local, domain)) = url.split_once('@') else {
        return false;
    };
    let label = |label: &str| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    };
    !local.is_empty()
        && local
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".!#$%&'*+/=?^_`{|}~-".contains(&b))
        && domain.split('.').all(label)
}

/// Appends `text` with a backslash in front of every character for which
/// `special` holds, in front of `&` that starts a character reference and in
/// front of a backslash that would otherwise escape its neighbour.
fn escape_with(text: &str, special: impl Fn(char) -> bool) -> String {
    let mut out = String::with_capacity(text.len() + 4);
    let mut chars = text.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        let next = chars.peek().map(|(_, next)| *next);
        let escape = match ch {
            '\\' => next.is_none_or(|c| c.is_ascii_punctuation()),
            '&' => is_entity_like(&text[index..]),
            _ => special(ch),
        };
        if escape {
            out.push('\\');
        }
        out.push(if ch == '\n' { ' ' } else { ch });
    }
    out
}

/// Writes a link destination.
pub(crate) fn write_destination(url: &str) -> String {
    let mut depth = 0i32;
    let mut balanced = true;
    for ch in url.chars() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                balanced &= depth >= 0;
            }
            _ => {}
        }
    }
    let plain = !url.is_empty()
        && !url.starts_with('<')
        && balanced
        && depth == 0
        && !url.chars().any(|c| c.is_whitespace() || c.is_control());
    if plain {
        escape_with(url, |_| false)
    } else {
        format!("<{}>", escape_with(url, |c| matches!(c, '<' | '>')))
    }
}

/// Writes a link title with its quotes.
pub(crate) fn write_title(title: &str) -> String {
    format!("\"{}\"", escape_with(title, |c| c == '"'))
}

/// Writes the alternative text of an image.
pub(crate) fn write_alt(alt: &str) -> String {
    let mut out = String::with_capacity(alt.len() + 4);
    let mut prev: Option<char> = None;
    let mut chars = alt.char_indices().peekable();
    while let Some((index, ch)) = chars.next() {
        let next = chars.peek().map(|(_, next)| *next);
        let escape = match ch {
            '\\' => next.is_none_or(|c| c.is_ascii_punctuation()),
            '&' => is_entity_like(&alt[index..]),
            '[' | ']' | '`' | '*' | '~' | '$' | '<' => true,
            '_' => {
                !(prev.is_some_and(char::is_alphanumeric)
                    && next.is_some_and(char::is_alphanumeric))
            }
            _ => false,
        };
        if escape {
            out.push('\\');
        }
        out.push(if ch == '\n' { ' ' } else { ch });
        prev = Some(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_fences_grow_with_backticks() {
        assert_eq!(write_code("a", false), "`a`");
        assert_eq!(write_code("a`b", false), "``a`b``");
        assert_eq!(write_code("`", false), "`` ` ``");
        assert_eq!(write_code(" a ", false), "`  a  `");
        assert_eq!(write_code("  ", false), "`  `");
        assert_eq!(write_code("a|b", true), "`a\\|b`");
    }

    #[test]
    fn destinations() {
        assert_eq!(write_destination("a.md"), "a.md");
        assert_eq!(write_destination("a b.md"), "<a b.md>");
        assert_eq!(write_destination(""), "<>");
        assert_eq!(write_destination("w/Foo_(bar)"), "w/Foo_(bar)");
        assert_eq!(write_destination("a)b"), "<a)b>");
        assert_eq!(write_destination("a\\*b"), "a\\\\*b");
        assert_eq!(write_destination("?a=1&amp;b=2"), "?a=1\\&amp;b=2");
        assert_eq!(write_title("say \"hi\""), "\"say \\\"hi\\\"\"");
    }

    #[test]
    fn autolinks() {
        assert!(is_autolink("https://example.com/a?b=c"));
        assert!(is_autolink("mailto:me@example.com"));
        assert!(is_autolink("me@example.com"));
        assert!(!is_autolink("example.com"));
        assert!(!is_autolink("https://example.com/a b"));
        assert!(!is_autolink("a.md"));
    }

    #[test]
    fn entities() {
        assert!(is_entity_like("&amp; x"));
        assert!(is_entity_like("&#38;"));
        assert!(is_entity_like("&#x26;"));
        assert!(!is_entity_like("& x"));
        assert!(!is_entity_like("&amp x"));
        assert!(!is_entity_like("&#xZZ;"));
    }

    #[test]
    fn line_starts() {
        assert_eq!(line_start_escape("# a", true, true), Some(0));
        assert_eq!(line_start_escape("#a", true, true), None);
        assert_eq!(line_start_escape("#", true, true), Some(0));
        assert_eq!(line_start_escape("#", false, true), None);
        assert_eq!(line_start_escape("- a", true, true), Some(0));
        assert_eq!(line_start_escape("-a", true, true), None);
        assert_eq!(line_start_escape("---", true, true), Some(0));
        assert_eq!(line_start_escape("--", true, true), None);
        assert_eq!(line_start_escape("--", true, false), Some(0));
        assert_eq!(line_start_escape("===", true, true), None);
        assert_eq!(line_start_escape("===", true, false), Some(0));
        assert_eq!(line_start_escape("___", true, true), Some(0));
        assert_eq!(line_start_escape("12. a", true, true), Some(2));
        assert_eq!(line_start_escape("12.a", true, true), None);
        assert_eq!(line_start_escape("1)", true, true), Some(1));
        assert_eq!(line_start_escape("> a", true, true), Some(0));
        assert_eq!(line_start_escape("[a]: b", true, true), Some(0));
        assert_eq!(line_start_escape("[a] b", true, true), None);
        assert_eq!(line_start_escape("|--|--|", true, false), Some(0));
        assert_eq!(line_start_escape("|--|--|", true, true), None);
        assert_eq!(line_start_escape("~~~", true, true), Some(0));
        assert_eq!(line_start_escape("plain", true, true), None);
    }
}
