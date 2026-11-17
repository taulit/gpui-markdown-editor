//! Builds the paint list of a frame from the visible block layouts and the
//! selection, and paints it.

use std::collections::HashMap;
use std::sync::Arc;

use gpui::{
    App, Bounds, ContentMask, Hsla, PaintQuad, PathBuilder, Pixels, Point, TextAlign, Window,
    WindowTextSystem, fill, point, px, quad, size, transparent_black,
};

use crate::edit::Selection;
use crate::model::{Block, BlockId, BlockKind, Document};

use super::ViewInput;
use super::layout::{BlockLayout, Decoration};
use super::state::ViewState;
use super::text::TextLayout;

struct TextOp {
    origin: Point<Pixels>,
    text: Arc<TextLayout>,
    align: TextAlign,
    width: Pixels,
}

enum Overlay {
    Quad(PaintQuad),
    Check { bounds: Bounds<Pixels>, color: Hsla },
}

/// Everything painted in one frame, in window coordinates.
#[derive(Default)]
pub(crate) struct Frame {
    bounds: Bounds<Pixels>,
    under: Vec<PaintQuad>,
    images: Vec<(Bounds<Pixels>, Arc<gpui::RenderImage>, Pixels)>,
    texts: Vec<TextOp>,
    over: Vec<Overlay>,
}

/// A visible block: its slot, layout and window origin.
struct Visible {
    slot: usize,
    layout: Arc<BlockLayout>,
    origin: Point<Pixels>,
}

/// Position of a node in document order: (top-level slot, order in block).
type NodeKey = (usize, usize);

