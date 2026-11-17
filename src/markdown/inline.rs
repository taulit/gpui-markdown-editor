//! Builds [`Inline`] content from parser events.

use crate::extension::{Extensions, InlineMatch};
use crate::model::{Inline, Mark, MarkSpan};

/// How much of a piece of text was written with escapes in the source.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Escaped {
    /// Plain text.
    None,
    /// The first character follows a backslash.
    First,
    /// The text was decoded from a character reference.
    All,
}

/// Collects the matches of all extensions in one run of literal text.
///
/// `escaped` holds the sorted byte offsets of characters that were escaped in
/// the source. A match that starts at one of them is dropped, which is how a
/// writer keeps text such as `\#tag` literal. Earlier extensions win when
/// matches overlap. The result is sorted by position.
pub(crate) fn extension_matches(
    extensions: &Extensions,
    run: &str,
    escaped: &[usize],
) -> Vec<InlineMatch> {
    let mut accepted: Vec<InlineMatch> = Vec::new();
    for extension in extensions.iter() {
        let mut fresh = Vec::new();
        let mut last_end = 0;
        for found in extension.parse_inline(run) {
            let range = found.range.clone();
            let valid = range.start < range.end
                && range.start >= last_end
                && range.end <= run.len()
                && run.is_char_boundary(range.start)
                && run.is_char_boundary(range.end)
                && !found.text.is_empty()
                && escaped.binary_search(&range.start).is_err();
            if !valid {
                continue;
            }
            last_end = range.end;
            let next = accepted.partition_point(|a| a.range.end <= range.start);
            if accepted
                .get(next)
                .is_some_and(|a| a.range.start < range.end)
            {
                continue;
            }
            fresh.push(found);
        }
        if !fresh.is_empty() {
            accepted.append(&mut fresh);
            accepted.sort_by_key(|m| m.range.start);
        }
    }
    accepted
}

/// Sorts spans and merges spans of the same mark that touch or overlap.
///
/// Links, raw spans and custom marks are units of their own (two links or
/// two HTML tags in a row stay two spans) and are never merged.
pub(crate) fn normalize_marks(marks: &mut Vec<MarkSpan>) {
    if marks.len() < 2 {
        return;
    }
    marks.sort_by_key(|m| m.range.start);
    let mut out: Vec<MarkSpan> = Vec::with_capacity(marks.len());
    // The most recent span per mark kind is the only merge candidate.
    let mut last: [Option<usize>; 6] = [None; 6];
    for span in marks.drain(..) {
        let slot = match span.mark {
            Mark::Bold => 0,
            Mark::Italic => 1,
            Mark::Underline => 2,
            Mark::Strike => 3,
            Mark::Code => 4,
            Mark::Highlight => 5,
            Mark::Link { .. } | Mark::Raw | Mark::Custom { .. } => {
                out.push(span);
                continue;
            }
        };
        if let Some(index) = last[slot] {
            let previous = &mut out[index];
            if previous.mark == span.mark && span.range.start <= previous.range.end {
                previous.range.end = previous.range.end.max(span.range.end);
                continue;
            }
        }
        last[slot] = Some(out.len());
        out.push(span);
    }
    // Spans that start together are listed from the outside in.
    out.sort_by(|a, b| {
        (a.range.start, b.range.end, mark_rank(&a.mark)).cmp(&(
            b.range.start,
            a.range.end,
            mark_rank(&b.mark),
        ))
    });
    *marks = out;
}

fn mark_rank(mark: &Mark) -> u8 {
    match mark {
        Mark::Link { .. } => 0,
        Mark::Bold => 1,
        Mark::Italic => 2,
        Mark::Strike => 3,
        Mark::Underline => 4,
        Mark::Highlight => 5,
        Mark::Custom { .. } => 6,
        Mark::Code => 7,
        Mark::Raw => 8,
    }
}

const UNDERLINE_OPEN: &str = "<u>";

/// Accumulates the text and marks of one block.
pub(crate) struct InlineBuilder<'e> {
    extensions: &'e Extensions,
    text: String,
    marks: Vec<MarkSpan>,
    /// Start of the current run of literal text in `text`.
    run_start: usize,
    /// Offsets in `text` of escaped characters of the current run.
    escaped: Vec<usize>,
    open: Vec<(Mark, usize)>,
    /// Offsets in `text` of `<u>` tags that are still open.
    underline: Vec<usize>,
}

impl<'e> InlineBuilder<'e> {
    pub(crate) fn new(extensions: &'e Extensions) -> Self {
        Self {
            extensions,
            text: String::new(),
            marks: Vec::new(),
            run_start: 0,
            escaped: Vec::new(),
            open: Vec::new(),
            underline: Vec::new(),
        }
    }

