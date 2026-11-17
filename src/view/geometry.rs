//! Geometry queries: hit testing, caret positions, visual line movement and
//! scrolling to a position.

use std::cell::RefMut;
use std::ops::Range;
use std::sync::Arc;

use gpui::{Bounds, Pixels, Point, WindowTextSystem, point, px, size};
use unicode_segmentation::UnicodeSegmentation as _;

use crate::model::{BlockId, MarkSpan, TextPos, TextRange};

use super::LayoutInput;
use super::layout::{BlockLayout, Decoration, RectKind};
use super::state::ViewState;
use super::text::{floor_boundary, prev_boundary};

/// Geometry queries on a [`DocumentView`](super::DocumentView), opened with
/// [`DocumentView::query`](super::DocumentView::query).
///
/// Every query works for positions in blocks that are not laid out yet by
/// laying them out on demand. Positions with an offset past the end of their
/// text or inside a character are clamped.
pub(crate) struct ViewQuery<'a> {
    state: RefMut<'a, ViewState>,
    input: LayoutInput<'a>,
    ts: &'a WindowTextSystem,
}

/// One visual line of a text node, in document coordinates (x relative to
/// the content column).
#[derive(Clone, Copy, Debug)]
struct LineRef {
    slot: usize,
    node: usize,
    line: usize,
    rect: Bounds<Pixels>,
}

impl<'a> ViewQuery<'a> {
    pub(super) fn new(
        state: RefMut<'a, ViewState>,
        input: LayoutInput<'a>,
        ts: &'a WindowTextSystem,
    ) -> Self {
        Self { state, input, ts }
    }

    fn layout(&mut self, ix: usize) -> Option<Arc<BlockLayout>> {
        self.state.layout(&self.input, ix, self.ts)
    }

    /// The block layout and node index holding `pos.block`.
    fn node_of(&mut self, block: BlockId) -> Option<(usize, Arc<BlockLayout>, usize)> {
        let ix = self.state.locate(self.input.document, block)?;
        let layout = self.layout(ix)?;
        let (node, _) = layout.node(block)?;
        Some((ix, layout, node))
    }

    fn lines_of(&mut self, ix: usize) -> Vec<LineRef> {
        let Some(layout) = self.layout(ix) else {
            return Vec::new();
        };
        let top = self.state.top_of(ix);
        let mut out = Vec::new();
        for (n, node) in layout.nodes.iter().enumerate() {
            for line in 0..node.text.lines.len() {
                let r = node.line_rect(line);
                out.push(LineRef {
                    slot: ix,
                    node: n,
                    line,
                    rect: r + point(Pixels::ZERO, top),
                });
            }
        }
        out
    }

    fn pos_on_line(&mut self, l: LineRef, x: Pixels) -> Option<TextPos> {
        let layout = self.layout(l.slot)?;
        let node = &layout.nodes[l.node];
        Some(TextPos {
            block: node.id,
            offset: node.offset_on_line(l.line, x),
        })
    }

    /// The text position closest to `p`, also for points in the margins,
    /// between blocks and below the last block. `None` only for a document
    /// without text.
    pub fn position_at(&mut self, p: Point<Pixels>) -> Option<TextPos> {
        let d = self.state.to_doc(p);
        let ix = self.state.slot_at_y(d.y)?;
        let n = self.state.slots.len();
        // Blocks without text (rules, images) send the search to neighbours.
        for dist in 0..n {
            let mut best: Option<(Pixels, Pixels, LineRef)> = None;
            for j in [ix.checked_sub(dist), Some(ix + dist)]
                .into_iter()
                .flatten()
            {
                if j >= n || (dist == 0 && j != ix) {
                    continue;
                }
                for l in self.lines_of(j) {
                    let dy = axis_distance(d.y, l.rect.top(), l.rect.bottom());
                    let dx = axis_distance(d.x, l.rect.left(), l.rect.right());
                    if best.is_none_or(|(by, bx, _)| (dy, dx) < (by, bx)) {
                        best = Some((dy, dx, l));
                    }
                }
            }
            if let Some((_, _, l)) = best {
                return self.pos_on_line(l, d.x);
            }
        }
        None
    }

