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
mod images;
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
pub(crate) use layout::MIN_COLUMN;
#[cfg(test)]
pub(crate) use state::ViewStats;
pub(crate) use table_ui::Affordance;

/// How many decoded images the view keeps before it drops those far from
/// the viewport.
const MAX_KEPT_IMAGES: usize = 48;

/// What layout depends on.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LayoutInput<'a> {
    pub document: &'a Document,
    pub style: &'a EditorStyle,
    /// [`EditorOptions::first_line_is_title`](crate::EditorOptions::first_line_is_title).
    pub first_line_is_title: bool,
    /// Width of the content column including the side padding.
    pub max_width: Option<Pixels>,
    /// Column widths set by the user, per table.
    pub column_widths: &'a ColumnWidths,
    /// The width of an image being resized, before it is applied.
    pub image_preview: Option<(BlockId, u32)>,
}

/// Column widths in pixels per table, see [`DocMeta`](crate::DocMeta).
pub(crate) type ColumnWidths = std::collections::HashMap<BlockId, Vec<f32>>;

/// Everything the element paints.
#[derive(Clone)]
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
    /// [`EditorOptions::resolve_src`](crate::EditorOptions::resolve_src).
    pub resolve_src: Option<&'a crate::options::SrcResolver>,
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

    /// The width of the content column in the last frame.
    pub fn content_width(&self) -> Pixels {
        self.state.borrow().width
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

    /// Requests the images of the blocks laid out in the last frame and
    /// records their load state. `true` if a state changed, which changes
    /// the layout of its block.
    fn load_images(
        &self,
        srcs: &[gpui::SharedString],
        resolver: Option<&crate::options::SrcResolver>,
        window: &mut Window,
        cx: &mut gpui::App,
    ) -> bool {
        let mut state = self.state();
        let resolver = resolver.map(|r| &**r as &dyn Fn(&str) -> Option<std::path::PathBuf>);
        let mut changed = false;
        for src in srcs {
            let info = images::request(src, resolver, window, cx);
            changed |= images::update(&mut state.images, src, info);
        }
        // Forget images far from the viewport once there are many, so that
        // scrolling through a long document does not keep them all.
        if state.images.len() > MAX_KEPT_IMAGES {
            let drop: Vec<_> = state
                .images
                .keys()
                .filter(|k| !srcs.contains(k))
                .cloned()
                .collect();
            for src in drop {
                if let Some(images::ImageInfo::Ready { image, .. }) = state.images.remove(&src) {
                    cx.drop_image(image, Some(window));
                }
                images::forget(&src, resolver, cx);
            }
        }
        changed
    }

    /// The image sources of the blocks laid out in the last frame.
    fn visible_images(&self, input: &ViewInput<'_>) -> Vec<gpui::SharedString> {
        let state = self.state();
        let blocks = input.layout.document.blocks();
        let mut srcs = Vec::new();
        for slot in state.visible.clone() {
            if let Some(block) = blocks.get(slot) {
                images::sources(block, &mut srcs);
            }
        }
        srcs
    }

    fn state(&self) -> RefMut<'_, state::ViewState> {
        self.state.borrow_mut()
    }
}
