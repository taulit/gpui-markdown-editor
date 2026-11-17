//! Layout of one top-level block (with its nested blocks and table cells)
//! into positioned text nodes and decorations.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

use gpui::{
    Bounds, FontWeight, Hsla, Pixels, Point, SharedString, TextAlign, WindowTextSystem, point, px,
    size,
};

use crate::model::{Align, Block, BlockId, BlockKind, Inline, Mark, MarkSpan};
use crate::style::EditorStyle;

use super::text::{TextLayout, TextSpec, build_runs, hash_inline};

/// Where a top-level block stands in the document, as far as it changes its
/// look.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct BlockCtx {
    /// The first block of its container: no space above.
    pub first: bool,
    /// Rendered as the document title.
    pub title: bool,
    /// The last item of a list: paragraph spacing below.
    pub list_end: bool,
}

/// A text-bearing node (block or table cell) placed in a [`BlockLayout`].
#[derive(Debug)]
pub(crate) struct NodeLayout {
    pub id: BlockId,
    /// Origin of the text, relative to the block's top-left corner.
    pub origin: Point<Pixels>,
    pub text: Arc<TextLayout>,
    pub align: TextAlign,
    pub align_width: Pixels,
    /// Alignment offset per visual line (empty for left-aligned text).
    pub offsets: Vec<Pixels>,
    /// Painted backgrounds of inline code and highlights.
    pub backgrounds: Vec<(Range<usize>, Hsla)>,
    /// Links, wiki links and tags, for `link_at`.
    pub links: Vec<MarkSpan>,
}

impl NodeLayout {
    pub fn line_offset(&self, line: usize) -> Pixels {
        self.offsets.get(line).copied().unwrap_or_default()
    }

    /// The rectangle of visual line `line`, relative to the block.
    pub fn line_rect(&self, line: usize) -> Bounds<Pixels> {
        let l = &self.text.lines[line.min(self.text.lines.len() - 1)];
        Bounds::new(
            point(
                self.origin.x + self.line_offset(line),
                self.origin.y + self.text.line_height * line as f32,
            ),
            size(l.width, self.text.line_height),
        )
    }

    /// The caret position for `offset`, relative to the block (top of line).
    pub fn caret_point(&self, offset: usize) -> (usize, Point<Pixels>) {
        let line = self.text.line_for_offset(offset);
        let x = self.text.x_in_line(line, offset) + self.line_offset(line);
        (
            line,
            point(
                self.origin.x + x,
                self.origin.y + self.text.line_height * line as f32,
            ),
        )
    }

    /// Closest offset to `p` (relative to the block) on visual line `line`.
    pub fn offset_on_line(&self, line: usize, x: Pixels) -> usize {
        self.text
            .offset_in_line(line, x - self.origin.x - self.line_offset(line))
    }

    pub fn range_rects(&self, range: Range<usize>) -> Vec<Bounds<Pixels>> {
        self.text
            .range_rects(range, &self.offsets)
            .into_iter()
            .map(|r| r + self.origin)
            .collect()
    }

    pub fn bounds(&self) -> Bounds<Pixels> {
        Bounds::new(
            self.origin,
            size(self.align_width.max(self.text.width), self.text.height()),
        )
    }
}

/// What a rectangle in [`BlockLayout::rects`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RectKind {
    Text,
    Rule,
    Image,
    Table,
    Cell,
}

/// Something painted that is not node text.
#[derive(Debug)]
pub(crate) enum Decoration {
    Rect {
        bounds: Bounds<Pixels>,
        color: Hsla,
        radius: Pixels,
    },
    Text {
        origin: Point<Pixels>,
        text: Arc<TextLayout>,
        align: TextAlign,
        width: Pixels,
    },
    Checkbox {
        id: BlockId,
        bounds: Bounds<Pixels>,
        checked: bool,
    },
}

/// The laid out top-level block.
#[derive(Debug)]
pub(crate) struct BlockLayout {
    /// Height including the space above and below.
    pub height: Pixels,
    /// Text nodes in document order (table cells row by row).
    pub nodes: Vec<NodeLayout>,
    pub decorations: Vec<Decoration>,
    /// The area of every block and cell of the subtree, in document order.
    pub rects: Vec<(BlockId, RectKind, Bounds<Pixels>)>,
}