    /// The rule, image or table under `p`. Points inside a table hit the
    /// table; callers that edit cells should try [`Self::position_at`] for
    /// table cells first.
    pub fn node_at(&mut self, p: Point<Pixels>) -> Option<BlockId> {
        let d = self.state.to_doc(p);
        let ix = self.state.slot_at_y(d.y)?;
        let layout = self.layout(ix)?;
        let local = d - point(Pixels::ZERO, self.state.top_of(ix));
        layout
            .rects
            .iter()
            .find(|(_, kind, r)| {
                matches!(kind, RectKind::Rule | RectKind::Image | RectKind::Table)
                    && r.contains(&local)
            })
            .map(|(id, _, _)| *id)
    }

    /// The checklist item whose checkbox is under `p`.
    pub fn checkbox_at(&mut self, p: Point<Pixels>) -> Option<BlockId> {
        let d = self.state.to_doc(p);
        let ix = self.state.slot_at_y(d.y)?;
        let layout = self.layout(ix)?;
        let local = d - point(Pixels::ZERO, self.state.top_of(ix));
        layout.decorations.iter().find_map(|dec| match dec {
            Decoration::Checkbox { id, bounds, .. } if bounds.dilate(px(4.)).contains(&local) => {
                Some(*id)
            }
            _ => None,
        })
    }

    /// The link, wiki link or tag under `p`, with the block that holds it.
    pub fn link_at(&mut self, p: Point<Pixels>) -> Option<(BlockId, MarkSpan)> {
        let d = self.state.to_doc(p);
        let ix = self.state.slot_at_y(d.y)?;
        let layout = self.layout(ix)?;
        let local = d - point(Pixels::ZERO, self.state.top_of(ix));
        for node in &layout.nodes {
            if node.links.is_empty() || !node.bounds().contains(&local) {
                continue;
            }
            let line = ((local.y - node.origin.y) / node.text.line_height) as usize;
            let x = local.x - node.origin.x - node.line_offset(line);
            let offset = node.text.char_at(line, x)?;
            return node
                .links
                .iter()
                .find(|m| m.range.start <= offset && offset < m.range.end)
                .map(|m| (node.id, m.clone()));
        }
        None
    }

    /// The caret rectangle at `pos`: the caret width wide, one line high.
    pub fn caret_bounds(&mut self, pos: TextPos) -> Option<Bounds<Pixels>> {
        let (ix, layout, n) = self.node_of(pos.block)?;
        let node = &layout.nodes[n];
        let offset = floor_boundary(&node.text.text, pos.offset);
        let (_, p) = node.caret_point(offset);
        let r = Bounds::new(p, size(self.input.style.caret_width, node.text.line_height));
        Some(self.state.to_window(ix, r))
    }

    /// The first line rectangle of `range` in `block`, for placing the
    /// candidate window of an input method. An empty range gives the caret.
    pub fn bounds_for_range(
        &mut self,
        block: BlockId,
        range: Range<usize>,
    ) -> Option<Bounds<Pixels>> {
        let (ix, layout, n) = self.node_of(block)?;
        let node = &layout.nodes[n];
        let len = node.text.len();
        let range = floor_boundary(&node.text.text, range.start.min(len))
            ..floor_boundary(&node.text.text, range.end.min(len));
        if range.is_empty() {
            return self.caret_bounds(TextPos {
                block,
                offset: range.start,
            });
        }
        let r = *node.range_rects(range).first()?;
        Some(self.state.to_window(ix, r))
    }

    /// The bounds of a block, nested block or cell.
    pub fn block_bounds(&mut self, id: BlockId) -> Option<Bounds<Pixels>> {
        let ix = self.state.locate(self.input.document, id)?;
        let layout = self.layout(ix)?;
        let r = if self
            .input
            .document
            .blocks()
            .get(ix)
            .is_some_and(|b| b.id == id)
        {
            Bounds::new(
                point(Pixels::ZERO, Pixels::ZERO),
                size(self.state.width, layout.height),
            )
        } else {
            layout.rects.iter().find(|(r, _, _)| *r == id)?.2
        };
        Some(self.state.to_window(ix, r))
    }

    fn line_of(&mut self, pos: TextPos) -> Option<LineRef> {
        let (ix, layout, n) = self.node_of(pos.block)?;
        let node = &layout.nodes[n];
        let line = node
            .text
            .line_for_offset(floor_boundary(&node.text.text, pos.offset));
        let top = self.state.top_of(ix);
        Some(LineRef {
            slot: ix,
            node: n,
            line,
            rect: node.line_rect(line) + point(Pixels::ZERO, top),
        })
    }

