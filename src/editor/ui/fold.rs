//! Folding: a top-level heading hides the blocks after it up to the next
//! heading of the same or a higher level.
//!
//! Hidden blocks are not laid out and cannot hold the caret: movement
//! skips them and a caret inside a section being folded moves to the end
//! of its heading. The find bar unfolds the section of its current match.
//! The state is [`DocMeta::folded_headings`] (ordinals among all headings
//! of the document); a fold made in the editor emits
//! [`EditorEvent::MetaChanged`].

use std::collections::{BTreeSet, HashSet};

use gpui::{Context, Window};

use super::super::MarkdownEditor;
use crate::edit::Selection;
use crate::edit::order;
use crate::event::EditorEvent;
use crate::meta::DocMeta;
use crate::model::{Block, BlockId, BlockKind, Document, TextPos};

/// The level of a top-level heading.
fn level(block: &Block) -> Option<u8> {
    match block.kind() {
        BlockKind::Heading(level) => Some(*level),
        _ => None,
    }
}

/// The blocks hidden by the folded headings of `folded`.
pub(in crate::editor) fn hidden_blocks(
    doc: &Document,
    folded: &HashSet<BlockId>,
) -> HashSet<BlockId> {
    let mut hidden = HashSet::new();
    if folded.is_empty() {
        return hidden;
    }
    // The level of the innermost folded heading whose section we are in.
    let mut fold: Option<u8> = None;
    for block in doc.blocks() {
        if let Some(level) = level(block) {
            if fold.is_some_and(|f| level <= f) {
                fold = None;
            }
            if fold.is_none() {
                if folded.contains(&block.id()) {
                    fold = Some(level);
                }
                continue;
            }
        }
        if fold.is_some() {
            hidden.insert(block.id());
        }
    }
    hidden
}

/// Calls `f` with the id of every heading of the document in document
/// order, nested ones included (the ordinals of [`DocMeta`]).
fn visit_headings(blocks: &[Block], f: &mut impl FnMut(BlockId)) {
    for block in blocks {
        if level(block).is_some() {
            f(block.id());
        }
        visit_headings(block.children(), f);
    }
}

/// The folded heading ids for the ordinals in `meta`. Only top-level
/// headings fold.
pub(in crate::editor) fn folds_by_id(doc: &Document, meta: &DocMeta) -> HashSet<BlockId> {
    let top: HashSet<BlockId> = doc.blocks().iter().map(Block::id).collect();
    let mut out = HashSet::new();
    let mut ordinal = 0;
    visit_headings(doc.blocks(), &mut |id| {
        if meta.folded_headings.contains(&ordinal) && top.contains(&id) {
            out.insert(id);
        }
        ordinal += 1;
    });
    out
}

fn ordinals(doc: &Document, folded: &HashSet<BlockId>) -> BTreeSet<usize> {
    let mut out = BTreeSet::new();
    let mut ordinal = 0;
    visit_headings(doc.blocks(), &mut |id| {
        if folded.contains(&id) {
            out.insert(ordinal);
        }
        ordinal += 1;
    });
    out
}

impl MarkdownEditor {
    /// The top-level index of the block holding `id` (a block, nested
    /// block or cell).
    fn top_index(&self, id: BlockId) -> Option<usize> {
        self.state.locate(id).map(|loc| loc.top())
    }

    /// Folds or unfolds the top-level heading `heading`.
    pub(in crate::editor) fn toggle_fold(
        &mut self,
        heading: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let fold = !self.folded.contains(&heading);
        if self.set_folded(heading, fold, cx) {
            self.leave_hidden(true);
            self.after_change(false, window, cx);
        }
    }

    /// [`Command::FoldHeading`](crate::Command::FoldHeading): the heading
    /// at the caret.
    pub(in crate::editor) fn fold_at_caret(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let heading = self
            .state
            .current_block()
            .filter(|id| self.state.doc.blocks().iter().any(|b| b.id() == *id))
            .filter(|id| self.state.doc.block(*id).and_then(level).is_some());
        if let Some(heading) = heading {
            self.toggle_fold(heading, window, cx);
        }
    }

    /// Changes the fold state of `heading`. `true` if it changed.
    fn set_folded(&mut self, heading: BlockId, fold: bool, cx: &mut Context<Self>) -> bool {
        let is_heading = self
            .state
            .doc
            .blocks()
            .iter()
            .any(|b| b.id() == heading && level(b).is_some());
        let changed = if fold && is_heading {
            self.folded.insert(heading)
        } else {
            self.folded.remove(&heading)
        };
        if changed {
            self.apply_folds(cx, true);
        }
        changed
    }

    /// Unfolds the sections that hide `block`, for the find bar.
    pub(in crate::editor) fn unfold_around(&mut self, block: BlockId, cx: &mut Context<Self>) {
        let Some(ix) = self.top_index(block) else {
            return;
        };
        let blocks = self.state.doc.blocks();
        if !blocks
            .get(ix)
            .is_some_and(|b| self.hidden.contains(&b.id()))
        {
            return;
        }
        // Every folded heading above whose section reaches `ix`.
        let mut open = Vec::new();
        let mut min_level = u8::MAX;
        for b in blocks[..ix].iter().rev() {
            if let Some(l) = level(b)
                && l < min_level
            {
                min_level = l;
                if self.folded.contains(&b.id()) {
                    open.push(b.id());
                }
            }
        }
        for id in open {
            self.folded.remove(&id);
        }
        self.apply_folds(cx, true);
    }

