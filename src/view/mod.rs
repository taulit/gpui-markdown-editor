//! Rendering of a [`Document`](crate::Document): layout, painting and hit
//! testing.
//!
//! - [`DocumentView`] is the state the editor keeps: layout cache, block
//!   heights, scroll position. It answers the geometry queries through
//!   [`DocumentView::query`].
//! - [`DocumentElement`] paints a document with a view. It reads what it
//!   needs from its source entity through [`ViewSource`] at prepaint time.
//!
//! Points passed in and bounds returned are in window coordinates, like the
//! positions of mouse events.

// The geometry queries are the interface for the input wiring, which is not
// connected yet; the preview mode uses only some of them.
#![allow(dead_code)]

mod element;
mod geometry;
mod layout;
mod paint;
mod scroll;
mod state;
mod text;

#[cfg(test)]
mod tests;

use std::cell::{RefCell, RefMut};
use std::ops::Range;
use std::rc::Rc;

use gpui::{Pixels, WindowTextSystem};

use crate::edit::Selection;
use crate::model::{BlockId, Document, TextRange};
use crate::style::EditorStyle;

pub(crate) use element::DocumentElement;
pub(crate) use geometry::ViewQuery;
pub(crate) use state::ViewStats;

/// What layout depends on.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LayoutInput<'a> {
    pub document: &'a Document,
    pub style: &'a EditorStyle,
    /// [`EditorOptions::first_line_is_title`](crate::EditorOptions::first_line_is_title).
    pub first_line_is_title: bool,
    /// Width of the content column including the side padding.
    pub max_width: Option<Pixels>,
}

/// Everything the element paints.
#[derive(Clone, Debug)]
pub(crate) struct ViewInput<'a> {
    pub layout: LayoutInput<'a>,
    pub selection: Option<&'a Selection>,
    /// Extra backgrounds, for example search hits.
    pub highlights: &'a [TextRange],
    /// Whether the caret is drawn in this frame (blinking is up to the caller).
    pub caret_visible: bool,
    /// Text being composed by an input method, drawn underlined.
    pub marked: Option<(BlockId, Range<usize>)>,
}

/// An entity a [`DocumentElement`] can render.
pub(crate) trait ViewSource: 'static {
    fn view_input(&self) -> ViewInput<'_>;
}

/// The view state of one editor, shared by the editor and its element.
#[derive(Clone, Default)]
pub(crate) struct DocumentView {
    state: Rc<RefCell<state::ViewState>>,
}

impl std::fmt::Debug for DocumentView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DocumentView").finish_non_exhaustive()
    }
}

impl DocumentView {
    pub fn new() -> Self {
        Self::default()
    }

    /// Drops the layouts of the given blocks or cells; call after a
    /// transaction with the ids of the changed blocks.
    pub fn invalidate(&self, ids: impl IntoIterator<Item = BlockId>) {
        self.state.borrow_mut().invalidate(ids);
    }

    /// Drops all layouts, keeping the scroll position.
    pub fn invalidate_all(&self) {
        self.state.borrow_mut().invalidate_all();
    }

    /// Prepares for a different document: drops all layouts and heights and
    /// scrolls to the top.
    pub fn reset(&self) {
        self.state.borrow_mut().reset();
    }

    /// Scrolls `range` into view at the next frame (works before the first
    /// frame, when the size of the view is not known yet).
    pub fn request_reveal(&self, range: TextRange) {
        self.state.borrow_mut().pending_reveal = Some(range);
    }

    /// Opens the geometry queries for `input`. Lays out blocks on demand with
    /// `ts` (`window.text_system()`).
    pub fn query<'a>(&'a self, input: LayoutInput<'a>, ts: &'a WindowTextSystem) -> ViewQuery<'a> {
        let mut state = self.state.borrow_mut();
        state.sync(&input);
        ViewQuery::new(state, input, ts)
    }

    /// Counters for tests and timing.
    pub fn stats(&self) -> ViewStats {
        self.state.borrow().stats()
    }

    /// Lays out the blocks around the viewport for `bounds` and builds the
    /// paint list of a frame.
    fn prepare_frame(
        &self,
        input: &ViewInput<'_>,
        bounds: gpui::Bounds<Pixels>,
        ts: &WindowTextSystem,
    ) -> paint::Frame {
        let mut state = self.state();
        state.bounds = bounds;
        state.sync(&input.layout);
        if let Some(range) = state.pending_reveal.take() {
            drop(state);
            self.query(input.layout, ts).scroll_to_reveal(range);
            state = self.state();
        }
        state.layout_visible(&input.layout, ts);
        paint::build_frame(&mut state, input, ts)
    }

    fn state(&self) -> RefMut<'_, state::ViewState> {
        self.state.borrow_mut()
    }
}
