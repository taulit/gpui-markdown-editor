//! Container prefixes of nested blocks: `> ` of quotes and the indentation of
//! list items.
//!
//! Text of nested blocks is stored without these prefixes. The parser uses
//! this module to remove them from source lines that it keeps verbatim (raw
//! blocks and raw inline spans inside quotes and list items).

use std::ops::Range;

/// An open quote or list item while parsing.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Container {
    /// `true` for a quote, `false` for a list item.
    pub quote: bool,
    /// Start of the line that holds the container's marker.
    pub line_start: usize,
    /// Byte offset where the content starts on the marker line.
    pub content_byte: usize,
    /// Column of `content_byte`, with tabs expanded to stops of four.
    pub content_col: usize,
    /// Columns of indentation that continuation lines of a list item carry.
    pub width: usize,
}

/// A position in a line that can stand in the middle of a tab.
#[derive(Clone, Copy)]
struct Cursor {
    pos: usize,
    col: usize,
    /// Columns of a partially consumed tab that are still ahead.
    virt: usize,
    /// The line lacks the prefix of a container: it is a lazy continuation
    /// line (or blank).
    lazy: bool,
}

impl Cursor {
    /// Consumes one column of whitespace. Returns `false` at anything else.
    fn take_space(&mut self, bytes: &[u8], limit: usize) -> bool {
        if self.virt > 0 {
            self.virt -= 1;
            return true;
        }
        if self.pos >= limit {
            return false;
        }
        match bytes.get(self.pos) {
            Some(b' ') => {
                self.pos += 1;
                self.col += 1;
                true
            }
            Some(b'\t') => {
                let width = 4 - self.col % 4;
                self.pos += 1;
                self.col += width;
                self.virt = width - 1;
                true
            }
            _ => false,
        }
    }
}

/// Returns the start of the line that contains `pos`.
pub(crate) fn line_start(source: &str, pos: usize) -> usize {
    let pos = pos.min(source.len());
    source.as_bytes()[..pos]
        .iter()
        .rposition(|b| matches!(b, b'\n' | b'\r'))
        .map_or(0, |i| i + 1)
}

/// Returns the end of the line that starts at or contains `pos`, without the
/// line terminator.
fn line_end(source: &str, pos: usize) -> usize {
    let bytes = source.as_bytes();
    bytes[pos.min(bytes.len())..]
        .iter()
        .position(|b| matches!(b, b'\n' | b'\r'))
        .map_or(bytes.len(), |i| pos + i)
}

/// Walks over the prefixes of `containers` on the line starting at
/// `line_start` and returns the position behind them.
fn skip_prefix(source: &str, containers: &[Container], line_start: usize, limit: usize) -> Cursor {
    let bytes = source.as_bytes();
    let mut cursor = Cursor {
        pos: line_start,
        col: 0,
        virt: 0,
        lazy: false,
    };
    for container in containers {
        if container.line_start == line_start {
            // The container starts on this very line: continue behind its marker.
            cursor = Cursor {
                pos: container.content_byte.min(limit),
                col: container.content_col,
                virt: 0,
                lazy: false,
            };
            continue;
        }
        if container.quote {
            let saved = cursor;
            let mut spaces = 0;
            while spaces < 3 && cursor.take_space(bytes, limit) {
                spaces += 1;
            }
            if cursor.virt == 0 && cursor.pos < limit && bytes.get(cursor.pos) == Some(&b'>') {
                cursor.pos += 1;
                cursor.col += 1;
                cursor.take_space(bytes, limit);
            } else {
                cursor = saved;
                cursor.lazy = true;
                break;
            }
        } else {
            let mut columns = 0;
            while columns < container.width && cursor.take_space(bytes, limit) {
                columns += 1;
            }
            if columns < container.width {
                cursor.lazy = true;
                break;
            }
        }
    }
    cursor
}

