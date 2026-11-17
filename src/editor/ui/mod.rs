//! The built-in UI parts ([`UiParts`](crate::UiParts)): find bar, slash menu,
//! format bar, link popover. They share the look of the Sheaf prototype
//! (`docs/design/plan/04-design.md`): the popover surface, radius 14, a
//! hairline and a soft shadow.
//!
//! Every part is drawn as an absolutely positioned child of the editor's
//! root, outside the key context of the document, so that keys typed into
//! a popup field never reach the document's bindings.

pub(super) mod complete;
pub(super) mod find;
pub(super) mod fold;
pub(super) mod format_bar;
pub(super) mod link;
pub(super) mod slash;

use std::time::Duration;

use gpui::{
    Animation, AnimationElement, AnimationExt as _, App, Bounds, BoxShadow, Div, ElementId, Hsla,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Point, SharedString, Size,
    Stateful, StatefulInteractiveElement as _, Styled as _, div, ease_out_quint, hsla, point,
    prelude::FluentBuilder as _, px,
};
use gpui_component::ActiveTheme as _;

/// Space kept between a popup and the edge of the editor.
pub(super) const MARGIN: Pixels = px(8.);

/// How long a popup takes to appear (plan 04: popover, 250 ms ease-out).
pub(super) const APPEAR: Duration = Duration::from_millis(250);

/// How far a popup slides while it appears.
pub(super) const APPEAR_OFFSET: Pixels = px(6.);

/// The colors of the popups, taken from the gpui-component theme.
#[derive(Clone, Copy, Debug)]
pub(super) struct Palette {
    pub bg: Hsla,
    pub fg: Hsla,
    pub muted: Hsla,
    pub hair: Hsla,
    pub hover: Hsla,
    pub accent: Hsla,
}

impl Palette {
    pub fn new(cx: &App) -> Self {
        let theme = cx.theme();
        Self {
            bg: theme.popover,
            fg: theme.popover_foreground,
            muted: theme.muted_foreground,
            hair: theme.border,
            hover: theme.foreground.opacity(0.07),
            accent: theme.primary,
        }
    }

    /// The background of the selected row of a list (accent 22 %).
    pub fn selected(&self) -> Hsla {
        self.accent.opacity(0.22)
    }
}

/// The floating surface: popover color, radius 14, hairline, shadow
/// `0 10px 30px -6px rgba(0,0,0,.5)`.
pub(super) fn panel(palette: &Palette) -> Div {
    div()
        .flex()
        .bg(palette.bg)
        .text_color(palette.fg)
        .rounded(px(14.))
        .border_1()
        .border_color(palette.hair)
        .shadow(vec![shadow()])
}

/// The popup shadow of plan 04: `0 10px 30px -6px rgba(0,0,0,.5)`.
fn shadow() -> BoxShadow {
    BoxShadow {
        color: hsla(0., 0., 0., 0.5),
        offset: point(px(0.), px(10.)),
        blur_radius: px(30.),
        spread_radius: px(-6.),
        inset: false,
    }
}

/// The panel of a menu list (slash menu, completions): `width` wide,
/// padding 6, swallowing clicks so that they do not reach the document.
pub(super) fn menu_panel(
    palette: &Palette,
    id: &'static str,
    role: gpui::Role,
    label: &'static str,
    width: Pixels,
) -> Stateful<Div> {
    panel(palette)
        .id(id)
        .role(role)
        .aria_label(label)
        .occlude()
        .flex_col()
        .w(width)
        .p(px(6.))
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

/// A row of a menu list: item `index` of `count`, highlighted when
/// `selected`, with `id` as element id and debug selector.
pub(super) fn menu_row(
    id: SharedString,
    palette: &Palette,
    role: gpui::Role,
    label: SharedString,
    (index, count): (usize, usize),
    selected: bool,
) -> Stateful<Div> {
    let hover = palette.hover;
    let selector = id.clone();
    div()
        .id(id)
        .debug_selector(move || selector.to_string())
        .role(role)
        .aria_label(label)
        .aria_selected(selected)
        .aria_position_in_set(index + 1)
        .aria_size_of_set(count)
        .flex()
        .px(px(12.))
        .rounded(px(9.))
        .cursor_pointer()
        .when(selected, |s| s.bg(palette.selected()))
        .when(!selected, |s| s.hover(move |s| s.bg(hover)))
}

/// Moves a list selection by `delta`, wrapping around; `selected` as it is
/// for an empty list.
pub(super) fn wrap(selected: usize, delta: isize, len: usize) -> usize {
    if len == 0 {
        return selected;
    }
    (selected as isize + delta).rem_euclid(len as isize) as usize
}

/// A square toolbar button (38 x 36 in the format bar), highlighted when
/// `active`.
pub(super) fn tool_button(
    id: impl Into<SharedString>,
    palette: &Palette,
    active: bool,
    content: impl IntoElement,
) -> Stateful<Div> {
    let hover = palette.hover;
    let id = id.into();
    div()
        .id(id.clone())
        .debug_selector(move || format!("ui-{id}"))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px(8.))
        .cursor_pointer()
        .when_some(active.then(|| palette.selected()), |s, bg| s.bg(bg))
        .hover(move |s| s.bg(hover))
        .child(content)
}

