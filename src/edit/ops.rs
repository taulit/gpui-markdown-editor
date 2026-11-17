//! Compound edits built from steps: splitting, joining, lifting and deleting
//! ranges across blocks.

use super::inline_ops;
use super::order::{self, Loc};
use super::step::{Step, StepError};
use super::transaction::Tr;
use crate::model::{Block, BlockId, BlockKind, Inline, Mark, TextPos};

/// A command did not apply, or a step failed.
#[derive(Debug)]
pub(crate) enum Abort {
    /// The command does not apply here. Nothing changes.
    NotApplicable,
    /// A step failed: a bug. The transaction is rolled back.
    Step(StepError),
}

impl From<StepError> for Abort {
    fn from(error: StepError) -> Self {
        Abort::Step(error)
    }
}

pub(crate) type EditResult<T> = Result<T, Abort>;

/// A fresh block created by editing: no source range, written anew.
pub(crate) fn new_block(id: BlockId, kind: BlockKind, content: Inline) -> Block {
    Block {
        id,
        kind,
        content,
        children: Vec::new(),
        origin: None,
        dirty: true,
    }
}

impl Tr<'_> {
    pub(crate) fn replace(
        &mut self,
        node: BlockId,
        range: std::ops::Range<usize>,
        insert: Inline,
    ) -> Result<(), StepError> {
        if range.is_empty() && insert.text.is_empty() {
            return Ok(());
        }
        self.step(Step::ReplaceText {
            node,
            range,
            insert,
        })
    }

    pub(crate) fn is_literal(&self, node: BlockId) -> bool {
        self.block(node)
            .is_ok_and(|block| order::is_literal(&block.kind))
    }

    pub(crate) fn is_cell(&self, node: BlockId) -> bool {
        self.loc(node).is_ok_and(|loc| loc.is_cell())
    }

    /// Inserts typed text at `pos` with the formatting `marks`. Returns the
    /// offset behind it.
    pub(crate) fn insert_typed(
        &mut self,
        pos: TextPos,
        text: &str,
        marks: &[Mark],
    ) -> Result<usize, StepError> {
        let end = pos.offset + text.len();
        if self.is_literal(pos.block) {
            self.replace(pos.block, pos.offset..pos.offset, Inline::plain(text))?;
            return Ok(end);
        }
        let containing = inline_ops::containing(self.inline(pos.block)?, pos.offset);
        self.replace(
            pos.block,
            pos.offset..pos.offset,
            inline_ops::typed(text, marks),
        )?;
        for mark in containing {
            if !marks.contains(&mark) && !text.is_empty() {
                self.step(Step::RemoveMark {
                    node: pos.block,
                    range: pos.offset..end,
                    mark,
                })?;
            }
        }
        Ok(end)
    }

    pub(crate) fn new_block(&mut self, kind: BlockKind, content: Inline) -> Block {
        let id = self.alloc_id();
        new_block(id, kind, content)
    }

    pub(crate) fn insert_block(
        &mut self,
        parent: Option<BlockId>,
        index: usize,
        block: Block,
    ) -> Result<BlockId, StepError> {
        let id = block.id;
        self.step(Step::InsertBlock {
            parent,
            index,
            block: Box::new(block),
        })?;
        Ok(id)
    }

    /// Inserts `block` as the next sibling of `id`.
    pub(crate) fn insert_after(&mut self, id: BlockId, block: Block) -> Result<BlockId, StepError> {
        let (parent, index) = self.parent(id)?;
        self.insert_block(parent, index + 1, block)
    }

    pub(crate) fn insert_before(
        &mut self,
        id: BlockId,
        block: Block,
    ) -> Result<BlockId, StepError> {
        let (parent, index) = self.parent(id)?;
        self.insert_block(parent, index, block)
    }

    pub(crate) fn remove(&mut self, id: BlockId) -> Result<(), StepError> {
        self.step(Step::RemoveBlock { block: id })
    }

    pub(crate) fn move_block(
        &mut self,
        id: BlockId,
        parent: Option<BlockId>,
        index: usize,
    ) -> Result<(), StepError> {
        self.step(Step::MoveBlock {
            block: id,
            parent,
            index,
        })
    }

    fn child_ids(&self, id: BlockId) -> Result<Vec<BlockId>, StepError> {
        Ok(self.block(id)?.children.iter().map(|c| c.id).collect())
    }

    /// Moves the children of `id` behind it, as its siblings.
    pub(crate) fn lift_children(&mut self, id: BlockId) -> Result<(), StepError> {
        let children = self.child_ids(id)?;
        if children.is_empty() {
            return Ok(());
        }
        let (parent, index) = self.parent(id)?;
        for (offset, child) in children.into_iter().enumerate() {
            self.move_block(child, parent, index + 1 + offset)?;
        }
        Ok(())
    }

    /// Removes `id` and puts its children in its place.
    pub(crate) fn unwrap(&mut self, id: BlockId) -> Result<(), StepError> {
        let children = self.child_ids(id)?;
        let (parent, index) = self.parent(id)?;
        for (offset, child) in children.into_iter().enumerate() {
            self.move_block(child, parent, index + offset)?;
        }
        self.remove(id)
    }

    /// Changes the kind of a text block. Children move out if the new kind
    /// cannot hold them; marks go if it is code.
    pub(crate) fn set_kind(&mut self, id: BlockId, kind: BlockKind) -> Result<(), StepError> {
        let block = self.block(id)?;
        if block.kind == kind {
            return Ok(());
        }
        let has_children = !block.children.is_empty();
        let has_marks = !block.content.marks.is_empty();
        if has_children && !order::holds_children(&kind) {
            self.lift_children(id)?;
        }
        if has_marks && order::is_literal(&kind) {
            self.step(Step::SetMarks {
                node: id,
                marks: Vec::new(),
            })?;
        }
        self.step(Step::SetBlockKind { block: id, kind })
    }

    /// Splits the text block at `pos`: the text behind it and the children
    /// move into a new block of `kind` after it. Returns the new block.
    pub(crate) fn split(&mut self, pos: TextPos, kind: BlockKind) -> Result<BlockId, StepError> {
        let inline = self.inline(pos.block)?;
        let len = inline.text.len();
        let mut tail = inline_ops::slice(inline, pos.offset..len);
        if order::is_literal(&kind) {
            tail.marks.clear();
        }
        self.replace(pos.block, pos.offset..len, Inline::default())?;
        let block = self.new_block(kind.clone(), tail);
        let new = self.insert_after(pos.block, block)?;
        if order::holds_children(&kind) {
            for (index, child) in self.child_ids(pos.block)?.into_iter().enumerate() {
                self.move_block(child, Some(new), index)?;
            }
        }
        Ok(new)
    }

    /// Appends the text of `b` to `a` and removes `b`, keeping its children
    /// in its place. Returns the offset in `a` where the text of `b` starts.
    pub(crate) fn join(&mut self, a: BlockId, b: BlockId) -> Result<usize, StepError> {
        let at = self.inline(a)?.text.len();
        let mut content = self.inline(b)?.clone();
        if self.is_literal(a) {
            content.marks.clear();
        }
        self.replace(a, at..at, content)?;
        self.unwrap(b)?;
        Ok(at)
    }

    /// Deletes the text between two positions in document order, which may
    /// lie in different nodes. Blocks in between are removed; the first and
    /// last block are merged unless one of them is a table cell.
    pub(crate) fn delete_range(&mut self, start: TextPos, end: TextPos) -> EditResult<TextPos> {
        if start.block == end.block {
            self.replace(start.block, start.offset..end.offset, Inline::default())?;
            return Ok(start);
        }
        // A table whose cells are all selected goes as a whole: an empty
        // paragraph takes its place at that end of the range.
        let start_table = self.covered_table(start, true)?;
        let end_table = self.covered_table(end, false)?;
        if let (Some(a), Some(b)) = (start_table, end_table)
            && a == b
        {
            let block = self.new_block(BlockKind::Paragraph, Inline::default());
            let new = self.insert_before(a, block)?;
            self.remove(a)?;
            return Ok(TextPos {
                block: new,
                offset: 0,
            });
        }
        let start = match start_table {
            Some(table) => {
                let block = self.new_block(BlockKind::Paragraph, Inline::default());
                let new = self.insert_before(table, block)?;
                TextPos {
                    block: new,
                    offset: 0,
                }
            }
            None => start,
        };
        let end = match end_table {
            Some(table) => {
                let block = self.new_block(BlockKind::Paragraph, Inline::default());
                let new = self.insert_after(table, block)?;
                TextPos {
                    block: new,
                    offset: 0,
                }
            }
            None => end,
        };
        let start_loc = self.loc(start.block)?;
        let end_loc = self.loc(end.block)?;
        let between: Vec<order::Stop> = order::stops_between(self.doc(), &start_loc, &end_loc)
            .into_iter()
            .filter(|stop| stop.id != start.block && stop.id != end.block)
            .collect();
        let join = !start_loc.is_cell() && !end_loc.is_cell();

        // Text first: it does not move anything.
        let start_len = self.inline(start.block)?.text.len();
        let mut removed_paths: Vec<order::Path> = Vec::new();
        let mut unwrap_ids = Vec::new();
        let mut remove_ids = Vec::new();
        let mut clear_cells = Vec::new();
        for stop in &between {
            match &stop.loc {
                Loc::Cell { table, .. } => {
                    let table_removed = removed_paths.iter().any(|p| table.starts_with(p));
                    if !table_removed {
                        clear_cells.push(stop.id);
                    }
                }
                Loc::Block(path) => {
                    if removed_paths.iter().any(|p| path.starts_with(p)) {
                        continue;
                    }
                    if matches!(&end_loc, Loc::Cell { table, .. } if table == path) {
                        // The table of the last cell stays.
                        continue;
                    }
                    if order::is_ancestor(path, &end_loc) {
                        unwrap_ids.push(stop.id);
                    } else {
                        removed_paths.push(path.clone());
                        remove_ids.push(stop.id);
                    }
                }
            }
        }
        self.replace(start.block, start.offset..start_len, Inline::default())?;
        for cell in clear_cells {
            let len = self.inline(cell)?.text.len();
            self.replace(cell, 0..len, Inline::default())?;
        }
        self.replace(end.block, 0..end.offset, Inline::default())?;
        for id in remove_ids {
            self.remove(id)?;
        }
        for id in unwrap_ids {
            self.unwrap(id)?;
        }
        if join {
            self.join(start.block, end.block)?;
        }
        Ok(start)
    }

    /// The table that `pos` covers from its edge: the start of its first
    /// cell (`at_start`) or the end of its last cell.
    fn covered_table(&self, pos: TextPos, at_start: bool) -> Result<Option<BlockId>, StepError> {
        let Loc::Cell { table, row, col } = self.loc(pos.block)? else {
            return Ok(None);
        };
        let Some(block) = order::block_at(&self.doc().blocks, &table) else {
            return Ok(None);
        };
        let BlockKind::Table(data) = &block.kind else {
            return Ok(None);
        };
        let covered = if at_start {
            row == 0 && col == 0 && pos.offset == 0
        } else {
            let last_row = data.rows.len().saturating_sub(1);
            let last_col = data.rows.last().map_or(0, |r| r.len().saturating_sub(1));
            row == last_row && col == last_col && pos.offset == self.inline(pos.block)?.text.len()
        };
        Ok(covered.then_some(block.id))
    }

    /// Removes every block and leaves one empty paragraph. Returns its start.
    pub(crate) fn clear_all(&mut self) -> Result<TextPos, StepError> {
        let ids: Vec<BlockId> = self.doc().blocks.iter().map(|b| b.id).collect();
        let block = self.new_block(BlockKind::Paragraph, Inline::default());
        let new = self.insert_block(None, 0, block)?;
        for id in ids {
            self.remove(id)?;
        }
        Ok(TextPos {
            block: new,
            offset: 0,
        })
    }
}
