//! Table navigation and row/column commands.

use crate::edit::Selection;
use crate::edit::commands::blocks::new_cell;
use crate::edit::history::EditKind;
use crate::edit::ops::Abort;
use crate::edit::order::{self, Loc};
use crate::edit::state::EditorState;
use crate::edit::step::{Step, TableOp};
use crate::model::{Align, Block, BlockId, BlockKind, Inline, TableData, TextPos};

/// Where the caret's cell is.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CellAt {
    pub(crate) table: BlockId,
    pub(crate) row: usize,
    pub(crate) col: usize,
    pub(crate) rows: usize,
    pub(crate) cols: usize,
}

impl EditorState {
    /// The cell that holds `id`, with the table size.
    pub(crate) fn cell_at(&self, id: BlockId) -> Option<CellAt> {
        let Loc::Cell { table, row, col } = self.locate(id)? else {
            return None;
        };
        let block = order::block_at(&self.doc.blocks, &table)?;
        let BlockKind::Table(data) = &block.kind else {
            return None;
        };
        Some(CellAt {
            table: block.id,
            row,
            col,
            rows: data.rows.len(),
            cols: data.align.len(),
        })
    }

    /// The cell under the caret.
    pub(crate) fn current_cell(&self) -> Option<CellAt> {
        self.cell_at(self.selection.head()?.block)
    }

    fn table_data(&self, table: BlockId) -> Option<&TableData> {
        let loc = self.locate(table)?;
        match &order::block_at(&self.doc.blocks, loc.block_path())?.kind {
            BlockKind::Table(data) => Some(data),
            _ => None,
        }
    }

    fn cell_id(&self, table: BlockId, row: usize, col: usize) -> Option<BlockId> {
        Some(self.table_data(table)?.rows.get(row)?.get(col)?.id)
    }

    fn caret_in_cell(&mut self, table: BlockId, row: usize, col: usize) -> bool {
        let Some(id) = self.cell_id(table, row, col) else {
            return false;
        };
        let pos = self.end_of(id);
        self.set_selection(Selection::caret(pos))
    }

    /// Enter in a cell: the cell below, or a new row at the end.
    pub(crate) fn cell_enter(&mut self, cell: BlockId) -> bool {
        let Some(at) = self.cell_at(cell) else {
            return false;
        };
        if at.row + 1 < at.rows {
            return self.caret_in_cell(at.table, at.row + 1, at.col);
        }
        self.add_row(at, at.rows, at.col)
    }

    /// Tab and Shift+Tab in a table. Tab in the last cell adds a row.
    pub(crate) fn next_cell(&mut self, cell: BlockId, forward: bool) -> bool {
        let Some(at) = self.cell_at(cell) else {
            return false;
        };
        let linear = at.row * at.cols + at.col;
        if forward {
            let next = linear + 1;
            if next < at.rows * at.cols {
                return self.caret_in_cell(at.table, next / at.cols, next % at.cols);
            }
            self.add_row(at, at.rows, 0)
        } else {
            match linear.checked_sub(1) {
                Some(previous) => {
                    self.caret_in_cell(at.table, previous / at.cols, previous % at.cols)
                }
                None => false,
            }
        }
    }

    /// Inserts a row at `index` and puts the caret into column `col` of it.
    fn add_row(&mut self, at: CellAt, index: usize, col: usize) -> bool {
        self.transact(EditKind::Other, None, "", move |tr, _| {
            let cells: Vec<_> = (0..at.cols).map(|_| new_cell(tr)).collect();
            let target = cells.get(col).map(|c| c.id).ok_or(Abort::NotApplicable)?;
            tr.step(Step::TableOp {
                table: at.table,
                op: TableOp::InsertRow { index, cells },
            })?;
            Ok(Selection::caret(TextPos {
                block: target,
                offset: 0,
            }))
        })
    }

