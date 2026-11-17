//! The GPUI element that paints a document.

use gpui::{
    App, Bounds, CursorStyle, DispatchPhase, Element, ElementId, Entity, GlobalElementId, Hitbox,
    HitboxBehavior, InspectorElementId, IntoElement, LayoutId, Pixels, ScrollEvent, Style, Window,
    px, relative,
};

use super::paint::{Frame, build_frame};
use super::{DocumentView, ViewSource};

/// Paints the document of `source` with the state of `view`, filling the
/// space it is given. Scrolls with the mouse wheel on its own; everything
/// else (clicks, keys) is up to the owner, using the view's geometry queries.
pub(crate) struct DocumentElement<T: ViewSource> {
    view: DocumentView,
    source: Entity<T>,
}

impl<T: ViewSource> DocumentElement<T> {
    pub fn new(view: DocumentView, source: Entity<T>) -> Self {
        Self { view, source }
    }
}

impl<T: ViewSource> IntoElement for DocumentElement<T> {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl<T: ViewSource> Element for DocumentElement<T> {
    type RequestLayoutState = ();
    type PrepaintState = (Frame, Hitbox);

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let ts = window.text_system().clone();
        let frame = {
            let input = self.source.read(cx).view_input();
            let mut state = self.view.state();
            state.bounds = bounds;
            state.sync(&input.layout);
            if let Some(range) = state.pending_reveal.take() {
                drop(state);
                self.view.query(input.layout, &ts).scroll_to_reveal(range);
                state = self.view.state();
            }
            state.layout_visible(&input.layout, &ts);
            build_frame(&mut state, &input, &ts)
        };
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        (frame, hitbox)
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let (frame, hitbox) = prepaint;
        window.set_cursor_style(CursorStyle::IBeam, hitbox);
        frame.paint(window, cx);

        let view = self.view.clone();
        let source = self.source.entity_id();
        let hitbox = hitbox.clone();
        window.on_mouse_event(move |event: &ScrollEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble || !hitbox.is_hovered(window) {
                return;
            }
            let dy = event.delta.pixel_delta(px(20.)).y;
            if dy != Pixels::ZERO {
                view.state().scroll_by(-dy);
                cx.notify(source);
                cx.stop_propagation();
            }
        });
    }
}