impl BlockLayout {
    pub fn node(&self, id: BlockId) -> Option<(usize, &NodeLayout)> {
        self.nodes.iter().enumerate().find(|(_, n)| n.id == id)
    }

    /// Document order of `id` within this block.
    pub fn order_of(&self, id: BlockId) -> Option<usize> {
        self.rects.iter().position(|(r, _, _)| *r == id)
    }
}

/// Shaped texts, reused across block layouts while their content, look and
/// wrap width stay the same.
#[derive(Default)]
pub(crate) struct TextCache {
    entries: HashMap<(BlockId, bool), CachedText>,
    /// Number of texts shaped so far (for tests and timing).
    pub shaped: usize,
}

struct CachedText {
    key: u64,
    layout: Arc<TextLayout>,
    backgrounds: Vec<(Range<usize>, Hsla)>,
    frame: u64,
}

impl TextCache {
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn remove(&mut self, id: BlockId) {
        self.entries.remove(&(id, false));
        self.entries.remove(&(id, true));
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Drops texts not used since `frame` when the cache grows large.
    pub fn prune(&mut self, frame: u64, max: usize) {
        if self.entries.len() > max {
            self.entries.retain(|_, e| e.frame + 2 >= frame);
        }
    }
}

/// Everything block layout needs besides the block.
pub(crate) struct LayoutEnv<'a> {
    pub style: &'a EditorStyle,
    pub ts: &'a WindowTextSystem,
    pub texts: &'a mut TextCache,
    pub frame: u64,
}

impl LayoutEnv<'_> {
    fn text(
        &mut self,
        id: BlockId,
        inline: &Inline,
        spec: &TextSpec,
        wrap: Option<Pixels>,
    ) -> (Arc<TextLayout>, Vec<(Range<usize>, Hsla)>) {
        let mut h = DefaultHasher::new();
        hash_inline(inline, &mut h);
        spec.hash_into(&mut h);
        wrap.map(|w| w.as_f32().to_bits()).hash(&mut h);
        let key = h.finish();
        let slot = (id, wrap.is_some());
        if let Some(e) = self.texts.entries.get_mut(&slot)
            && e.key == key
        {
            e.frame = self.frame;
            return (e.layout.clone(), e.backgrounds.clone());
        }
        let (runs, backgrounds) = build_runs(inline, spec, self.style);
        let layout = Arc::new(TextLayout::shape(self.ts, &inline.text, &runs, spec, wrap));
        self.texts.shaped += 1;
        self.texts.entries.insert(
            slot,
            CachedText {
                key,
                layout: layout.clone(),
                backgrounds: backgrounds.clone(),
                frame: self.frame,
            },
        );
        (layout, backgrounds)
    }

    /// Shapes decoration text that has no id (bullets, numbers, alt text).
    fn label(&mut self, text: &str, spec: &TextSpec) -> Arc<TextLayout> {
        let inline = Inline::plain(text);
        let (runs, _) = build_runs(&inline, spec, self.style);
        Arc::new(TextLayout::shape(self.ts, text, &runs, spec, None))
    }

    fn body(&self) -> TextSpec {
        let s = self.style;
        TextSpec {
            family: s.font_family.clone(),
            size: s.text_size,
            line_height: s.text_size * s.line_height,
            weight: FontWeight::NORMAL,
            italic: false,
            color: s.text_color,
            strike: false,
            plain: false,
        }
    }

    fn mono(&self, color: Hsla) -> TextSpec {
        let s = self.style;
        TextSpec {
            family: s.code_font_family.clone(),
            size: s.code_size,
            line_height: s.code_size * s.code_line_height,
            color,
            plain: true,
            ..self.body()
        }
    }

    fn heading(&self, level: u8) -> TextSpec {
        let s = self.style;
        let (size, weight) = match level {
            0 | 1 => (s.title_size, s.title_weight),
            2 => (s.h2_size, s.h2_weight),
            _ => (s.h3_size, s.h3_weight),
        };
        TextSpec {
            size,
            line_height: size * s.heading_line_height,
            weight,
            ..self.body()
        }
    }
}

/// Lays out a top-level block at width `width`.
pub(crate) fn layout_block(
    env: &mut LayoutEnv<'_>,
    block: &Block,
    ctx: BlockCtx,
    width: Pixels,
) -> BlockLayout {
    let mut out = BlockLayout {
        height: Pixels::ZERO,
        nodes: Vec::new(),
        decorations: Vec::new(),
        rects: Vec::new(),
    };
    let (top, content, bottom) = place(env, block, ctx, px(0.), px(0.), width, &mut out);
    // `place` positions the content below its top margin already.
    out.height = top + content + bottom;
    out
}