    /// The position one visual line above `pos`, at window x `goal_x`
    /// (crossing into earlier blocks). `None` on the first line.
    pub fn position_above(&mut self, pos: TextPos, goal_x: Pixels) -> Option<TextPos> {
        self.vertical(pos, goal_x, false)
    }

    /// The position one visual line below `pos`, at window x `goal_x`.
    /// `None` on the last line.
    pub fn position_below(&mut self, pos: TextPos, goal_x: Pixels) -> Option<TextPos> {
        self.vertical(pos, goal_x, true)
    }

    fn vertical(&mut self, pos: TextPos, goal_x: Pixels, down: bool) -> Option<TextPos> {
        let cur = self.line_of(pos)?;
        let x = goal_x - self.state.bounds.origin.x - self.state.column_x;
        let eps = px(0.5);
        let n = self.state.slots.len();
        let mut ix = cur.slot;
        loop {
            let lines = self.lines_of(ix);
            let candidates = lines.into_iter().filter(|l| {
                if down {
                    l.rect.top() >= cur.rect.bottom() - eps
                } else {
                    l.rect.bottom() <= cur.rect.top() + eps
                }
            });
            let mut best: Option<LineRef> = None;
            for l in candidates {
                let better = match best {
                    None => true,
                    Some(b) => {
                        let (lt, bt) = (l.rect.top(), b.rect.top());
                        if (lt - bt).abs() > eps {
                            if down { lt < bt } else { lt > bt }
                        } else {
                            axis_distance(x, l.rect.left(), l.rect.right())
                                < axis_distance(x, b.rect.left(), b.rect.right())
                        }
                    }
                };
                if better {
                    best = Some(l);
                }
            }
            if let Some(l) = best {
                return self.pos_on_line(l, x);
            }
            if down {
                ix += 1;
                if ix >= n {
                    return None;
                }
            } else {
                ix = ix.checked_sub(1)?;
            }
        }
    }

    /// The start of the visual line at `pos`.
    pub fn line_start(&mut self, pos: TextPos) -> Option<TextPos> {
        let l = self.line_of(pos)?;
        let layout = self.layout(l.slot)?;
        let node = &layout.nodes[l.node];
        Some(TextPos {
            block: node.id,
            offset: node.text.lines[l.line].start,
        })
    }

    /// The end of the visual line at `pos`. On a soft-wrapped line that is
    /// before the space the line breaks at.
    pub fn line_end(&mut self, pos: TextPos) -> Option<TextPos> {
        let l = self.line_of(pos)?;
        let layout = self.layout(l.slot)?;
        let node = &layout.nodes[l.node];
        let lines = &node.text.lines;
        let vl = &lines[l.line];
        let wrapped = lines
            .get(l.line + 1)
            .is_some_and(|next| next.segment == vl.segment);
        let offset = if wrapped && vl.end > vl.start {
            prev_boundary(&node.text.text, vl.end)
        } else {
            vl.end
        };
        Some(TextPos {
            block: node.id,
            offset,
        })
    }

    /// The word at `pos` (for double click). Between a word and a space or
    /// punctuation the word wins; in a run of spaces the spaces are the word.
    pub fn word_range(&mut self, pos: TextPos) -> Option<TextRange> {
        let (_, layout, n) = self.node_of(pos.block)?;
        let text = &layout.nodes[n].text.text;
        let offset = floor_boundary(text, pos.offset);
        let range = word_at(text, offset);
        Some(TextRange {
            start: TextPos {
                block: pos.block,
                offset: range.start,
            },
            end: TextPos {
                block: pos.block,
                offset: range.end,
            },
        })
    }

    /// The whole text of the block or cell at `pos` (for triple click).
    pub fn block_range(&mut self, block: BlockId) -> Option<TextRange> {
        let (_, layout, n) = self.node_of(block)?;
        let len = layout.nodes[n].text.len();
        Some(TextRange {
            start: TextPos { block, offset: 0 },
            end: TextPos { block, offset: len },
        })
    }

    // --- scrolling ---------------------------------------------------------

    /// The document y at the top of the viewport.
    pub fn scroll_top(&mut self) -> Pixels {
        self.state.scroll_top()
    }

