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

mod element;
mod geometry;
mod layout;
mod paint;
mod scroll;
mod state;
mod table_ui;
mod text;

#[cfg(test)]
mod tests;

use std::cell::{RefCell, RefMut};
use std::ops::Range;
use std::rc::Rc;

use gpui::{
    Bounds, Context, EntityInputHandler, FocusHandle, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Window, WindowTextSystem,
};

use crate::edit::Selection;
use crate::model::{BlockId, Document, TextRange};
use crate::style::EditorStyle;

pub(crate) use element::DocumentElement;
pub(crate) use geometry::ViewQuery;
#[cfg(test)]
pub(crate) use state::ViewStats;
pub(crate) use table_ui::Affordance;

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
    /// The mouse position while it is over the editor, for the table
    /// affordances.
    pub hover: Option<gpui::Point<Pixels>>,
    /// The table handle whose menu is open, drawn highlighted.
    pub open_handle: Option<Affordance>,
}

/// An entity a [`DocumentElement`] can render. The element routes text
/// input to it while `focus_handle` is focused, and mouse events over the
/// document (and, during a drag, anywhere) to the `mouse_*` methods.
pub(crate) trait ViewSource: EntityInputHandler {
    fn view_input(&self) -> ViewInput<'_>;
    fn input_focus(&self) -> &FocusHandle;
    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>);
    /// A mouse move while the left button is pressed.
    fn mouse_drag(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>);
    fn mouse_up(&mut self, event: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>);
    /// A mouse move without a pressed button; `None` when the mouse left.
    fn mouse_hover(
        &mut self,
        position: Option<gpui::Point<Pixels>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    );
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
    #[cfg(test)]
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

    /// The bounds of the element in the last frame, in window coordinates.
    pub fn bounds(&self) -> Bounds<Pixels> {
        self.state.borrow().bounds
    }

    /// Counters for tests and timing.
    #[cfg(test)]
    pub fn stats(&self) -> ViewStats {
        self.state.borrow().stats()
    }

    /// Lays out the blocks around the viewport for `bounds` and builds the
    /// paint list of a frame.
    fn prepare_frame(
        &self,
        input: &ViewInput<'_>,
        bounds: Bounds<Pixels>,
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
