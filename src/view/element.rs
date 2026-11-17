//! The GPUI element that paints a document.

use gpui::{
    App, Bounds, CursorStyle, DispatchPhase, Element, ElementId, ElementInputHandler, Entity,
    GlobalElementId, Hitbox, HitboxBehavior, InspectorElementId, IntoElement, LayoutId,
    ModifiersChangedEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Point, ScrollEvent, Style, Window, px, relative,
};

use super::paint::Frame;
use super::{Affordance, DocumentView, ViewInput, ViewSource};

/// Paints the document of `source` with the state of `view`, filling the
/// space it is given. Scrolls with the mouse wheel on its own, hands text
/// input and mouse events to `source` and sets the mouse cursor.
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

pub(crate) struct Prepaint {
    frame: Frame,
    hitbox: Hitbox,
    cursor: CursorStyle,
    /// The accessibility nodes, only while assistive technology is active.
    a11y: Vec<super::a11y::A11yBlock>,
    scale: f32,
    /// Synthetic node ids of checkboxes, for their click action.
    checkboxes: Vec<(gpui::accesskit::NodeId, crate::model::BlockId)>,
}

/// The mouse cursor over `p`: a pointer over checkboxes, and over links
/// while the secondary modifier (Ctrl, Cmd on macOS) is held, because a
/// plain click on a link places the caret.
fn cursor_at(
    view: &DocumentView,
    input: &ViewInput<'_>,
    p: Point<Pixels>,
    window: &Window,
    cx: &App,
) -> CursorStyle {
    let ts = window.text_system().clone();
    let mut q = view.query(input.layout, &ts);
    let caret = input.selection.and_then(|s| s.head()).map(|h| h.block);
    if input.block_drag.is_some() {
        return CursorStyle::ClosedHand;
    }
    if q.drag_handle_at(p).is_some() {
        return CursorStyle::OpenHand;
    }
    match q.affordance_at(p, input.hover.or(Some(p)), caret) {
        Some(Affordance::ColumnBorder { .. }) => return CursorStyle::ResizeLeftRight,
        Some(_) => return CursorStyle::PointingHand,
        None => {}
    }
    if q.checkbox_at(p).is_some()
        || q.fold_toggle_at(p).is_some()
        || (window.modifiers().secondary(cx) && q.link_at(p).is_some())
    {
        CursorStyle::PointingHand
    } else {
        CursorStyle::IBeam
    }
}

impl<T: ViewSource> Element for DocumentElement<T> {
    type RequestLayoutState = ();
    type PrepaintState = Prepaint;

    fn id(&self) -> Option<ElementId> {
        Some("markdown-document".into())
    }

    fn a11y_role(&self) -> Option<gpui::Role> {
        Some(gpui::Role::Document)
    }

    fn write_a11y_info(&self, node: &mut gpui::accesskit::Node) {
        node.set_label("Document");
    }

    fn a11y_synthetic_children(
        &mut self,
        prepaint: &mut Self::PrepaintState,
        builder: &mut gpui::A11ySubtreeBuilder,
    ) {
        let scale = f64::from(prepaint.scale);
        prepaint.checkboxes.clear();
        for block in &prepaint.a11y {
            let mut node = gpui::accesskit::Node::new(block.role);
            node.set_label(block.label.clone());
            let b = block.bounds;
            node.set_bounds(gpui::accesskit::Rect {
                x0: f64::from(b.origin.x.as_f32()) * scale,
                y0: f64::from(b.origin.y.as_f32()) * scale,
                x1: f64::from((b.origin.x + b.size.width).as_f32()) * scale,
                y1: f64::from((b.origin.y + b.size.height).as_f32()) * scale,
            });
            if let Some(level) = block.level {
                node.set_level(level);
            }
            if block.selected {
                node.set_selected(true);
            }
            let id = builder.synthetic_node_id(block.id);
            if let Some(checked) = block.toggled {
                node.set_toggled(if checked {
                    gpui::Toggled::True
                } else {
                    gpui::Toggled::False
                });
                node.add_action(gpui::AccessibleAction::Click);
                prepaint.checkboxes.push((id, block.id));
            }
            builder.push_child(id, node);
        }
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
        let input = self.source.read(cx).view_input();
        let mut frame = self.view.prepare_frame(&input, bounds, &ts);
        let srcs = self.view.visible_images(&input);
        let resolver = input.resolve_src.cloned();
        if !srcs.is_empty() && self.view.load_images(&srcs, resolver.as_ref(), window, cx) {
            let input = self.source.read(cx).view_input();
            frame = self.view.prepare_frame(&input, bounds, &ts);
        }
        let input = self.source.read(cx).view_input();
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let cursor = cursor_at(&self.view, &input, window.mouse_position(), window, cx);
        let a11y = if window.is_a11y_active() {
            self.view.a11y_nodes(&input, &ts)
        } else {
            Vec::new()
        };
        Prepaint {
            frame,
            hitbox,
            cursor,
            a11y,
            scale: window.scale_factor(),
            checkboxes: Vec::new(),
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus = self.source.read(cx).input_focus().clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.source.clone()),
            cx,
        );
        for (node, block) in prepaint.checkboxes.drain(..) {
            let source = self.source.clone();
            window.on_a11y_action(node, gpui::AccessibleAction::Click, move |_, window, cx| {
                source.update(cx, |source, cx| source.a11y_toggle(block, window, cx));
            });
        }
        let hitbox = prepaint.hitbox.clone();
        let cursor = prepaint.cursor;
        window.set_cursor_style(cursor, &hitbox);
        prepaint.frame.paint(window, cx);

        let view = self.view.clone();
        let source = self.source.clone();
        let scroll_hitbox = hitbox.clone();
        window.on_mouse_event(move |event: &ScrollEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble || !scroll_hitbox.is_hovered(window) {
                return;
            }
            let dy = event.delta.pixel_delta(px(20.)).y;
            if dy != Pixels::ZERO {
                view.state().scroll_by(-dy);
                cx.notify(source.entity_id());
                cx.stop_propagation();
            }
        });

        let source = self.source.clone();
        let down_hitbox = hitbox.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble
                && event.button == MouseButton::Left
                && down_hitbox.is_hovered(window)
            {
                source.update(cx, |source, cx| source.mouse_down(event, window, cx));
                cx.stop_propagation();
            }
        });

        let source = self.source.clone();
        let view = self.view.clone();
        let move_hitbox = hitbox.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble {
                return;
            }
            if event.pressed_button == Some(MouseButton::Left) {
                source.update(cx, |source, cx| source.mouse_drag(event, window, cx));
            } else if move_hitbox.is_hovered(window) {
                source.update(cx, |source, cx| {
                    source.mouse_hover(Some(event.position), window, cx)
                });
                let now = cursor_at(
                    &view,
                    &source.read(cx).view_input(),
                    event.position,
                    window,
                    cx,
                );
                if now != cursor {
                    cx.notify(source.entity_id());
                }
            } else {
                source.update(cx, |source, cx| source.mouse_hover(None, window, cx));
            }
        });

        let source = self.source.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                source.update(cx, |source, cx| source.mouse_up(event, window, cx));
            }
        });

        let source = self.source.entity_id();
        window.on_modifiers_changed(move |_: &ModifiersChangedEvent, window, cx| {
            if hitbox.is_hovered(window) {
                cx.notify(source);
            }
        });
    }
}