    /// Appends literal text.
    pub(crate) fn text(&mut self, text: &str, escaped: Escaped) {
        if !self.extensions.is_empty() {
            match escaped {
                Escaped::None => {}
                Escaped::First => self.escaped.push(self.text.len()),
                Escaped::All => {
                    let base = self.text.len();
                    self.escaped
                        .extend(text.char_indices().map(|(i, _)| base + i));
                }
            }
        }
        self.text.push_str(text);
    }

    /// Appends a soft line break, stored as a space.
    pub(crate) fn soft_break(&mut self) {
        self.text.push(' ');
    }

    /// Appends a hard line break.
    pub(crate) fn hard_break(&mut self) {
        self.text.push('\n');
    }

    /// Starts a mark at the current position.
    pub(crate) fn open(&mut self, mark: Mark) {
        self.flush_run();
        self.open.push((mark, self.text.len()));
    }

    /// Ends the innermost open mark. Returns `false` if it covered no text.
    pub(crate) fn close(&mut self) -> bool {
        self.flush_run();
        match self.open.pop() {
            Some((mark, start)) if start < self.text.len() => {
                self.marks.push(MarkSpan {
                    range: start..self.text.len(),
                    mark,
                });
                true
            }
            _ => false,
        }
    }

    /// Appends text covered by `mark` that takes no part in extension syntax.
    fn atom(&mut self, text: &str, mark: Mark) {
        self.flush_run();
        if text.is_empty() {
            return;
        }
        let start = self.text.len();
        self.text.push_str(text);
        self.marks.push(MarkSpan {
            range: start..self.text.len(),
            mark,
        });
        self.run_start = self.text.len();
    }

    /// Appends a code span.
    pub(crate) fn code(&mut self, text: &str) {
        self.atom(text, Mark::Code);
    }

    /// Appends source text that is kept verbatim.
    pub(crate) fn raw(&mut self, text: &str) {
        self.atom(text, Mark::Raw);
    }

    /// Handles `<u>`. The tag is kept as raw text until its `</u>` shows up.
    pub(crate) fn underline_open(&mut self) {
        // Flushing can change the length of the text in front.
        self.flush_run();
        let start = self.text.len();
        self.raw(UNDERLINE_OPEN);
        self.underline.push(start);
    }

    /// Handles `</u>`. Returns `false` if no `<u>` is open.
    pub(crate) fn underline_close(&mut self) -> bool {
        self.flush_run();
        let Some(start) = self.underline.pop() else {
            return false;
        };
        let removed = start..start + UNDERLINE_OPEN.len();
        let Some(raw_index) = self
            .marks
            .iter()
            .rposition(|span| span.mark == Mark::Raw && span.range == removed)
        else {
            return false;
        };
        self.marks.remove(raw_index);
        self.text.replace_range(removed.clone(), "");
        let shift = |offset: usize| {
            if offset >= removed.end {
                offset - removed.len()
            } else {
                offset.min(removed.start)
            }
        };
        // Only spans that were added after the tag can reach behind it.
        let mut later = self.marks.split_off(raw_index);
        for span in &mut later {
            span.range = shift(span.range.start)..shift(span.range.end);
        }
        later.retain(|span| !span.range.is_empty());
        self.marks.append(&mut later);
        for (_, open_start) in self.open.iter_mut().rev() {
            if *open_start <= removed.start {
                break;
            }
            *open_start = shift(*open_start);
        }
        self.run_start = self.text.len();
        if start < self.text.len() {
            self.marks.push(MarkSpan {
                range: start..self.text.len(),
                mark: Mark::Underline,
            });
        }
        true
    }

    /// Ends the current run of literal text and applies extension syntax.
    fn flush_run(&mut self) {
        if !self.extensions.is_empty() && self.run_start < self.text.len() {
            let start = self.run_start;
            let escaped: Vec<usize> = self.escaped.iter().map(|offset| offset - start).collect();
            let matches = extension_matches(self.extensions, &self.text[start..], &escaped);
            if !matches.is_empty() {
                let run = self.text.split_off(start);
                let mut cursor = 0;
                for found in matches {
                    self.text.push_str(&run[cursor..found.range.start]);
                    let mark_start = self.text.len();
                    self.text.push_str(&found.text);
                    self.marks.push(MarkSpan {
                        range: mark_start..self.text.len(),
                        mark: found.mark,
                    });
                    cursor = found.range.end;
                }
                self.text.push_str(&run[cursor..]);
            }
        }
        self.run_start = self.text.len();
        self.escaped.clear();
    }

    /// Returns the finished content.
    pub(crate) fn finish(mut self) -> Inline {
        self.flush_run();
        while !self.open.is_empty() {
            self.close();
        }
        normalize_marks(&mut self.marks);
        Inline {
            text: self.text,
            marks: self.marks,
        }
    }
}