    /// Recomputes the hidden blocks after `folded` changed and, if `store`,
    /// writes the ordinals to the metadata (emitting `MetaChanged`).
    fn apply_folds(&mut self, cx: &mut Context<Self>, store: bool) {
        let hidden = hidden_blocks(&self.state.doc, &self.folded);
        let changed: Vec<BlockId> = self.hidden.symmetric_difference(&hidden).copied().collect();
        self.hidden = hidden;
        if !changed.is_empty() {
            self.view
                .reestimate(&self.state.doc, &changed, &self.hidden);
        }
        // Metadata set by the host is kept as given until the user folds.
        let ordinals = ordinals(&self.state.doc, &self.folded);
        if store && ordinals != self.meta.folded_headings {
            self.meta.folded_headings = ordinals;
            cx.emit(EditorEvent::MetaChanged);
        }
        cx.notify();
    }

    /// After a structural edit: drops folds of headings that are gone or
    /// no longer headings, and updates the hidden blocks and ordinals.
    pub(in crate::editor) fn sync_folds(&mut self, cx: &mut Context<Self>) {
        if self.folded.is_empty() {
            return;
        }
        let doc = &self.state.doc;
        self.folded.retain(|id| {
            doc.blocks()
                .iter()
                .any(|b| b.id() == *id && level(b).is_some())
        });
        self.apply_folds(cx, true);
    }

    /// Takes the folds from the metadata, after `set_meta` or
    /// `set_markdown`.
    pub(in crate::editor) fn load_folds(&mut self, cx: &mut Context<Self>) {
        self.folded = folds_by_id(&self.state.doc, &self.meta);
        self.apply_folds(cx, false);
    }

    /// Moves a selection end that is inside a folded section out of it:
    /// forward past the section when `forward`, else to the end of its
    /// heading. The anchor of a range is collapsed onto the head.
    pub(in crate::editor) fn leave_hidden(&mut self, forward_default: bool) {
        if self.hidden.is_empty() {
            return;
        }
        let Some(head) = self.state.selection.head() else {
            return;
        };
        let Some(ix) = self.top_index(head.block) else {
            return;
        };
        let blocks = self.state.doc.blocks();
        if !blocks
            .get(ix)
            .is_some_and(|b| self.hidden.contains(&b.id()))
        {
            return;
        }
        let forward = match self.last_head {
            Some(last) => self.state.cmp_pos(last, head).is_lt(),
            None => forward_default,
        };
        // The heading that hides the block, and the first visible block
        // after the section.
        let heading = blocks[..ix]
            .iter()
            .rposition(|b| !self.hidden.contains(&b.id()))
            .map(|i| blocks[i].id());
        let after = blocks[ix..]
            .iter()
            .position(|b| !self.hidden.contains(&b.id()))
            .map(|i| blocks[ix + i].id());
        let target = if forward {
            after
                .and_then(|id| self.first_text_in(id))
                .or_else(|| heading.map(|h| self.state.end_of(h)))
        } else {
            heading.map(|h| self.state.end_of(h))
        };
        if let Some(target) = target {
            self.state.set_selection(Selection::caret(target));
        }
    }

    /// The first caret position in top-level block `id` or after it.
    fn first_text_in(&self, id: BlockId) -> Option<TextPos> {
        let loc = self.state.locate(id)?;
        let stop = order::stop_at(&self.state.doc, &loc)
            .filter(|s| s.text)
            .or_else(|| order::next_text(&self.state.doc, &loc))?;
        Some(TextPos {
            block: stop.id,
            offset: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hidden_texts(md: &str, fold: &[usize]) -> Vec<String> {
        let doc = Document::parse(md);
        let folded = fold.iter().map(|i| doc.blocks()[*i].id()).collect();
        let hidden = hidden_blocks(&doc, &folded);
        doc.blocks()
            .iter()
            .filter(|b| hidden.contains(&b.id()))
            .map(|b| b.text().to_string())
            .collect()
    }

    #[test]
    fn sections_end_at_the_same_or_a_higher_level() {
        let md = "# A\n\na\n\n## B\n\nb\n\n### C\n\nc\n\n## D\n\nd\n\n# E\n\ne\n";
        assert_eq!(hidden_texts(md, &[0]), ["a", "B", "b", "C", "c", "D", "d"]);
        assert_eq!(hidden_texts(md, &[2]), ["b", "C", "c"]);
        assert_eq!(hidden_texts(md, &[4]), ["c"]);
        // A fold inside a folded section changes nothing.
        assert_eq!(hidden_texts(md, &[0, 2]), hidden_texts(md, &[0]));
        assert_eq!(hidden_texts(md, &[8]), ["e"]);
        assert!(hidden_texts(md, &[1]).is_empty(), "not a heading");
    }

    #[test]
    fn ordinals_count_all_headings() {
        let doc = Document::parse("# A\n\n> ## Q\n\n## B\n");
        let mut meta = DocMeta::default();
        meta.folded_headings.insert(2);
        meta.folded_headings.insert(1);
        let folded = folds_by_id(&doc, &meta);
        // The nested heading (ordinal 1) does not fold.
        assert_eq!(folded, HashSet::from([doc.blocks()[2].id()]));
        assert_eq!(ordinals(&doc, &folded), BTreeSet::from([2]));
    }
}