pub(crate) fn build_frame(
    state: &mut ViewState,
    input: &ViewInput<'_>,
    ts: &WindowTextSystem,
) -> Frame {
    let style = input.layout.style;
    let mut frame = Frame {
        bounds: state.bounds,
        ..Default::default()
    };
    let scroll = state.scroll_top();
    let column_x = state.bounds.origin.x + state.column_x;
    let mut visible = Vec::new();
    for slot in state.visible.clone() {
        let Some(layout) = state.layout(&input.layout, slot, ts) else {
            continue;
        };
        let y = state.bounds.origin.y + state.top_of(slot) - scroll;
        if y > state.bounds.bottom() || y + layout.height < state.bounds.top() {
            continue;
        }
        visible.push(Visible {
            slot,
            layout,
            origin: point(column_x, y),
        });
    }

    // Order keys of the visible nodes, to place selections quickly.
    let mut keys: HashMap<BlockId, NodeKey> = HashMap::new();
    for v in &visible {
        for (k, (id, _, _)) in v.layout.rects.iter().enumerate() {
            keys.insert(*id, (v.slot, k));
        }
    }

    for v in &visible {
        for dec in &v.layout.decorations {
            match dec {
                Decoration::Rect {
                    bounds,
                    color,
                    radius,
                } => frame
                    .under
                    .push(fill(*bounds + v.origin, *color).corner_radii(*radius)),
                Decoration::Text {
                    origin,
                    text,
                    align,
                    width,
                } => frame.texts.push(TextOp {
                    origin: *origin + v.origin,
                    text: text.clone(),
                    align: *align,
                    width: *width,
                }),
                Decoration::Image { bounds, src } => {
                    if let Some(super::images::ImageInfo::Ready { image, .. }) =
                        state.images.get(src)
                    {
                        frame
                            .images
                            .push((*bounds + v.origin, image.clone(), style.image_radius));
                    }
                }
                Decoration::Checkbox {
                    bounds, checked, ..
                } => {
                    let b = *bounds + v.origin;
                    let r = b.size.width / 2.;
                    if *checked {
                        frame.over.push(Overlay::Quad(
                            fill(b, style.checkbox_checked_color).corner_radii(r),
                        ));
                        frame.over.push(Overlay::Check {
                            bounds: b,
                            color: style.checkbox_check_color,
                        });
                    } else {
                        frame.over.push(Overlay::Quad(quad(
                            b,
                            r,
                            transparent_black(),
                            px(1.6),
                            style.checkbox_border_color,
                            Default::default(),
                        )));
                    }
                }
            }
        }
        for node in &v.layout.nodes {
            for (range, color) in &node.backgrounds {
                for r in node.range_rects(range.clone()) {
                    let r = Bounds::new(
                        point(r.origin.x - px(2.), r.origin.y + px(2.)),
                        size(r.size.width + px(4.), r.size.height - px(4.)),
                    );
                    frame
                        .under
                        .push(fill(r + v.origin, *color).corner_radii(px(4.)));
                }
            }
            frame.texts.push(TextOp {
                origin: node.origin + v.origin,
                text: node.text.clone(),
                align: node.align,
                width: node.align_width,
            });
        }
    }

    let doc = input.layout.document;
    let caret = input.selection.and_then(Selection::head).map(|p| p.block);
    for v in &visible {
        for g in super::table_ui::tables(&v.layout, doc) {
            let hover = input.hover.map(|h| h - v.origin);
            let Some(shown) = super::table_ui::shown(&g, hover, caret) else {
                continue;
            };
            for (a, r) in shown {
                paint_affordance(&mut frame.over, a, r + v.origin, input, style);
            }
        }
    }
    for range in input.highlights {
        let (Some(a), Some(b)) = (keys.get(&range.start.block), keys.get(&range.end.block)) else {
            if range.start.block == range.end.block {
                continue;
            }
            let ends = [range.start, range.end].map(|p| key_of(state, doc, p.block));
            if let [Some(a), Some(b)] = ends {
                paint_range(
                    &mut frame.under,
                    &visible,
                    (a, range.start.offset),
                    (b, range.end.offset),
                    style.highlight_color,
                    false,
                );
            }
            continue;
        };
        paint_range(
            &mut frame.under,
            &visible,
            (*a, range.start.offset),
            (*b, range.end.offset),
            style.highlight_color,
            false,
        );
    }

    match input.selection {
        Some(Selection::Text { anchor, head }) if anchor != head => {
            let ka = keys
                .get(&anchor.block)
                .copied()
                .or_else(|| key_of(state, doc, anchor.block));
            let kh = keys
                .get(&head.block)
                .copied()
                .or_else(|| key_of(state, doc, head.block));
            if let (Some(ka), Some(kh)) = (ka, kh) {
                let (a, b) = if (ka, anchor.offset) <= (kh, head.offset) {
                    ((ka, anchor.offset), (kh, head.offset))
                } else {
                    ((kh, head.offset), (ka, anchor.offset))
                };
                paint_range(
                    &mut frame.under,
                    &visible,
                    a,
                    b,
                    style.selection_color,
                    true,
                );
            }
        }
        Some(Selection::Text { head, .. }) if input.caret_visible => {
            if let Some((v, node)) = find_node(&visible, head.block) {
                let (_, p) =
                    node.caret_point(super::text::floor_boundary(&node.text.text, head.offset));
                let r = Bounds::new(p + v.origin, size(style.caret_width, node.text.line_height));
                frame.over.push(Overlay::Quad(fill(r, style.caret_color)));
            }
        }
        Some(Selection::All) => {
            let first = crate::edit::order::first_text(doc);
            let last = crate::edit::order::last_text(doc);
            if let (Some(first), Some(last)) = (first, last) {
                let end = crate::edit::order::inline_at(doc, &last.loc).map_or(0, |i| i.text.len());
                if let (Some(a), Some(b)) =
                    (key_of(state, doc, first.id), key_of(state, doc, last.id))
                {
                    paint_range(
                        &mut frame.under,
                        &visible,
                        (a, 0),
                        (b, end),
                        style.selection_color,
                        true,
                    );
                }
            }
            for v in &visible {
                for (_, kind, r) in &v.layout.rects {
                    use super::layout::RectKind;
                    if matches!(kind, RectKind::Rule | RectKind::Image | RectKind::Table) {
                        frame
                            .under
                            .push(fill(*r + v.origin, style.selection_color).corner_radii(px(4.)));
                    }
                }
            }
        }
        Some(Selection::Cells { anchor, head }) => {
            let cells = cell_rect_ids(doc, *anchor, *head);
            for v in &visible {
                for (id, kind, r) in &v.layout.rects {
                    if *kind == super::layout::RectKind::Cell && cells.contains(id) {
                        frame.under.push(fill(*r + v.origin, style.selection_color));
                    }
                }
            }
        }
        Some(Selection::Node(id)) => {
            for v in &visible {
                if let Some((_, kind, r)) = v.layout.rects.iter().find(|(r, _, _)| r == id) {
                    let radius = if *kind == super::layout::RectKind::Image {
                        style.image_radius + px(3.)
                    } else {
                        px(6.)
                    };
                    frame.over.push(Overlay::Quad(quad(
                        (*r + v.origin).dilate(px(3.)),
                        radius,
                        transparent_black(),
                        px(2.),
                        style.node_selection_color,
                        Default::default(),
                    )));
                    if *kind == super::layout::RectKind::Image {
                        for h in image_handles(*r + v.origin) {
                            frame.over.push(Overlay::Quad(quad(
                                h,
                                h.size.width / 2.,
                                style.node_selection_color,
                                px(1.5),
                                gpui::white(),
                                Default::default(),
                            )));
                        }
                    }
                }
            }
        }
        _ => {}
    }

    if let Some((block, range)) = &input.marked
        && let Some((v, node)) = find_node(&visible, *block)
    {
        let color = style.text_color;
        for r in node.range_rects(range.clone()) {
            let y = r.origin.y + r.size.height / 2. + node.text.font_size * 0.6;
            let line = Bounds::new(point(r.origin.x, y), size(r.size.width, px(1.)));
            frame.over.push(Overlay::Quad(fill(line + v.origin, color)));
        }
    }

    frame
}

