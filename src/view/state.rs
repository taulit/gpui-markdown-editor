//! The view state behind the element: per-block layout cache, estimated and
//! measured block heights, and the scroll anchor.
//!
//! # Virtualization
//!
//! Every top-level block has a [`Slot`] with its height: estimated from the
//! text length until the block is laid out, measured afterwards. Block tops
//! are the prefix sums of the slot heights. Only blocks that intersect the
//! viewport plus a buffer are laid out per frame; the geometry queries lay out
//! further blocks on demand.
//!
//! The scroll position is stored as an anchor: a block and the distance of
//! the viewport top from that block's top. When the estimate of a block above
//! the anchor is corrected, everything below shifts in document coordinates,
//! but the anchor and the content on screen stay put.
//!
//! # Invalidation
//!
//! Layouts are cached per top-level block under a hash of the block subtree,
//! its context and the width, and the shaped texts per node under a hash of
//! text, marks and look. A changed block therefore misses the cache and is
//! laid out again even without an explicit invalidation; [`ViewState::invalidate`]
//! additionally drops the entries and marks the height as an estimate.

use std::collections::HashMap;
use std::sync::Arc;

use gpui::{Bounds, Pixels, WindowTextSystem, point, px, size};

use crate::model::{Block, BlockId, BlockKind, Document};
use crate::style::EditorStyle;

use super::LayoutInput;
use super::layout::{
    BlockCtx, BlockLayout, LayoutEnv, TextCache, block_key, estimate_height, is_list_end,
};

/// One top-level block.
#[derive(Clone, Debug)]
pub(crate) struct Slot {
    pub id: BlockId,
    pub height: Pixels,
    pub measured: bool,
}

struct CachedBlock {
    key: u64,
    layout: Arc<BlockLayout>,
    frame: u64,
}

/// Counters for tests and timing.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ViewStats {
    /// Texts shaped since the view was created.
    pub shaped_texts: usize,
    /// Block layouts built since the view was created.
    pub block_layouts: usize,
    /// Blocks laid out in the last frame.
    pub visible_blocks: usize,
    /// Cached block layouts right now.
    pub cached_blocks: usize,
}

pub(crate) struct ViewState {
    pub slots: Vec<Slot>,
    slot_index: HashMap<BlockId, usize>,
    /// `tops[i]` is the y of block `i` in document coordinates; the last
    /// entry is the bottom of the last block.
    tops: Vec<Pixels>,
    tops_dirty: bool,
    blocks: HashMap<BlockId, CachedBlock>,
    texts: TextCache,
    /// Nested block or cell id -> top-level id, filled by layouts.
    node_top: HashMap<BlockId, BlockId>,
    style: Option<EditorStyle>,
    title: bool,
    reestimate: bool,
    /// Element bounds in window coordinates.
    pub bounds: Bounds<Pixels>,
    /// Width of the content column (without side padding).
    pub width: Pixels,
    /// x of the content column relative to the element.
    pub column_x: Pixels,
    pub padding_top: Pixels,
    pub padding_bottom: Pixels,
    /// The scroll anchor: block id and viewport top relative to its top.
    anchor: Option<BlockId>,
    anchor_offset: Pixels,
    /// Fallback when the anchor block was removed.
    anchor_index: usize,
    pub frame: u64,
    pub block_layouts: usize,
    /// Slots laid out in the last frame.
    pub visible: std::ops::Range<usize>,
    /// A range to scroll into view at the next frame.
    pub pending_reveal: Option<crate::model::TextRange>,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            slot_index: HashMap::new(),
            tops: vec![Pixels::ZERO],
            tops_dirty: true,
            blocks: HashMap::new(),
            texts: TextCache::default(),
            node_top: HashMap::new(),
            style: None,
            title: false,
            reestimate: false,
            bounds: Bounds::default(),
            width: px(604.),
            column_x: px(48.),
            padding_top: px(18.),
            padding_bottom: px(120.),
            anchor: None,
            anchor_offset: Pixels::ZERO,
            anchor_index: 0,
            frame: 0,
            block_layouts: 0,
            visible: 0..0,
            pending_reveal: None,
        }
    }
}