/// Places `block` with its top margin starting at `y`. Returns the top
/// margin, content height and bottom margin.
fn place(
    env: &mut LayoutEnv<'_>,
    block: &Block,
    ctx: BlockCtx,
    x: Pixels,
    y: Pixels,
    width: Pixels,
    out: &mut BlockLayout,
) -> (Pixels, Pixels, Pixels) {
    let s = env.style;
    let width = width.max(px(20.));
    let para = s.paragraph_spacing;
    let list_bottom = if ctx.list_end { para } else { Pixels::ZERO };

    let text_node = |env: &mut LayoutEnv<'_>,
                     out: &mut BlockLayout,
                     id: BlockId,
                     inline: &Inline,
                     spec: &TextSpec,
                     origin: Point<Pixels>,
                     w: Pixels| {
        let (text, backgrounds) = env.text(id, inline, spec, Some(w));
        let h = text.height();
        out.rects
            .push((id, RectKind::Text, Bounds::new(origin, size(w, h))));
        out.nodes.push(NodeLayout {
            id,
            origin,
            text,
            align: TextAlign::Left,
            align_width: w,
            offsets: Vec::new(),
            backgrounds,
            links: links_of(inline),
        });
        h
    };

    match &block.kind {
        BlockKind::Paragraph | BlockKind::Heading(_) => {
            let level = match block.kind {
                BlockKind::Heading(l) => l,
                _ if ctx.title => 1,
                _ => 0,
            };
            let (spec, above, below) = match level {
                0 => (env.body(), Pixels::ZERO, para),
                1 => (
                    env.heading(1),
                    s.heading_spacing_above,
                    s.title_spacing_below,
                ),
                2 => (
                    env.heading(2),
                    s.heading_spacing_above,
                    s.heading_spacing_below,
                ),
                _ => (
                    env.heading(level),
                    s.heading_spacing_above * 0.7,
                    s.heading_spacing_below,
                ),
            };
            // Like collapsing CSS margins: the space below the previous
            // block (usually the paragraph spacing) counts towards this one.
            let above = if ctx.first {
                Pixels::ZERO
            } else {
                (above - para).max(Pixels::ZERO)
            };
            let h = text_node(
                env,
                out,
                block.id,
                &block.content,
                &spec,
                point(x, y + above),
                width,
            );
            let ch = children(env, block, x, y + above + h, width, out);
            (above, h + ch, below)
        }
        BlockKind::BulletItem | BlockKind::OrderedItem { .. } | BlockKind::TaskItem { .. } => {
            let mut spec = env.body();
            let lh = spec.line_height;
            let (indent, pad) = match &block.kind {
                BlockKind::BulletItem => {
                    let mut marker_spec = spec.clone();
                    marker_spec.color = s.muted_color;
                    let marker = env.label("•", &marker_spec);
                    let w = marker.width;
                    out.decorations.push(Decoration::Text {
                        origin: point(x, y),
                        text: marker,
                        align: TextAlign::Left,
                        width: w,
                    });
                    (w + s.list_marker_gap, Pixels::ZERO)
                }
                BlockKind::OrderedItem { number } => {
                    let mut marker_spec = spec.clone();
                    marker_spec.color = s.muted_color;
                    let marker = env.label(&format!("{number}."), &marker_spec);
                    let column = env.label("00.", &marker_spec).width.max(marker.width);
                    out.decorations.push(Decoration::Text {
                        origin: point(x, y),
                        text: marker,
                        align: TextAlign::Right,
                        width: column,
                    });
                    (column + s.list_marker_gap, Pixels::ZERO)
                }
                BlockKind::TaskItem { checked } => {
                    let pad = px(3.);
                    let d = s.checkbox_size;
                    let top = y + pad + ((lh - d) / 2.).max(Pixels::ZERO);
                    out.decorations.push(Decoration::Checkbox {
                        id: block.id,
                        bounds: Bounds::new(point(x, top), size(d, d)),
                        checked: *checked,
                    });
                    if *checked {
                        spec.color = s.faint_color;
                        spec.strike = true;
                    }
                    (d + s.checkbox_gap, pad)
                }
                _ => unreachable!(),
            };
            let tx = x + indent;
            let tw = width - indent;
            let h = text_node(
                env,
                out,
                block.id,
                &block.content,
                &spec,
                point(tx, y + pad),
                tw,
            );
            let own = h + pad * 2.;
            let ch = children(env, block, tx, y + own, tw, out);
            (Pixels::ZERO, own + ch, list_bottom)
        }
        BlockKind::Quote => {
            let indent = s.quote_bar_width + s.quote_indent;
            let pad = px(2.);
            let mut spec = env.body();
            spec.color = s.muted_color;
            let h = text_node(
                env,
                out,
                block.id,
                &block.content,
                &spec,
                point(x + indent, y + pad),
                width - indent,
            );
            let ch = children(env, block, x + indent, y + pad + h, width - indent, out);
            let total = h + ch + pad * 2.;
            out.decorations.push(Decoration::Rect {
                bounds: Bounds::new(point(x, y), size(s.quote_bar_width, total)),
                color: s.quote_bar_color,
                radius: s.quote_bar_width / 2.,
            });
            (Pixels::ZERO, total, para)
        }
        BlockKind::CodeBlock { .. } | BlockKind::Raw | BlockKind::Custom(_) => {
            let code = matches!(block.kind, BlockKind::CodeBlock { .. });
            let spec = env.mono(if code { s.text_color } else { s.muted_color });
            let (pad_x, pad_y) = if code {
                (px(14.), px(10.))
            } else {
                (px(0.), px(0.))
            };
            let above = if ctx.first || !code {
                Pixels::ZERO
            } else {
                px(4.)
            };
            let top = y + above;
            let h = text_node(
                env,
                out,
                block.id,
                &block.content,
                &spec,
                point(x + pad_x, top + pad_y),
                width - pad_x * 2.,
            );
            let total = h + pad_y * 2.;
            if code {
                out.decorations.push(Decoration::Rect {
                    bounds: Bounds::new(point(x, top), size(width, total)),
                    color: s.code_background,
                    radius: s.code_radius,
                });
            }
            (above, total, if code { para + px(4.) } else { para })
        }
        BlockKind::Rule => {
            let m = px(14.);
            let above = if ctx.first { Pixels::ZERO } else { m };
            out.decorations.push(Decoration::Rect {
                bounds: Bounds::new(point(x, y + above), size(width, px(1.))),
                color: s.table_border_color,
                radius: Pixels::ZERO,
            });
            out.rects.push((
                block.id,
                RectKind::Rule,
                Bounds::new(point(x, y + above - px(6.)), size(width, px(13.))),
            ));
            (above, px(1.), m)
        }
        BlockKind::Image(image) => {
            let m = px(12.);
            let above = if ctx.first { Pixels::ZERO } else { m };
            let w = image
                .width
                .map_or(width, |w| px(w as f32).min(width))
                .max(px(40.));
            let h = s.image_placeholder_height;
            let bounds = Bounds::new(point(x, y + above), size(w, h));
            out.decorations.push(Decoration::Rect {
                bounds,
                color: s.code_background,
                radius: s.image_radius,
            });
            let label: SharedString = if image.alt.is_empty() {
                image.src.clone()
            } else {
                image.alt.clone()
            };
            if !label.is_empty() {
                let mut spec = env.body();
                spec.color = s.faint_color;
                spec.size = s.code_size;
                spec.line_height = s.code_size * 1.4;
                let text = env.label(&label.replace('\n', " "), &spec);
                let th = text.height();
                out.decorations.push(Decoration::Text {
                    origin: point(x + px(12.), y + above + (h - th) / 2.),
                    text,
                    align: TextAlign::Center,
                    width: w - px(24.),
                });
            }
            out.rects.push((block.id, RectKind::Image, bounds));
            (above, h, m)
        }
        BlockKind::Table(table) => {
            let m = px(12.);
            let above = if ctx.first { Pixels::ZERO } else { m };
            let h = place_table(env, block.id, table, x, y + above, width, out);
            (above, h, m)
        }
    }
}