/// A "+" bar or a row or column handle.
fn paint_affordance(
    out: &mut Vec<Overlay>,
    a: super::table_ui::Affordance,
    r: Bounds<Pixels>,
    input: &ViewInput<'_>,
    style: &crate::style::EditorStyle,
) {
    use super::table_ui::Affordance;
    let hovered = input.hover.is_some_and(|h| r.contains(&h));
    let open = input.open_handle == Some(a);
    match a {
        Affordance::AddRow { .. } | Affordance::AddColumn { .. } => {
            let bg = if hovered {
                style.table_border_color
            } else {
                style.table_header_background
            };
            out.push(Overlay::Quad(fill(r, bg).corner_radii(px(4.))));
            let c = r.center();
            let (arm, t) = (px(4.), px(1.5));
            let color = style.muted_color;
            out.push(Overlay::Quad(fill(
                Bounds::new(point(c.x - arm, c.y - t / 2.), size(arm * 2., t)),
                color,
            )));
            out.push(Overlay::Quad(fill(
                Bounds::new(point(c.x - t / 2., c.y - arm), size(t, arm * 2.)),
                color,
            )));
        }
        Affordance::RowHandle { .. } | Affordance::ColumnHandle { .. } => {
            let color = if open {
                style.node_selection_color
            } else if hovered {
                style.muted_color
            } else {
                style.faint_color
            };
            out.push(Overlay::Quad(
                fill(r, color.opacity(0.55)).corner_radii(px(3.)),
            ));
        }
        Affordance::ColumnBorder { .. } => {}
    }
}

/// The resize handles of a selected image with bounds `r`: pills inside
/// its left and right edge. The second one is the right handle.
pub(crate) fn image_handles(r: Bounds<Pixels>) -> [Bounds<Pixels>; 2] {
    let h = px(40.).min(r.size.height - px(8.)).max(px(8.));
    let w = px(7.);
    let y = r.center().y - h / 2.;
    [
        Bounds::new(point(r.left() + px(8.), y), size(w, h)),
        Bounds::new(point(r.right() - px(8.) - w, y), size(w, h)),
    ]
}

fn find_node(visible: &[Visible], id: BlockId) -> Option<(&Visible, &super::layout::NodeLayout)> {
    visible
        .iter()
        .find_map(|v| v.layout.node(id).map(|(_, n)| (v, n)))
}