/// Describes the list item whose marker starts at `marker`. Returns the
/// container and the offset behind the marker.
pub(crate) fn item_container(
    source: &str,
    containers: &[Container],
    line_start: usize,
    marker: usize,
) -> (Container, usize) {
    let bytes = source.as_bytes();
    let eol = line_end(source, marker);
    let mut cursor = skip_prefix(source, containers, line_start, marker);
    let base_col = cursor.col - cursor.virt;
    while cursor.pos < marker && cursor.take_space(bytes, marker) {}
    let marker_col = cursor.col + marker.saturating_sub(cursor.pos);

    let marker_len = match bytes.get(marker) {
        Some(b'-' | b'+' | b'*') => 1,
        Some(b'0'..=b'9') => {
            let digits = bytes[marker..eol]
                .iter()
                .take_while(|b| b.is_ascii_digit())
                .count();
            match bytes.get(marker + digits) {
                Some(b'.' | b')') => digits + 1,
                _ => 0,
            }
        }
        _ => 0,
    };
    let marker_end = (marker + marker_len).min(eol);
    let after_marker = Cursor {
        pos: marker_end,
        col: marker_col + marker_len,
        virt: 0,
        lazy: false,
    };
    let mut probe = after_marker;
    let mut spaces = 0;
    while probe.take_space(bytes, eol) {
        spaces += 1;
    }
    let blank = probe.pos >= eol;
    let content = if !blank && (1..=4).contains(&spaces) {
        Cursor { virt: 0, ..probe }
    } else {
        // An empty first line, or an indented code block behind the marker:
        // exactly one column belongs to the marker.
        let mut one = after_marker;
        one.take_space(bytes, eol);
        Cursor {
            pos: one.pos,
            col: after_marker.col + 1,
            virt: 0,
            lazy: false,
        }
    };
    let container = Container {
        quote: false,
        line_start,
        content_byte: content.pos,
        content_col: content.col,
        width: content.col.saturating_sub(base_col).max(1),
    };
    (container, marker_end)
}

/// Describes the quote whose `>` is at `marker`.
pub(crate) fn quote_container(
    source: &str,
    containers: &[Container],
    line_start: usize,
    marker: usize,
) -> Container {
    let bytes = source.as_bytes();
    let mut cursor = skip_prefix(source, containers, line_start, marker);
    while cursor.pos < marker && cursor.take_space(bytes, marker) {}
    let marker_col = cursor.col + marker.saturating_sub(cursor.pos);
    let has_marker = bytes.get(marker) == Some(&b'>');
    let mut content_byte = marker + usize::from(has_marker);
    let mut content_col = marker_col + usize::from(has_marker);
    if bytes.get(content_byte) == Some(&b' ') {
        content_byte += 1;
        content_col += 1;
    }
    Container {
        quote: true,
        line_start,
        content_byte,
        content_col,
        width: 0,
    }
}

/// Iterates over the lines of `range` as `(start, end)` offsets without line
/// terminators. `\n`, `\r\n` and `\r` end a line.
fn lines(source: &str, range: Range<usize>) -> impl Iterator<Item = (usize, usize)> + '_ {
    let bytes = source.as_bytes();
    let end = range.end.min(bytes.len());
    let mut pos = range.start.min(end);
    let mut done = false;
    std::iter::from_fn(move || {
        if done {
            return None;
        }
        let start = pos;
        while pos < end && !matches!(bytes[pos], b'\n' | b'\r') {
            pos += 1;
        }
        let line = (start, pos);
        if pos >= end {
            done = true;
        } else if bytes[pos] == b'\r' && pos + 1 < end && bytes[pos + 1] == b'\n' {
            pos += 2;
        } else {
            pos += 1;
        }
        Some(line)
    })
}

/// Returns the source text of `range` with `\n` line endings and without the
/// prefixes of `containers` on continuation lines. Lines that consist of
/// whitespace come out empty.
pub(crate) fn strip(source: &str, containers: &[Container], range: Range<usize>) -> String {
    let Some(text) = source.get(range.clone()) else {
        return String::new();
    };
    let mut out = String::with_capacity(text.len());
    let bytes = source.as_bytes();
    for (index, (start, end)) in lines(source, range.clone()).enumerate() {
        if index > 0 {
            out.push('\n');
        }
        let at_line_start = start == 0 || matches!(bytes[start - 1], b'\n' | b'\r');
        if containers.is_empty() || !at_line_start {
            out.push_str(&source[start..end]);
            continue;
        }
        let cursor = skip_prefix(source, containers, start, end);
        // Indentation is written with spaces: a tab would change its width
        // when the line is put behind another prefix.
        let mut pos = cursor.pos.min(end);
        let mut col = cursor.col;
        let mut indent = cursor.virt;
        while pos < end {
            let width = match bytes[pos] {
                b' ' => 1,
                b'\t' => 4 - col % 4,
                _ => break,
            };
            indent += width;
            col += width;
            pos += 1;
        }
        if pos < end {
            // A lazy line continues a paragraph only as long as it cannot be
            // taken for a block of its own. Indented like code it never can,
            // also not inside a footnote, which takes four columns itself.
            let indent = if cursor.lazy { indent.max(8) } else { indent };
            out.extend(std::iter::repeat_n(' ', indent));
            out.push_str(source.get(pos..end).unwrap_or(""));
        }
    }
    out
}

