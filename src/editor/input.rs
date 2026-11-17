//! Text input and IME through GPUI's [`EntityInputHandler`].
//!
//! Platform offsets are UTF-16 offsets into the text node that holds the
//! caret. [`EditorState`](crate::edit::EditorState) clamps every incoming
//! range and drops a marked range that no longer fits, so a stale range from
//! the input method (marked text deleted behind its back) cannot slice out
//! of bounds, see `docs/verified.md`.

use std::ops::Range;
use std::time::Instant;

use gpui::{Bounds, Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window};

use super::MarkdownEditor;
use crate::edit::ime::byte_to_utf16;

impl EntityInputHandler for MarkdownEditor {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let (text, actual) = self.state.text_for_range_utf16(range)?;
        *adjusted_range = Some(actual);
        Some(text)
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let (range, reversed) = self.state.selected_range_utf16()?;
        Some(UTF16Selection { range, reversed })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.state.marked_range_utf16()
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.state.unmark();
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state
            .replace_text_in_range_utf16(range, text, Instant::now());
        self.maybe_open_slash(text);
        self.maybe_open_completion(text);
        self.after_change(false, window, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state
            .replace_and_mark_utf16(range, new_text, new_selected_range, Instant::now());
        self.after_change(false, window, cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _element_bounds: Bounds<Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let (node, range) = self.state.utf16_range_to_bytes(&range_utf16)?;
        let ts = window.text_system().clone();
        let view = self.view.clone();
        let mut q = view.query(self.layout_input(), &ts);
        q.bounds_for_range(node, range)
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let head = self.state.selection.head()?;
        let ts = window.text_system().clone();
        let view = self.view.clone();
        let pos = view.query(self.layout_input(), &ts).position_at(point)?;
        if pos.block != head.block {
            return None;
        }
        let text = &self.state.text_of(pos.block)?.text;
        Some(byte_to_utf16(text, pos.offset))
    }
}