    /// Inserts an empty row above or below the caret's row.
    pub(crate) fn table_insert_row(&mut self, below: bool) -> bool {
        let Some(at) = self.current_cell() else {
            return false;
        };
        self.add_row(at, at.row + usize::from(below), at.col)
    }

    /// Inserts an empty column left or right of the caret's column.
    pub(crate) fn table_insert_column(&mut self, right: bool) -> bool {
        let Some(at) = self.current_cell() else {
            return false;
        };
        let index = at.col + usize::from(right);
        self.transact(EditKind::Other, None, "", move |tr, _| {
            let cells: Vec<_> = (0..at.rows).map(|_| new_cell(tr)).collect();
            let target = cells
                .get(at.row)
                .map(|c| c.id)
                .ok_or(Abort::NotApplicable)?;
            tr.step(Step::TableOp {
                table: at.table,
                op: TableOp::InsertColumn {
                    index,
                    align: Align::None,
                    cells,
                },
            })?;
            Ok(Selection::caret(TextPos {
                block: target,
                offset: 0,
            }))
        })
    }

    /// Removes the caret's row. The last row cannot be removed; delete the
    /// table as a node instead.
    pub(crate) fn table_remove_row(&mut self) -> bool {
        let Some(at) = self.current_cell() else {
            return false;
        };
        if at.rows < 2 {
            return false;
        }
        let row = if at.row + 1 < at.rows {
            at.row + 1
        } else {
            at.row - 1
        };
        let Some(next) = self.cell_id(at.table, row, at.col) else {
            return false;
        };
        self.transact(EditKind::Other, None, "", move |tr, _| {
            tr.step(Step::TableOp {
                table: at.table,
                op: TableOp::RemoveRow { index: at.row },
            })?;
            Ok(Selection::caret(TextPos {
                block: next,
                offset: 0,
            }))
        })
    }

    /// Removes the caret's column. The last column cannot be removed.
    pub(crate) fn table_remove_column(&mut self) -> bool {
        let Some(at) = self.current_cell() else {
            return false;
        };
        if at.cols < 2 {
            return false;
        }
        let col = if at.col + 1 < at.cols {
            at.col + 1
        } else {
            at.col - 1
        };
        let Some(next) = self.cell_id(at.table, at.row, col) else {
            return false;
        };
        self.transact(EditKind::Other, None, "", move |tr, _| {
            tr.step(Step::TableOp {
                table: at.table,
                op: TableOp::RemoveColumn { index: at.col },
            })?;
            Ok(Selection::caret(TextPos {
                block: next,
                offset: 0,
            }))
        })
    }

    /// Moves the caret's row up or down by one.
    pub(crate) fn table_move_row(&mut self, down: bool) -> bool {
        let Some(at) = self.current_cell() else {
            return false;
        };
        let to = if down {
            at.row + 1
        } else {
            at.row.wrapping_sub(1)
        };
        if to >= at.rows {
            return false;
        }
        self.transact(EditKind::Other, None, "", move |tr, sel| {
            tr.step(Step::TableOp {
                table: at.table,
                op: TableOp::MoveRow { from: at.row, to },
            })?;
            Ok(sel.clone())
        })
    }

    /// Moves the caret's column left or right by one.
    pub(crate) fn table_move_column(&mut self, right: bool) -> bool {
        let Some(at) = self.current_cell() else {
            return false;
        };
        let to = if right {
            at.col + 1
        } else {
            at.col.wrapping_sub(1)
        };
        if to >= at.cols {
            return false;
        }
        self.transact(EditKind::Other, None, "", move |tr, sel| {
            tr.step(Step::TableOp {
                table: at.table,
                op: TableOp::MoveColumn { from: at.col, to },
            })?;
            Ok(sel.clone())
        })
    }

