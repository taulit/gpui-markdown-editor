//! IME support: the marked (preedit) range and UTF-16 offsets.
//!
//! Platform input methods speak UTF-16 offsets into "the text", which for
//! this editor is the text node that holds the caret. Every incoming range
//! is clamped to that text and to `char` boundaries, and a marked range that
//! no longer fits (its text was deleted, the caret moved to another node) is
//! dropped instead of being trusted.

use std::ops::Range;
use std::time::Instant;

use super::Selection;
use super::inline_ops::floor_boundary;
use super::state::EditorState;
use crate::model::{BlockId, TextPos};

/// The byte offset of the UTF-16 offset `utf16` in `text`, clamped to the
/// text. An offset inside a surrogate pair maps to the start of the char.
pub(crate) fn utf16_to_byte(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (index, ch) in text.char_indices() {
        let next = units + ch.len_utf16();
        if next > utf16 {
            return index;
        }
        units = next;
    }
    text.len()
}

/// The UTF-16 offset of the byte offset `byte`, clamped to the text and to
/// a `char` boundary.
pub(crate) fn byte_to_utf16(text: &str, byte: usize) -> usize {
    let byte = floor_boundary(text, byte);
    text[..byte].chars().map(char::len_utf16).sum()
}

fn range_to_bytes(text: &str, range: &Range<usize>) -> Range<usize> {
    let start = utf16_to_byte(text, range.start.min(range.end));
    let end = utf16_to_byte(text, range.end.max(range.start));
    start..end
}

impl EditorState {
    /// The node the IME works in: the one holding the caret.
    fn ime_node(&self) -> Option<(BlockId, String)> {
        let head = self.selection.head()?;
        let text = self.text_of(head.block)?.text.clone();
        Some((head.block, text))
    }

    /// The marked range if it still fits the caret's node.
    pub(crate) fn valid_marked(&self) -> Option<(BlockId, Range<usize>)> {
        let (node, range) = self.marked.clone()?;
        let head = self.selection.head()?;
        let text = &self.text_of(head.block)?.text;
        let fits = head.block == node
            && range.start <= range.end
            && range.end <= text.len()
            && text.is_char_boundary(range.start)
            && text.is_char_boundary(range.end);
        fits.then_some((node, range))
    }

    /// Drops the marked range if it no longer fits the caret's node.
    fn validate_marked(&mut self) {
        if self.valid_marked().is_none() {
            self.marked = None;
        }
    }

    /// UTF-16 offset in the caret's node to a byte offset.
    pub(crate) fn utf16_to_offset(&self, utf16: usize) -> usize {
        self.ime_node()
            .map_or(0, |(_, text)| utf16_to_byte(&text, utf16))
    }

    /// Byte offset in the caret's node to a UTF-16 offset.
    pub(crate) fn offset_to_utf16(&self, offset: usize) -> usize {
        self.ime_node()
            .map_or(0, |(_, text)| byte_to_utf16(&text, offset))
    }

    /// The marked range in UTF-16 offsets of the caret's node.
    pub(crate) fn marked_range_utf16(&self) -> Option<Range<usize>> {
        let (_, range) = self.valid_marked()?;
        let (_, text) = self.ime_node()?;
        Some(byte_to_utf16(&text, range.start)..byte_to_utf16(&text, range.end))
    }

    /// The UTF-16 range `range` of the caret's node in bytes, clamped.
    pub(crate) fn utf16_range_to_bytes(
        &self,
        range: &Range<usize>,
    ) -> Option<(BlockId, Range<usize>)> {
        let (node, text) = self.ime_node()?;
        Some((node, range_to_bytes(&text, range)))
    }

    /// The selection in UTF-16 offsets of the caret's node, and whether it
    /// is reversed. A selection across nodes is reported as the caret.
    pub(crate) fn selected_range_utf16(&self) -> Option<(Range<usize>, bool)> {
        let Selection::Text { anchor, head } = self.selection else {
            return None;
        };
        let (_, text) = self.ime_node()?;
        let head16 = byte_to_utf16(&text, head.offset);
        if anchor.block != head.block {
            return Some((head16..head16, false));
        }
        let anchor16 = byte_to_utf16(&text, anchor.offset);
        Some(if anchor16 <= head16 {
            (anchor16..head16, false)
        } else {
            (head16..anchor16, true)
        })
    }

