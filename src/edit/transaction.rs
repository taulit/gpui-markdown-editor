//! Transactions: steps applied together, with their inverse.
//!
//! Commands build a transaction with [`Tr`], which applies each step at once
//! (so later steps can look at the changed document) and records its
//! inverse. Applying a step marks the top-level block it touches dirty, so
//! the serializer rewrites that block. The previous `dirty` flags are logged
//! and restored by the inverse, so undoing every change gives back the
//! original Markdown byte for byte.

use super::Selection;
use super::order::{self, Loc, PathIndex};
use super::step::{Affected, Attr, Step, StepError};
use crate::model::{Block, BlockId, BlockKind, Document, Inline};

/// Steps with the selection before and after them.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Transaction {
    pub(crate) steps: Vec<Step>,
    pub(crate) selection_before: Selection,
    pub(crate) selection_after: Selection,
    /// `dirty` flags of top-level blocks to set after the steps. Filled in
    /// inverses only.
    pub(crate) restore_dirty: Vec<(BlockId, bool)>,
}

/// What applying a transaction changed, for the view's layout cache.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Applied {
    /// Top-level blocks whose content or attributes changed.
    pub(crate) changed: Vec<BlockId>,
    /// `true` if blocks, rows or columns were inserted, removed or moved.
    pub(crate) structure: bool,
}

/// A transaction under construction. Every step is applied immediately.
pub(crate) struct Tr<'a> {
    doc: &'a mut Document,
    index: &'a PathIndex,
    steps: Vec<Step>,
    inverses: Vec<Step>,
    dirty_log: Vec<(BlockId, bool)>,
    applied: Applied,
    /// Parents whose children changed: their ordered runs are renumbered.
    parents: Vec<Option<BlockId>>,
}

impl<'a> Tr<'a> {
    pub(crate) fn new(doc: &'a mut Document, index: &'a PathIndex) -> Self {
        Self {
            doc,
            index,
            steps: Vec::new(),
            inverses: Vec::new(),
            dirty_log: Vec::new(),
            applied: Applied::default(),
            parents: Vec::new(),
        }
    }