fn children(
    env: &mut LayoutEnv<'_>,
    block: &Block,
    x: Pixels,
    y: Pixels,
    width: Pixels,
    out: &mut BlockLayout,
) -> Pixels {
    let mut h = Pixels::ZERO;
    let kids = &block.children;
    for (ix, child) in kids.iter().enumerate() {
        let ctx = BlockCtx {
            first: false,
            title: false,
            list_end: is_list_end(kids, ix),
        };
        let (a, c, b) = place(env, child, ctx, x, y + h, width, out);
        // The last child's bottom space would double the parent's.
        let b = if ix + 1 == kids.len() {
            Pixels::ZERO
        } else {
            b
        };
        h += a + c + b;
    }
    h
}

pub(crate) fn is_list_item(kind: &BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::BulletItem | BlockKind::OrderedItem { .. } | BlockKind::TaskItem { .. }
    )
}

/// `true` if `blocks[ix]` is a list item not followed by another one.
pub(crate) fn is_list_end(blocks: &[Block], ix: usize) -> bool {
    is_list_item(&blocks[ix].kind) && blocks.get(ix + 1).is_none_or(|n| !is_list_item(&n.kind))
}

fn links_of(inline: &Inline) -> Vec<MarkSpan> {
    inline
        .marks
        .iter()
        .filter(|m| matches!(m.mark, Mark::Link { .. } | Mark::Custom { .. }))
        .cloned()
        .collect()
}

