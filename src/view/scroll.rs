//! Scrollbar support: the view is a gpui-component scroll handle. The
//! scroll state itself (anchor, clamping, revealing) lives in
//! [`ViewState`](super::state::ViewState) and [`ViewQuery`](super::ViewQuery).

use gpui::{Bounds, Pixels, Point, Size, point, size};
use gpui_component::scroll::ScrollbarHandle;

use super::DocumentView;

impl ScrollbarHandle for DocumentView {
    fn viewport_bounds(&self) -> Bounds<Pixels> {
        self.state.borrow().bounds
    }

    fn offset(&self) -> Point<Pixels> {
        point(Pixels::ZERO, -self.state.borrow_mut().scroll_top())
    }

    fn set_offset(&self, offset: Point<Pixels>) {
        self.state.borrow_mut().set_scroll_top(-offset.y);
    }

    /// Estimated for blocks that were not laid out yet, so the thumb can
    /// shift slightly while scrolling through a long document.
    fn content_size(&self) -> Size<Pixels> {
        let mut state = self.state.borrow_mut();
        size(state.bounds.size.width, state.content_height())
    }
}