impl ViewState {
    pub fn stats(&self) -> ViewStats {
        ViewStats {
            shaped_texts: self.texts.shaped,
            block_layouts: self.block_layouts,
            visible_blocks: self.visible.len(),
            cached_blocks: self.blocks.len(),
        }
    }

    pub fn invalidate(&mut self, ids: impl IntoIterator<Item = BlockId>) {
        for id in ids {
            self.texts.remove(id);
            let top = self.node_top.get(&id).copied().unwrap_or(id);
            self.blocks.remove(&top);
            if let Some(&ix) = self.slot_index.get(&top)
                && let Some(slot) = self.slots.get_mut(ix)
            {
                slot.measured = false;
            }
        }
    }

    pub fn invalidate_all(&mut self) {
        self.blocks.clear();
        self.texts.clear();
        self.node_top.clear();
        for slot in &mut self.slots {
            slot.measured = false;
        }
    }

    /// Forgets everything about the previous document and scrolls to the top.
    pub fn reset(&mut self) {
        self.invalidate_all();
        self.reestimate = true;
        self.anchor = None;
        self.anchor_index = 0;
        self.anchor_offset = Pixels::ZERO;
    }

    pub fn viewport_height(&self) -> Pixels {
        self.bounds.size.height
    }

    /// Brings the state in line with `input`: style, column geometry, slots.
    /// Cheap when nothing changed (one id comparison per top-level block).
    pub fn sync(&mut self, input: &LayoutInput<'_>) {
        let style = input.style;
        if self.style.as_ref() != Some(style) || self.title != input.first_line_is_title {
            self.invalidate_all();
            self.style = Some(style.clone());
            self.title = input.first_line_is_title;
            self.tops_dirty = true;
        }
        let pad = style.padding;
        let avail = self.bounds.size.width;
        let column = input.max_width.map_or(avail, |m| m.min(avail));
        let width = (column - pad.left - pad.right).max(px(40.));
        self.column_x = ((avail - column) / 2.).max(Pixels::ZERO) + pad.left;
        if self.bounds.size.width <= Pixels::ZERO {
            self.column_x = pad.left;
        }
        if width != self.width {
            self.width = width;
            self.tops_dirty = true;
        }
        self.padding_top = pad.top;
        self.padding_bottom = pad.bottom;

        let blocks = input.document.blocks();
        let same = !self.reestimate
            && blocks.len() == self.slots.len()
            && blocks.iter().zip(&self.slots).all(|(b, s)| b.id == s.id);
        if !same {
            let old: HashMap<BlockId, Slot> = if self.reestimate {
                HashMap::new()
            } else {
                self.slots.drain(..).map(|s| (s.id, s)).collect()
            };
            self.reestimate = false;
            self.slots = blocks
                .iter()
                .map(|b| {
                    old.get(&b.id).cloned().unwrap_or_else(|| Slot {
                        id: b.id,
                        height: estimate_height(b, style, self.width),
                        measured: false,
                    })
                })
                .collect();
            self.slot_index = self
                .slots
                .iter()
                .enumerate()
                .map(|(ix, s)| (s.id, ix))
                .collect();
            self.tops_dirty = true;
        }
    }

    pub fn tops(&mut self) -> &[Pixels] {
        if self.tops_dirty || self.tops.len() != self.slots.len() + 1 {
            self.tops.clear();
            let mut y = self.padding_top;
            for slot in &self.slots {
                self.tops.push(y);
                y += slot.height;
            }
            self.tops.push(y);
            self.tops_dirty = false;
        }
        &self.tops
    }

    pub fn top_of(&mut self, ix: usize) -> Pixels {
        let tops = self.tops();
        tops[ix.min(tops.len() - 1)]
    }