fn place_table(
    env: &mut LayoutEnv<'_>,
    id: BlockId,
    table: &crate::model::TableData,
    x: Pixels,
    y: Pixels,
    width: Pixels,
    out: &mut BlockLayout,
) -> Pixels {
    let s = env.style;
    let cols = table.rows.iter().map(Vec::len).max().unwrap_or(0);
    if cols == 0 {
        out.rects.push((
            id,
            RectKind::Table,
            Bounds::new(point(x, y), size(width, px(1.))),
        ));
        return px(1.);
    }
    let (pad_x, pad_y) = (s.table_cell_padding.left, s.table_cell_padding.top);
    let body = TextSpec {
        size: s.table_text_size,
        line_height: s.table_text_size * s.table_line_height,
        ..env.body()
    };
    let header = TextSpec {
        weight: FontWeight::BOLD,
        ..body.clone()
    };
    let spec_for = |row: usize| if row == 0 { &header } else { &body };

    // Natural column widths, then scaled to fill the width like an HTML
    // table with `width: 100%`.
    let mut natural = vec![px(24.); cols];
    for (r, row) in table.rows.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            let (t, _) = env.text(cell.id, &cell.content, spec_for(r), None);
            natural[c] = natural[c].max(t.width + pad_x * 2. + px(1.));
        }
    }
    let total: Pixels = natural.iter().copied().sum();
    let widths: Vec<Pixels> = natural.iter().map(|w| *w * (width / total)).collect();

    let mut row_y = y;
    let mut row_tops = Vec::with_capacity(table.rows.len() + 1);
    let border = s.table_border_color;
    for (r, row) in table.rows.iter().enumerate() {
        row_tops.push(row_y);
        let mut cx = x;
        let mut row_h = Pixels::ZERO;
        let first_node = out.nodes.len();
        for (c, cell) in row.iter().enumerate() {
            let w = widths[c];
            let inner = (w - pad_x * 2.).max(px(1.));
            let wrap = (natural[c] > w + px(0.5)).then_some(inner);
            let (text, backgrounds) = env.text(cell.id, &cell.content, spec_for(r), wrap);
            let align = match table.align.get(c).copied().unwrap_or_default() {
                Align::Center => TextAlign::Center,
                Align::Right => TextAlign::Right,
                Align::None | Align::Left => TextAlign::Left,
            };
            let offsets = text.line_offsets(align, inner);
            row_h = row_h.max(text.height() + pad_y * 2.);
            out.nodes.push(NodeLayout {
                id: cell.id,
                origin: point(cx + pad_x, row_y + pad_y),
                text,
                align,
                align_width: inner,
                offsets,
                backgrounds,
                links: links_of(&cell.content),
            });
            cx += w;
        }
        // Cell rects span the full row height.
        let mut cx = x;
        for (c, node) in out.nodes[first_node..].iter().enumerate() {
            out.rects.push((
                node.id,
                RectKind::Cell,
                Bounds::new(point(cx, row_y), size(widths[c], row_h)),
            ));
            cx += widths[c];
        }
        if r == 0 {
            out.decorations.push(Decoration::Rect {
                bounds: Bounds::new(point(x, row_y), size(width, row_h)),
                color: s.table_header_background,
                radius: Pixels::ZERO,
            });
        }
        row_y += row_h;
    }
    row_tops.push(row_y);
    let height = row_y - y;
    let hair = px(1.);
    for top in &row_tops {
        out.decorations.push(Decoration::Rect {
            bounds: Bounds::new(point(x, *top - hair / 2.), size(width, hair)),
            color: border,
            radius: Pixels::ZERO,
        });
    }
    let mut cx = x;
    for w in widths.iter().map(Some).chain([None]) {
        out.decorations.push(Decoration::Rect {
            bounds: Bounds::new(point(cx - hair / 2., y), size(hair, height)),
            color: border,
            radius: Pixels::ZERO,
        });
        cx += w.copied().unwrap_or_default();
    }
    // The table comes first in document order, before its cells.
    let at = out
        .rects
        .len()
        .saturating_sub(table.rows.iter().map(Vec::len).sum());
    out.rects.insert(
        at,
        (
            id,
            RectKind::Table,
            Bounds::new(point(x, y), size(width, height)),
        ),
    );
    height
}