    /// The text of a UTF-16 range of the caret's node, with the range that
    /// was actually used after clamping.
    pub(crate) fn text_for_range_utf16(
        &self,
        range: Range<usize>,
    ) -> Option<(String, Range<usize>)> {
        let (_, text) = self.ime_node()?;
        let bytes = range_to_bytes(&text, &range);
        let actual = byte_to_utf16(&text, bytes.start)..byte_to_utf16(&text, bytes.end);
        Some((text[bytes].to_string(), actual))
    }

    /// The range an IME call replaces: the given UTF-16 range, else the
    /// marked range, else the selection (if it lies in one node).
    fn ime_target(&mut self, range_utf16: Option<Range<usize>>) -> Option<(BlockId, Range<usize>)> {
        self.validate_marked();
        let (node, text) = self.ime_node()?;
        if let Some(range) = range_utf16 {
            return Some((node, range_to_bytes(&text, &range)));
        }
        if let Some(marked) = self.marked.clone() {
            return Some(marked);
        }
        match self.ordered()? {
            (start, end) if start.block == end.block => Some((node, start.offset..end.offset)),
            _ => None,
        }
    }

    /// Selects `range` of `node` before an IME edit, without the side
    /// effects of [`EditorState::set_selection`].
    fn select_for_ime(&mut self, node: BlockId, range: Range<usize>) {
        self.put_selection(Selection::Text {
            anchor: TextPos {
                block: node,
                offset: range.start,
            },
            head: TextPos {
                block: node,
                offset: range.end,
            },
        });
    }

    /// Commits text from the IME (or plain typing through the input
    /// handler): replaces the range and clears the marked range. Input
    /// rules run as for typing.
    pub(crate) fn replace_text_in_range_utf16(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        now: Instant,
    ) -> bool {
        self.validate_marked();
        let had_marked = self.marked.is_some();
        if let Some((node, range)) = self.ime_target(range_utf16) {
            self.select_for_ime(node, range);
        }
        self.marked = None;
        if text.is_empty() {
            return self.type_text("", now);
        }
        if had_marked {
            self.type_text(text, now)
        } else {
            self.insert_text(text, now)
        }
    }

    /// Sets preedit text: replaces the range (or the marked range, or the
    /// selection) with `text` and marks it. `selected_utf16` is the
    /// selection inside `text`; without it the caret goes behind the text.
    pub(crate) fn replace_and_mark_utf16(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        selected_utf16: Option<Range<usize>>,
        now: Instant,
    ) -> bool {
        let target = self.ime_target(range_utf16);
        if let Some((node, range)) = target {
            self.select_for_ime(node, range);
        }
        let changed = self.type_text(text, now);
        let Some(head) = self.selection.head() else {
            self.marked = None;
            return changed;
        };
        if text.is_empty() {
            self.marked = None;
            return changed;
        }
        let start = head.offset.saturating_sub(text.len());
        self.marked = Some((head.block, start..head.offset));
        if let Some(selected) = selected_utf16 {
            let inner = range_to_bytes(text, &selected);
            self.put_selection(Selection::Text {
                anchor: TextPos {
                    block: head.block,
                    offset: start + inner.start,
                },
                head: TextPos {
                    block: head.block,
                    offset: start + inner.end,
                },
            });
        }
        self.validate_marked();
        changed
    }

    /// Ends the composition and keeps the text.
    pub(crate) fn unmark(&mut self) {
        self.marked = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_utf16_offsets() {
        let text = "aä😀b";
        assert_eq!(utf16_to_byte(text, 0), 0);
        assert_eq!(utf16_to_byte(text, 2), 3);
        // Inside the surrogate pair of the emoji.
        assert_eq!(utf16_to_byte(text, 3), 3);
        assert_eq!(utf16_to_byte(text, 4), 7);
        assert_eq!(utf16_to_byte(text, 99), text.len());
        assert_eq!(byte_to_utf16(text, 7), 4);
        assert_eq!(byte_to_utf16(text, 5), 2);
        assert_eq!(byte_to_utf16(text, 99), 5);
    }
}