    /// Total height of the content including padding.
    pub fn content_height(&mut self) -> Pixels {
        let bottom = self.padding_bottom;
        *self.tops().last().unwrap() + bottom
    }

    /// The slot at document y (clamped).
    pub fn slot_at_y(&mut self, y: Pixels) -> Option<usize> {
        if self.slots.is_empty() {
            return None;
        }
        let tops = self.tops();
        let ix = tops.partition_point(|t| *t <= y).saturating_sub(1);
        Some(ix.min(self.slots.len() - 1))
    }

    pub fn slot_of_top(&self, id: BlockId) -> Option<usize> {
        self.slot_index.get(&id).copied()
    }

    /// Finds the top-level block that contains `id` (a block, nested block or
    /// cell).
    pub fn locate(&self, doc: &Document, id: BlockId) -> Option<usize> {
        let blocks = doc.blocks();
        let hint = self.node_top.get(&id).copied().unwrap_or(id);
        if let Some(&ix) = self.slot_index.get(&hint)
            && blocks.get(ix).is_some_and(|b| contains(b, id))
        {
            return Some(ix);
        }
        blocks.iter().position(|b| contains(b, id))
    }

    pub fn ctx(&self, doc: &Document, ix: usize) -> BlockCtx {
        let blocks = doc.blocks();
        let title_kind = matches!(
            blocks[ix].kind,
            BlockKind::Paragraph | BlockKind::Heading(_)
        );
        BlockCtx {
            first: ix == 0,
            title: ix == 0 && self.title && title_kind,
            list_end: is_list_end(blocks, ix),
        }
    }

    /// The layout of top-level block `ix`, from the cache or laid out now.
    pub fn layout(
        &mut self,
        input: &LayoutInput<'_>,
        ix: usize,
        ts: &WindowTextSystem,
    ) -> Option<Arc<BlockLayout>> {
        let block = input.document.blocks().get(ix)?;
        let ctx = self.ctx(input.document, ix);
        let key = block_key(block, ctx, self.width);
        let frame = self.frame;
        if let Some(cached) = self.blocks.get_mut(&block.id)
            && cached.key == key
        {
            cached.frame = frame;
            let layout = cached.layout.clone();
            self.set_height(ix, layout.height);
            return Some(layout);
        }
        let mut env = LayoutEnv {
            style: input.style,
            ts,
            texts: &mut self.texts,
            frame,
        };
        let layout = Arc::new(super::layout::layout_block(
            &mut env, block, ctx, self.width,
        ));
        self.block_layouts += 1;
        for (id, _, _) in &layout.rects {
            if *id != block.id {
                self.node_top.insert(*id, block.id);
            }
        }
        self.blocks.insert(
            block.id,
            CachedBlock {
                key,
                layout: layout.clone(),
                frame,
            },
        );
        self.set_height(ix, layout.height);
        Some(layout)
    }

    fn set_height(&mut self, ix: usize, height: Pixels) {
        if let Some(slot) = self.slots.get_mut(ix) {
            if slot.height != height {
                slot.height = height;
                self.tops_dirty = true;
            }
            slot.measured = true;
        }
    }

    // --- scrolling ---------------------------------------------------------

    pub fn anchor_slot(&self) -> usize {
        self.anchor
            .and_then(|id| self.slot_index.get(&id).copied())
            .unwrap_or(self.anchor_index)
            .min(self.slots.len().saturating_sub(1))
    }

    /// The document y at the top of the viewport.
    pub fn scroll_top(&mut self) -> Pixels {
        if self.slots.is_empty() || self.anchor.is_none() {
            return Pixels::ZERO;
        }
        let ix = self.anchor_slot();
        self.top_of(ix) + self.anchor_offset
    }