/// Hash of everything that decides the layout of a top-level block.
pub(crate) fn block_key(block: &Block, ctx: BlockCtx, width: Pixels) -> u64 {
    let mut h = DefaultHasher::new();
    ctx.hash(&mut h);
    width.as_f32().to_bits().hash(&mut h);
    hash_block(block, &mut h);
    h.finish()
}

fn hash_block(block: &Block, h: &mut DefaultHasher) {
    block.id.hash(h);
    std::mem::discriminant(&block.kind).hash(h);
    match &block.kind {
        BlockKind::Heading(l) => l.hash(h),
        BlockKind::OrderedItem { number } => number.hash(h),
        BlockKind::TaskItem { checked } => checked.hash(h),
        BlockKind::Image(img) => {
            img.alt.hash(h);
            img.src.hash(h);
            img.width.hash(h);
        }
        BlockKind::Table(t) => {
            for a in &t.align {
                std::mem::discriminant(a).hash(h);
            }
            for row in &t.rows {
                row.len().hash(h);
                for cell in row {
                    cell.id.hash(h);
                    hash_inline(&cell.content, h);
                }
            }
        }
        _ => {}
    }
    hash_inline(&block.content, h);
    block.children.len().hash(h);
    for child in &block.children {
        hash_block(child, h);
    }
}

/// A guess of the height of a block that was not laid out yet.
pub(crate) fn estimate_height(block: &Block, style: &EditorStyle, width: Pixels) -> Pixels {
    let lines = |text: &str, size: Pixels| -> f32 {
        let per_line = (width.as_f32() / (size.as_f32() * 0.5)).max(1.);
        text.split('\n')
            .map(|l| (l.chars().count() as f32 / per_line).ceil().max(1.))
            .sum()
    };
    let body_lh = style.text_size * style.line_height;
    let own = match &block.kind {
        BlockKind::Paragraph => {
            body_lh * lines(&block.content.text, style.text_size) + style.paragraph_spacing
        }
        BlockKind::Heading(l) => {
            let size = match l {
                1 => style.title_size,
                2 => style.h2_size,
                _ => style.h3_size,
            };
            size * style.heading_line_height * lines(&block.content.text, size)
                + style.heading_spacing_above
                + style.heading_spacing_below
        }
        BlockKind::BulletItem | BlockKind::OrderedItem { .. } | BlockKind::Quote => {
            body_lh * lines(&block.content.text, style.text_size)
        }
        BlockKind::TaskItem { .. } => {
            body_lh * lines(&block.content.text, style.text_size) + px(6.)
        }
        BlockKind::CodeBlock { .. } | BlockKind::Raw | BlockKind::Custom(_) => {
            style.code_size * style.code_line_height * lines(&block.content.text, style.code_size)
                + px(30.)
        }
        BlockKind::Rule => px(29.),
        BlockKind::Image(_) => style.image_placeholder_height + px(24.),
        BlockKind::Table(t) => {
            (style.table_text_size * style.table_line_height + px(16.)) * t.rows.len() as f32
                + px(24.)
        }
    };
    let kids: Pixels = block
        .children
        .iter()
        .map(|c| estimate_height(c, style, width))
        .sum();
    own + kids
}