/// Paints the rectangles of the text between `start` and `end` (ordered)
/// in the visible blocks. With `line_ends`, lines continuing into the next
/// node get a small extra width so that selected line breaks show.
fn paint_range(
    out: &mut Vec<PaintQuad>,
    visible: &[Visible],
    start: (NodeKey, usize),
    end: (NodeKey, usize),
    color: Hsla,
    line_ends: bool,
) {
    for v in visible {
        if v.slot < start.0.0 || v.slot > end.0.0 {
            continue;
        }
        for node in &v.layout.nodes {
            let Some(k) = v.layout.order_of(node.id) else {
                continue;
            };
            let key = (v.slot, k);
            if key < start.0 || key > end.0 {
                continue;
            }
            let len = node.text.len();
            let a = if key == start.0 { start.1.min(len) } else { 0 };
            let b = if key == end.0 { end.1.min(len) } else { len };
            if a > b {
                continue;
            }
            let mut rects = node.range_rects(a..b);
            if line_ends
                && key != end.0
                && let Some(last) = rects.last_mut()
            {
                last.size.width += px(5.);
            }
            for r in rects {
                if r.size.width > Pixels::ZERO {
                    out.push(fill(r + v.origin, color));
                }
            }
        }
    }
}

/// The cells of the rectangle spanned by two cells of one table.
fn cell_rect_ids(doc: &Document, anchor: BlockId, head: BlockId) -> Vec<BlockId> {
    for block in doc.blocks() {
        let BlockKind::Table(t) = &block.kind else {
            continue;
        };
        let find = |id: BlockId| {
            t.rows
                .iter()
                .enumerate()
                .find_map(|(r, row)| row.iter().position(|c| c.id == id).map(|c| (r, c)))
        };
        let (Some(a), Some(h)) = (find(anchor), find(head)) else {
            continue;
        };
        let (rows, cols) = (a.0.min(h.0)..=a.0.max(h.0), a.1.min(h.1)..=a.1.max(h.1));
        return t
            .rows
            .iter()
            .enumerate()
            .filter(|(r, _)| rows.contains(r))
            .flat_map(|(_, row)| {
                row.iter()
                    .enumerate()
                    .filter(|(c, _)| cols.contains(c))
                    .map(|(_, cell)| cell.id)
            })
            .collect();
    }
    Vec::new()
}

/// Document order key of a node that may be outside the visible blocks.
fn key_of(state: &ViewState, doc: &Document, id: BlockId) -> Option<NodeKey> {
    let slot = state.locate(doc, id)?;
    let block = doc.blocks().get(slot)?;
    let mut counter = 0;
    dfs_order(block, id, &mut counter).then_some((slot, counter))
}

/// Counts nodes before `id` in the same order as `BlockLayout::rects`.
fn dfs_order(block: &Block, id: BlockId, counter: &mut usize) -> bool {
    if block.id == id {
        return true;
    }
    *counter += 1;
    if let BlockKind::Table(t) = &block.kind {
        for cell in t.rows.iter().flatten() {
            if cell.id == id {
                return true;
            }
            *counter += 1;
        }
    }
    block.children.iter().any(|c| dfs_order(c, id, counter))
}

impl Frame {
    pub fn paint(&self, window: &mut Window, cx: &mut App) {
        let mask = ContentMask {
            bounds: self.bounds,
        };
        window.with_content_mask(Some(mask), |window| {
            for q in &self.under {
                window.paint_quad(q.clone());
            }
            for (bounds, image, radius) in &self.images {
                let _ = window.paint_image(
                    *bounds,
                    *bounds,
                    gpui::Corners::all(*radius),
                    image.clone(),
                    0,
                    false,
                );
            }
            for t in &self.texts {
                if t.origin.y > self.bounds.bottom()
                    || t.origin.y + t.text.height() < self.bounds.top()
                {
                    continue;
                }
                t.text.paint(t.origin, t.align, t.width, window, cx);
            }
            for o in &self.over {
                match o {
                    Overlay::Quad(q) => window.paint_quad(q.clone()),
                    Overlay::Check { bounds, color } => {
                        // The check mark of the prototype: a 12 px icon
                        // centered in the circle.
                        let s = bounds.size.width.as_f32() / 21.;
                        let o = bounds.origin;
                        let mut path = PathBuilder::stroke(px(1.6) * s);
                        path.move_to(o + point(px(6.5), px(10.8)) * s);
                        path.line_to(o + point(px(9.2), px(13.4)) * s);
                        path.line_to(o + point(px(14.6), px(7.8)) * s);
                        if let Ok(path) = path.build() {
                            window.paint_path(path, *color);
                        }
                    }
                }
            }
        });
    }
}