/// Returns the text of a raw block: `text` with `\n` line endings and
/// without the indentation of its first line, which is taken off the other
/// lines as far as they have it and are not indented like code. Indentation in front of a raw block would
/// make it part of a list item written above it.
pub(crate) fn raw_block_text(text: &str) -> String {
    let text = normalize_newlines(text);
    let indent = text.len() - text.trim_start_matches(' ').len();
    if indent == 0 {
        return text;
    }
    let mut out = String::with_capacity(text.len());
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        let own = line.len() - line.trim_start_matches(' ').len();
        // Four columns of indentation mean something (a continuation line
        // that is no block of its own) and must not shrink below that.
        let cut = if own >= 4 { 0 } else { own.min(indent) };
        out.push_str(&line[cut..]);
    }
    out
}

/// Replaces `\r\n` and lone `\r` with `\n`.
pub(crate) fn normalize_newlines(text: &str) -> String {
    if !text.contains('\r') {
        return text.to_string();
    }
    text.replace("\r\n", "\n").replace('\r', "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quote(source: &str, containers: &[Container], marker: usize) -> Container {
        quote_container(source, containers, line_start(source, marker), marker)
    }

    fn item(source: &str, containers: &[Container], marker: usize) -> Container {
        item_container(source, containers, line_start(source, marker), marker).0
    }

    #[test]
    fn strips_quote_markers() {
        let source = "> <div>\n> x\n>\n>y\n";
        let stack = [quote(source, &[], 0)];
        assert_eq!(strip(source, &stack, 2..16), "<div>\nx\n\ny");
    }

    #[test]
    fn strips_item_indentation() {
        let source = "1. a\n\n   <div>\n     x\n   </div>\n";
        let stack = [item(source, &[], 0)];
        assert_eq!(stack[0].width, 3);
        assert_eq!(strip(source, &stack, 9..31), "<div>\n  x\n</div>");
    }

    #[test]
    fn strips_nested_containers_on_one_line() {
        let source = "> - a\n>   <b>\n>   c\n";
        let outer = quote(source, &[], 0);
        let inner = item(source, &[outer], 2);
        assert_eq!(inner.width, 2);
        assert_eq!(strip(source, &[outer, inner], 10..19), "<b>\nc");
    }

    #[test]
    fn expands_tabs_partially() {
        let source = "- a\n\t- b\n\n\t\tcode\n";
        let outer = item(source, &[], 0);
        let inner = item(source, &[outer], 5);
        assert_eq!(inner.width, 4);
        // Two tabs are eight columns, six of them belong to the two items.
        assert_eq!(strip(source, &[outer, inner], 10..16), "  code");
    }

    #[test]
    fn keeps_lazy_lines() {
        let source = "> a\nlazy\n";
        let stack = [quote(source, &[], 0)];
        assert_eq!(strip(source, &stack, 2..8), "a\n        lazy");
    }

    #[test]
    fn dedents_raw_blocks() {
        assert_eq!(
            raw_block_text("  <div>\r\n   x\r\n </div>"),
            "<div>\n x\n</div>"
        );
        assert_eq!(
            raw_block_text("  > [!note]\n    * * *"),
            "> [!note]\n    * * *"
        );
        assert_eq!(raw_block_text("<div>\n  x"), "<div>\n  x");
    }

    #[test]
    fn normalizes_line_endings() {
        assert_eq!(normalize_newlines("a\r\nb\rc\n"), "a\nb\nc\n");
        assert_eq!(strip("a\r\nb", &[], 0..4), "a\nb");
    }
}