    /// Scrolls so that the viewport top is at document y `y` (clamped to the
    /// content as far as it is known).
    pub fn set_scroll_top(&mut self, y: Pixels) {
        let max = (self.content_height() - self.viewport_height()).max(Pixels::ZERO);
        let y = y.clamp(Pixels::ZERO, max);
        match self.slot_at_y(y) {
            Some(ix) => {
                self.anchor = Some(self.slots[ix].id);
                self.anchor_index = ix;
                self.anchor_offset = y - self.top_of(ix);
            }
            None => {
                self.anchor = None;
                self.anchor_offset = Pixels::ZERO;
            }
        }
    }

    pub fn scroll_by(&mut self, dy: Pixels) {
        let top = self.scroll_top();
        self.set_scroll_top(top + dy);
    }

    /// Lays out the blocks around the viewport, correcting estimates without
    /// moving the content at the anchor.
    pub fn layout_visible(&mut self, input: &LayoutInput<'_>, ts: &WindowTextSystem) {
        self.frame += 1;
        if self.slots.is_empty() {
            self.visible = 0..0;
            return;
        }
        let vh = self.viewport_height();
        let buffer = (vh * 0.5).max(px(200.));
        // Laying out can change heights and clamping can move the viewport
        // onto other blocks, so repeat until stable (rarely more than twice).
        for _ in 0..4 {
            let anchor = self.anchor_slot();
            let top = self.scroll_top();
            let mut first = anchor;
            let mut ix = anchor;
            let mut y = self.top_of(anchor);
            while ix < self.slots.len() && y < top + vh + buffer {
                self.layout(input, ix, ts);
                y += self.slots[ix].height;
                ix += 1;
            }
            let last = ix;
            let mut bottom = self.top_of(anchor);
            while first > 0 && bottom > top - buffer {
                first -= 1;
                self.layout(input, first, ts);
                bottom -= self.slots[first].height;
            }
            self.visible = first..last;
            let before = self.scroll_top();
            // Re-anchor at the block actually at the viewport top, clamped.
            self.set_scroll_top(before);
            let after = self.scroll_top();
            let stable_range = {
                let a = self.anchor_slot();
                a >= first && a < last.max(first + 1)
            };
            if (after - before).abs() < px(0.5) && stable_range {
                break;
            }
        }
        self.prune();
    }

    fn prune(&mut self) {
        let keep = self.visible.len() * 4 + 256;
        if self.blocks.len() > keep {
            let frame = self.frame;
            self.blocks.retain(|_, c| c.frame + 2 >= frame);
        }
        self.texts.prune(self.frame, keep * 4);
    }

    /// The window rectangle of a document rectangle given relative to block
    /// `ix`.
    pub fn to_window(&mut self, ix: usize, r: Bounds<Pixels>) -> Bounds<Pixels> {
        let y = self.top_of(ix) - self.scroll_top();
        Bounds::new(
            point(
                self.bounds.origin.x + self.column_x + r.origin.x,
                self.bounds.origin.y + y + r.origin.y,
            ),
            r.size,
        )
    }

    /// Converts a window point to (document x relative to the column,
    /// document y).
    pub fn to_doc(&mut self, p: gpui::Point<Pixels>) -> gpui::Point<Pixels> {
        let top = self.scroll_top();
        point(
            p.x - self.bounds.origin.x - self.column_x,
            p.y - self.bounds.origin.y + top,
        )
    }

    pub fn column_bounds(&self) -> Bounds<Pixels> {
        Bounds::new(
            point(self.bounds.origin.x + self.column_x, self.bounds.origin.y),
            size(self.width, self.bounds.size.height),
        )
    }
}

/// `true` if `id` is `block`, one of its descendants or one of its cells.
pub(crate) fn contains(block: &Block, id: BlockId) -> bool {
    if block.id == id {
        return true;
    }
    if let BlockKind::Table(t) = &block.kind
        && t.rows.iter().flatten().any(|c| c.id == id)
    {
        return true;
    }
    block.children.iter().any(|c| contains(c, id))
}