    pub(crate) fn doc(&self) -> &Document {
        self.doc
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Applies `step`. On error nothing changes.
    pub(crate) fn step(&mut self, step: Step) -> Result<(), StepError> {
        let Affected {
            tops,
            moved,
            parents,
        } = step.affected(self.doc, self.index)?;
        let outcome = step.apply(self.doc, self.index)?;
        // Steps keep the path index up to date themselves.
        if outcome.structural {
            self.applied.structure = true;
        }
        for id in tops.into_iter().chain(moved) {
            self.mark_dirty(id);
        }
        for parent in parents {
            if !self.parents.contains(&parent) {
                self.parents.push(parent);
            }
        }
        self.steps.push(step);
        self.inverses.push(outcome.inverse);
        Ok(())
    }

    fn mark_dirty(&mut self, id: BlockId) {
        let Some(loc) = self.index.locate(self.doc, id) else {
            return;
        };
        let block = match &loc {
            Loc::Block(path) => order::block_at_mut(&mut self.doc.blocks, path),
            Loc::Cell { .. } => None,
        };
        let Some(block) = block else {
            return;
        };
        if !self.dirty_log.iter().any(|(logged, _)| *logged == id) {
            self.dirty_log.push((id, block.dirty));
        }
        block.dirty = true;
        let top = self.doc.blocks.get(loc.top()).map(|block| block.id);
        if let Some(top) = top
            && !self.applied.changed.contains(&top)
        {
            self.applied.changed.push(top);
        }
    }

    /// Undoes every step applied so far.
    pub(crate) fn rollback(mut self) {
        while let Some(inverse) = self.inverses.pop() {
            if inverse.apply(self.doc, self.index).is_err() {
                debug_assert!(false, "inverse step failed");
            }
        }
        self.index.invalidate();
        restore_dirty(self.doc, self.index, &self.dirty_log);
    }

    /// Sets the dirty flags in `flags`, as the last part of an inverse.
    fn restore(&mut self, flags: &[(BlockId, bool)]) {
        restore_dirty(self.doc, self.index, flags);
    }

    /// Renumbers the ordered runs under every parent whose children changed:
    /// the first item of a run keeps its number, the others count up.
    pub(crate) fn renumber(&mut self) -> Result<(), StepError> {
        let parents = std::mem::take(&mut self.parents);
        for parent in &parents {
            let children: Vec<(BlockId, Option<u32>)> = {
                let siblings = match parent {
                    None => Some(self.doc.blocks.as_slice()),
                    Some(id) => match self.index.locate(self.doc, *id) {
                        Some(Loc::Block(path)) => {
                            order::block_at(&self.doc.blocks, &path).map(|b| b.children.as_slice())
                        }
                        _ => None,
                    },
                };
                siblings
                    .unwrap_or_default()
                    .iter()
                    .map(|block| match block.kind {
                        BlockKind::OrderedItem { number } => (block.id, Some(number)),
                        _ => (block.id, None),
                    })
                    .collect()
            };
            // A run starts at its smallest number: moving or inserting items
            // keeps the start of the list where it was.
            let mut index = 0;
            while index < children.len() {
                if children[index].1.is_none() {
                    index += 1;
                    continue;
                }
                let start = index;
                while index < children.len() && children[index].1.is_some() {
                    index += 1;
                }
                let run = &children[start..index];
                let first = run.iter().filter_map(|(_, n)| *n).min().unwrap_or(1);
                for (offset, (id, number)) in run.iter().enumerate() {
                    let expected = first.saturating_add(offset as u32);
                    if *number != Some(expected) {
                        self.step(Step::SetAttr {
                            block: *id,
                            attr: Attr::Number(expected),
                        })?;
                    }
                }
            }
        }
        // Renumbering itself only changes numbers, not runs.
        self.parents.clear();
        Ok(())
    }

    /// Ends the transaction: the forward transaction, its inverse and what
    /// changed.
    pub(crate) fn finish(
        self,
        before: Selection,
        after: Selection,
    ) -> (Transaction, Transaction, Applied) {
        let Tr {
            doc,
            index,
            steps,
            mut inverses,
            dirty_log,
            mut applied,
            ..
        } = self;
        applied.changed.retain(
            |id| matches!(index.locate(doc, *id), Some(Loc::Block(path)) if path.len() == 1),
        );
        inverses.reverse();
        let forward = Transaction {
            steps,
            selection_before: before.clone(),
            selection_after: after.clone(),
            restore_dirty: Vec::new(),
        };
        let inverse = Transaction {
            steps: inverses,
            selection_before: after,
            selection_after: before,
            restore_dirty: dirty_log,
        };
        (forward, inverse, applied)
    }

    // ----- Reading helpers -----

    pub(crate) fn loc(&self, id: BlockId) -> Result<Loc, StepError> {
        self.index
            .locate(self.doc, id)
            .ok_or_else(|| StepError(format!("no node {id:?}")))
    }

    pub(crate) fn inline(&self, id: BlockId) -> Result<&Inline, StepError> {
        let loc = self.loc(id)?;
        order::inline_at(self.doc, &loc).ok_or_else(|| StepError("node has no text".into()))
    }

    pub(crate) fn block(&self, id: BlockId) -> Result<&Block, StepError> {
        match self.loc(id)? {
            Loc::Block(path) => {
                order::block_at(&self.doc.blocks, &path).ok_or_else(|| StepError("no block".into()))
            }
            Loc::Cell { .. } => Err(StepError("cell is not a block".into())),
        }
    }

    /// The parent id (`None` for the top level) and the index among its
    /// siblings.
    pub(crate) fn parent(&self, id: BlockId) -> Result<(Option<BlockId>, usize), StepError> {
        let Loc::Block(path) = self.loc(id)? else {
            return Err(StepError("cell is not a block".into()));
        };
        let Some((&index, parent)) = path.split_last() else {
            return Err(StepError("empty path".into()));
        };
        let parent = if parent.is_empty() {
            None
        } else {
            order::block_at(&self.doc.blocks, parent).map(|b| b.id)
        };
        Ok((parent, index))
    }

    pub(crate) fn alloc_id(&mut self) -> BlockId {
        self.doc.alloc_id()
    }
}

fn restore_dirty(doc: &mut Document, index: &PathIndex, flags: &[(BlockId, bool)]) {
    for (id, dirty) in flags {
        if let Some(Loc::Block(path)) = index.locate(doc, *id)
            && let Some(block) = order::block_at_mut(&mut doc.blocks, &path)
        {
            block.dirty = *dirty;
        }
    }
}

/// Applies a stored transaction (undo or redo). Returns the inverse.
pub(crate) fn apply(
    doc: &mut Document,
    index: &PathIndex,
    tx: &Transaction,
) -> Result<(Transaction, Applied), StepError> {
    let mut tr = Tr::new(doc, index);
    for step in &tx.steps {
        if let Err(error) = tr.step(step.clone()) {
            tr.rollback();
            return Err(error);
        }
    }
    tr.restore(&tx.restore_dirty);
    let (_, inverse, applied) = tr.finish(tx.selection_before.clone(), tx.selection_after.clone());
    Ok((inverse, applied))
}