    /// Sets the alignment of the caret's column.
    pub(crate) fn table_set_align(&mut self, align: Align) -> bool {
        let Some(at) = self.current_cell() else {
            return false;
        };
        if self
            .table_data(at.table)
            .and_then(|data| data.align.get(at.col))
            == Some(&align)
        {
            return false;
        }
        self.transact(EditKind::Other, None, "", move |tr, sel| {
            tr.step(Step::TableOp {
                table: at.table,
                op: TableOp::SetAlign {
                    column: at.col,
                    align,
                },
            })?;
            Ok(sel.clone())
        })
    }

    /// The cells of the rectangle spanned by `anchor` and `head`, row by
    /// row (rows shorter than the rectangle contribute fewer cells), with
    /// the first selected column.
    pub(crate) fn cell_rect(&self, anchor: BlockId, head: BlockId) -> Option<CellRect> {
        let (a, h) = (self.cell_at(anchor)?, self.cell_at(head)?);
        if a.table != h.table {
            return None;
        }
        let data = self.table_data(a.table)?;
        let rows = a.row.min(h.row)..a.row.max(h.row) + 1;
        let cols = a.col.min(h.col)..a.col.max(h.col) + 1;
        Some(CellRect {
            table: a.table,
            rows: rows.clone(),
            cols: cols.clone(),
            cells: data.rows[rows]
                .iter()
                .map(|row| {
                    row.iter()
                        .enumerate()
                        .filter(|(c, _)| cols.contains(c))
                        .map(|(_, cell)| cell.id)
                        .collect()
                })
                .collect(),
        })
    }

    /// The selected rectangle as a table of its own, for the clipboard. Its
    /// first row is the header.
    pub(crate) fn cells_block(&self, anchor: BlockId, head: BlockId) -> Option<Block> {
        let rect = self.cell_rect(anchor, head)?;
        let data = self.table_data(rect.table)?;
        let rows = data.rows[rect.rows.clone()]
            .iter()
            .map(|row| {
                rect.cols
                    .clone()
                    .map(|c| {
                        row.get(c).cloned().unwrap_or_else(|| crate::model::Cell {
                            id: BlockId(u64::MAX),
                            content: Inline::default(),
                        })
                    })
                    .collect()
            })
            .collect();
        let align = rect
            .cols
            .clone()
            .map(|c| data.align.get(c).copied().unwrap_or_default())
            .collect();
        let mut block = crate::edit::ops::new_block(
            BlockId(self.doc.next_id),
            BlockKind::Table(TableData { align, rows }),
            Inline::default(),
        );
        block.dirty = true;
        Some(block)
    }

    /// Empties the cells of a rectangle in one undo step; the caret goes to
    /// the start of `anchor`.
    pub(crate) fn clear_cells(&mut self, anchor: BlockId, head: BlockId) -> bool {
        let Some(rect) = self.cell_rect(anchor, head) else {
            return false;
        };
        let cells: Vec<(BlockId, usize)> = rect
            .cells
            .iter()
            .flatten()
            .map(|id| (*id, self.text_of(*id).map_or(0, |i| i.text.len())))
            .collect();
        self.transact(EditKind::Other, None, "", move |tr, _| {
            for (id, len) in cells {
                tr.replace(id, 0..len, Inline::default())?;
            }
            Ok(Selection::caret(TextPos {
                block: anchor,
                offset: 0,
            }))
        })
    }

    /// Typing or pasting over a cell rectangle replaces the text of its
    /// anchor cell only: the rectangle becomes a selection of that text.
    pub(crate) fn collapse_cells(&mut self) {
        if let Selection::Cells { anchor, .. } = self.selection {
            let end = self.end_of(anchor);
            self.put_selection(Selection::Text {
                anchor: TextPos {
                    block: anchor,
                    offset: 0,
                },
                head: end,
            });
        }
    }
}

/// A rectangle of cells, see [`EditorState::cell_rect`].
#[derive(Clone, Debug)]
pub(crate) struct CellRect {
    pub(crate) table: BlockId,
    pub(crate) rows: std::ops::Range<usize>,
    pub(crate) cols: std::ops::Range<usize>,
    pub(crate) cells: Vec<Vec<BlockId>>,
}