    /// Total height of the document (estimated for blocks not laid out).
    pub fn content_height(&mut self) -> Pixels {
        self.state.content_height()
    }

    pub fn viewport_height(&self) -> Pixels {
        self.state.viewport_height()
    }

    pub fn set_scroll_top(&mut self, y: Pixels) {
        self.state.set_scroll_top(y);
    }

    /// Scrolls by `dy` (positive scrolls down), clamped to the content.
    pub fn scroll_by(&mut self, dy: Pixels) {
        self.state.scroll_by(dy);
    }

    /// Scrolls the least amount that makes `range` visible (its start if it
    /// does not fit). Before the first frame, use
    /// [`DocumentView::request_reveal`](super::DocumentView::request_reveal).
    pub fn scroll_to_reveal(&mut self, range: TextRange) {
        let Some(start) = self.doc_caret(range.start) else {
            return;
        };
        let end = self.doc_caret(range.end).unwrap_or(start);
        let (top, bottom) = (start.top().min(end.top()), start.bottom().max(end.bottom()));
        let vh = self.state.viewport_height();
        let margin = (vh / 8.).min(px(40.));
        let scroll = self.state.scroll_top();
        let (top, bottom) = if bottom - top > vh - margin * 2. {
            (start.top(), start.bottom())
        } else {
            (top, bottom)
        };
        if top < scroll + margin {
            self.state.set_scroll_top(top - margin);
        } else if bottom > scroll + vh - margin {
            self.state.set_scroll_top(bottom - vh + margin);
        }
    }

    /// Scrolls `pos` into view, see [`Self::scroll_to_reveal`].
    pub fn reveal(&mut self, pos: TextPos) {
        self.scroll_to_reveal(TextRange {
            start: pos,
            end: pos,
        });
    }

    /// The caret rectangle in document coordinates.
    fn doc_caret(&mut self, pos: TextPos) -> Option<Bounds<Pixels>> {
        if let Some(l) = self.line_of(pos) {
            return Some(l.rect);
        }
        // Blocks without text (images, rules): their whole area.
        let ix = self.state.locate(self.input.document, pos.block)?;
        let layout = self.layout(ix)?;
        let r = layout.rects.iter().find(|(id, _, _)| *id == pos.block)?.2;
        Some(r + point(Pixels::ZERO, self.state.top_of(ix)))
    }

    /// Page down: scrolls by about a viewport and returns the position at
    /// the same place on screen as the caret was, at `goal_x`.
    pub fn page_down(&mut self, pos: TextPos, goal_x: Pixels) -> Option<TextPos> {
        self.page(pos, goal_x, true)
    }

    /// Page up, see [`Self::page_down`].
    pub fn page_up(&mut self, pos: TextPos, goal_x: Pixels) -> Option<TextPos> {
        self.page(pos, goal_x, false)
    }

    fn page(&mut self, pos: TextPos, goal_x: Pixels, down: bool) -> Option<TextPos> {
        let caret = self.caret_bounds(pos)?;
        let vh = self.state.viewport_height();
        let step = (vh - caret.size.height * 2.).max(caret.size.height);
        let before = self.state.scroll_top();
        self.state.scroll_by(if down { step } else { -step });
        let moved = self.state.scroll_top() - before;
        let target_y = caret.center().y + if down { step } else { -step } - moved;
        let target = point(goal_x, target_y);
        self.position_at(target)
    }
}

fn axis_distance(v: Pixels, lo: Pixels, hi: Pixels) -> Pixels {
    if v < lo {
        lo - v
    } else if v > hi {
        v - hi
    } else {
        Pixels::ZERO
    }
}

/// The word segment around `offset` (UAX #29 word bounds).
pub(crate) fn word_at(text: &str, offset: usize) -> Range<usize> {
    let is_word = |s: &str| s.chars().any(char::is_alphanumeric);
    let mut prev: Option<Range<usize>> = None;
    for (start, seg) in text.split_word_bound_indices() {
        let range = start..start + seg.len();
        if offset < range.end || (offset == range.end && range.end == text.len()) {
            if offset == range.start
                && !is_word(seg)
                && let Some(p) = prev.filter(|p| is_word(&text[p.clone()]))
            {
                return p;
            }
            return range;
        }
        prev = Some(range);
    }
    offset..offset
}