/// Positions a popup of `size` next to `anchor` (window coordinates),
/// below it or, if `above` is preferred and there is room, above it, and
/// keeps it inside `area` (the editor). Returns the position relative to
/// `area`'s origin.
pub(super) fn place(
    anchor: Bounds<Pixels>,
    size: Size<Pixels>,
    area: Bounds<Pixels>,
    above: bool,
) -> Point<Pixels> {
    let gap = px(6.);
    let room_above = anchor.top() - area.top() >= size.height + gap + MARGIN;
    let room_below = area.bottom() - anchor.bottom() >= size.height + gap + MARGIN;
    let y = if (above && room_above) || (!room_below && room_above) {
        anchor.top() - gap - size.height
    } else {
        anchor.bottom() + gap
    };
    let x = anchor.left();
    let max_x = (area.right() - MARGIN - size.width).max(area.left() + MARGIN);
    let max_y = (area.bottom() - MARGIN - size.height).max(area.top() + MARGIN);
    point(
        x.clamp(area.left() + MARGIN, max_x) - area.left(),
        y.clamp(area.top() + MARGIN, max_y) - area.top(),
    )
}

/// Fades `element` in and slides it into place from `APPEAR_OFFSET` above
/// (or below, with `from_below`) when it is first drawn. The animation runs
/// again only after the element was absent for a frame; with
/// [`App::reduce_motion`] GPUI draws the end state at once.
pub(super) fn appear<E: gpui::Styled + IntoElement + 'static>(
    element: E,
    id: &'static str,
    from_below: bool,
) -> AnimationElement<E> {
    let offset = if from_below {
        APPEAR_OFFSET
    } else {
        -APPEAR_OFFSET
    };
    element.with_animation(
        ElementId::Name(format!("{id}-appear").into()),
        Animation::new(APPEAR).with_easing(ease_out_quint()),
        move |element, t| element.relative().top(offset * (1. - t)).opacity(t),
    )
}

/// Wraps `child` at `position` relative to the editor.
pub(super) fn overlay(position: Point<Pixels>, child: impl IntoElement) -> Div {
    div()
        .absolute()
        .left(position.x)
        .top(position.y)
        .child(child)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::size;

    #[test]
    fn wrap_moves_around_the_ends() {
        assert_eq!(wrap(0, -1, 3), 2);
        assert_eq!(wrap(2, 1, 3), 0);
        assert_eq!(wrap(1, 1, 3), 2);
        assert_eq!(wrap(4, 1, 0), 4);
    }

    #[test]
    fn shadow_matches_the_design() {
        let s = shadow();
        assert_eq!(s.color.a, 0.5);
        assert_eq!(s.offset, point(px(0.), px(10.)));
        assert_eq!((s.blur_radius, s.spread_radius), (px(30.), px(-6.)));
    }

    #[test]
    fn place_keeps_popups_inside_the_editor() {
        let area = Bounds::new(point(px(100.), px(50.)), size(px(400.), px(300.)));
        let popup = size(px(200.), px(100.));
        // Below the anchor, relative to the area.
        let anchor = Bounds::new(point(px(150.), px(100.)), size(px(10.), px(20.)));
        assert_eq!(place(anchor, popup, area, false), point(px(50.), px(76.)));
        // Above when asked and there is room.
        let anchor = Bounds::new(point(px(150.), px(250.)), size(px(10.), px(20.)));
        assert_eq!(place(anchor, popup, area, true), point(px(50.), px(94.)));
        // Pushed left at the right edge, flipped up at the bottom.
        let anchor = Bounds::new(point(px(480.), px(300.)), size(px(10.), px(20.)));
        assert_eq!(place(anchor, popup, area, false), point(px(192.), px(144.)));
        // Too large: pinned to the margin.
        let huge = size(px(1000.), px(1000.));
        assert_eq!(place(anchor, huge, area, false), point(MARGIN, MARGIN));
    }
}
