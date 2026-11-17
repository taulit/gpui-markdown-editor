//! Steps: the smallest invertible changes of a document.

use std::fmt;
use std::ops::Range;

use gpui::SharedString;

use super::inline_ops;
use super::order::{self, Loc, Path, PathIndex};
use crate::model::{Align, Block, BlockId, BlockKind, Cell, Document, ImageData, Inline, Mark};

/// An attribute of a block that can be changed without touching its text.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Attr {
    /// Source, alt text, title and width of an image.
    Image(ImageData),
    /// The info string of a code block.
    #[allow(dead_code)] // set by the code block language picker of the next phase
    CodeLang(Option<SharedString>),
    /// Whether a task item is ticked.
    Checked(bool),
    /// The ordinal of an ordered item.
    Number(u32),
}

/// A change of a table's rows, columns or cells.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TableOp {
    InsertRow {
        index: usize,
        cells: Vec<Cell>,
    },
    RemoveRow {
        index: usize,
    },
    MoveRow {
        from: usize,
        to: usize,
    },
    InsertColumn {
        index: usize,
        align: Align,
        cells: Vec<Cell>,
    },
    RemoveColumn {
        index: usize,
    },
    MoveColumn {
        from: usize,
        to: usize,
    },
    SetAlign {
        column: usize,
        align: Align,
    },
}

/// One invertible change.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Step {
    /// Replaces a range of one text node with formatted text. Spans around
    /// the range follow the rules of [`inline_ops::replace`].
    ReplaceText {
        node: BlockId,
        range: Range<usize>,
        insert: Inline,
    },
    /// Replaces a range with plain text and then sets all spans of the node.
    /// The exact inverse of the text steps.
    SetText {
        node: BlockId,
        range: Range<usize>,
        text: String,
        marks: Vec<crate::model::MarkSpan>,
    },
    AddMark {
        node: BlockId,
        range: Range<usize>,
        mark: Mark,
    },
    /// Removes marks of the kind of `mark` (any link for a link).
    RemoveMark {
        node: BlockId,
        range: Range<usize>,
        mark: Mark,
    },
    SetMarks {
        node: BlockId,
        marks: Vec<crate::model::MarkSpan>,
    },
    SetBlockKind {
        block: BlockId,
        kind: BlockKind,
    },
    SetAttr {
        block: BlockId,
        attr: Attr,
    },
    /// Inserts a block into the children of `parent` (`None`: the top level).
    InsertBlock {
        parent: Option<BlockId>,
        index: usize,
        block: Box<Block>,
    },
    /// Removes a block with its children.
    RemoveBlock {
        block: BlockId,
    },
    /// Moves a block with its children. `index` counts in the new parent's
    /// children after the block was taken out.
    MoveBlock {
        block: BlockId,
        parent: Option<BlockId>,
        index: usize,
    },
    TableOp {
        table: BlockId,
        op: TableOp,
    },
}

/// Why a step could not be applied. Steps built by the editor are valid, so
/// this points at a bug; the transaction is rolled back.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StepError(pub(crate) String);

impl fmt::Display for StepError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn err<T>(message: impl Into<String>) -> Result<T, StepError> {
    Err(StepError(message.into()))
}

/// The effect of an applied step.
pub(crate) struct Outcome {
    pub(crate) inverse: Step,
    /// Inserts, removals and moves of blocks, rows or columns.
    pub(crate) structural: bool,
}

/// Which blocks a step changes, computed before it is applied.
#[derive(Default)]
pub(crate) struct Affected {
    /// Top-level blocks whose source no longer matches.
    pub(crate) tops: Vec<BlockId>,
    /// A block that moves into or out of the top level: its own source
    /// range no longer describes it.
    pub(crate) moved: Option<BlockId>,
    /// Parents whose list of children changes (for renumbering).
    pub(crate) parents: Vec<Option<BlockId>>,
}

fn locate(doc: &Document, index: &PathIndex, id: BlockId) -> Result<Loc, StepError> {
    index
        .locate(doc, id)
        .ok_or_else(|| StepError(format!("no node {id:?}")))
}

fn block_path(doc: &Document, index: &PathIndex, id: BlockId) -> Result<Path, StepError> {
    match locate(doc, index, id)? {
        Loc::Block(path) => Ok(path),
        Loc::Cell { .. } => err(format!("{id:?} is a cell, not a block")),
    }
}

fn top_id(doc: &Document, loc: &Loc) -> Option<BlockId> {
    doc.blocks.get(loc.top()).map(|block| block.id)
}

fn parent_id(doc: &Document, path: &[usize]) -> Option<BlockId> {
    let parent = path.get(..path.len().checked_sub(1)?)?;
    order::block_at(&doc.blocks, parent).map(|block| block.id)
}

fn check_range(text: &str, range: &Range<usize>) -> Result<(), StepError> {
    if range.start > range.end
        || range.end > text.len()
        || !text.is_char_boundary(range.start)
        || !text.is_char_boundary(range.end)
    {
        return err(format!("range {range:?} invalid for {text:?}"));
    }
    Ok(())
}

impl Step {
    pub(crate) fn affected(
        &self,
        doc: &Document,
        index: &PathIndex,
    ) -> Result<Affected, StepError> {
        let mut out = Affected::default();
        let top_of = |id: BlockId| -> Result<Option<BlockId>, StepError> {
            Ok(top_id(doc, &locate(doc, index, id)?))
        };
        match self {
            Step::ReplaceText { node, .. }
            | Step::SetText { node, .. }
            | Step::AddMark { node, .. }
            | Step::RemoveMark { node, .. }
            | Step::SetMarks { node, .. } => out.tops.extend(top_of(*node)?),
            Step::SetBlockKind { block, .. } | Step::SetAttr { block, .. } => {
                let path = block_path(doc, index, *block)?;
                out.tops.extend(top_id(doc, &Loc::Block(path.clone())));
                out.parents.push(parent_id(doc, &path));
            }
            Step::TableOp { table, .. } => out.tops.extend(top_of(*table)?),
            Step::InsertBlock { parent, .. } => {
                if let Some(parent) = parent {
                    out.tops.extend(top_of(*parent)?);
                }
                out.parents.push(*parent);
            }
            Step::RemoveBlock { block } => {
                let path = block_path(doc, index, *block)?;
                let parent = parent_id(doc, &path);
                if parent.is_some() {
                    out.tops.extend(top_id(doc, &Loc::Block(path)));
                }
                out.parents.push(parent);
            }
            Step::MoveBlock { block, parent, .. } => {
                let path = block_path(doc, index, *block)?;
                let old_parent = parent_id(doc, &path);
                if old_parent.is_some() {
                    out.tops.extend(top_id(doc, &Loc::Block(path)));
                }
                if let Some(parent) = parent {
                    out.tops.extend(top_of(*parent)?);
                }
                if old_parent.is_none() != parent.is_none() {
                    out.moved = Some(*block);
                }
                out.parents.push(old_parent);
                out.parents.push(*parent);
            }
        }
        Ok(out)
    }

    /// Applies the step and returns its inverse. On error the document is
    /// unchanged.
    pub(crate) fn apply(
        &self,
        doc: &mut Document,
        index: &PathIndex,
    ) -> Result<Outcome, StepError> {
        let text_outcome = |inverse| {
            Ok(Outcome {
                inverse,
                structural: false,
            })
        };
        match self {
            Step::ReplaceText {
                node,
                range,
                insert,
            } => {
                let loc = locate(doc, index, *node)?;
                let Some(inline) = order::inline_at_mut(doc, &loc) else {
                    return err("node has no text");
                };
                check_range(&inline.text, range)?;
                for span in &insert.marks {
                    check_range(&insert.text, &span.range)?;
                }
                let inverse = Step::SetText {
                    node: *node,
                    range: range.start..range.start + insert.text.len(),
                    text: inline.text[range.clone()].to_string(),
                    marks: inline.marks.clone(),
                };
                inline_ops::replace(inline, range.clone(), insert);
                text_outcome(inverse)
            }
            Step::SetText {
                node,
                range,
                text,
                marks,
            } => {
                let loc = locate(doc, index, *node)?;
                let Some(inline) = order::inline_at_mut(doc, &loc) else {
                    return err("node has no text");
                };
                check_range(&inline.text, range)?;
                let new_len = inline.text.len() - range.len() + text.len();
                if marks.iter().any(|span| span.range.end > new_len) {
                    return err("marks outside text");
                }
                let inverse = Step::SetText {
                    node: *node,
                    range: range.start..range.start + text.len(),
                    text: inline.text[range.clone()].to_string(),
                    marks: inline.marks.clone(),
                };
                inline.text.replace_range(range.clone(), text);
                inline.marks = marks.clone();
                text_outcome(inverse)
            }
            Step::AddMark { node, range, mark } | Step::RemoveMark { node, range, mark } => {
                let loc = locate(doc, index, *node)?;
                let Some(inline) = order::inline_at_mut(doc, &loc) else {
                    return err("node has no text");
                };
                check_range(&inline.text, range)?;
                let inverse = Step::SetMarks {
                    node: *node,
                    marks: inline.marks.clone(),
                };
                if matches!(self, Step::AddMark { .. }) {
                    inline_ops::add_mark(inline, range.clone(), mark.clone());
                } else {
                    inline_ops::remove_mark(inline, range.clone(), mark);
                }
                text_outcome(inverse)
            }
            Step::SetMarks { node, marks } => {
                let loc = locate(doc, index, *node)?;
                let Some(inline) = order::inline_at_mut(doc, &loc) else {
                    return err("node has no text");
                };
                for span in marks {
                    check_range(&inline.text, &span.range)?;
                }
                let inverse = Step::SetMarks {
                    node: *node,
                    marks: std::mem::replace(&mut inline.marks, marks.clone()),
                };
                text_outcome(inverse)
            }
            Step::SetBlockKind { block, kind } => {
                let path = block_path(doc, index, *block)?;
                let Some(target) = order::block_at_mut(&mut doc.blocks, &path) else {
                    return err("no block");
                };
                if !target.children.is_empty() && !order::holds_children(kind) {
                    return err("kind cannot hold the children");
                }
                let old = std::mem::replace(&mut target.kind, kind.clone());
                // A table's cells are stops; changing to or from a table
                // changes the structure.
                let structural =
                    matches!(old, BlockKind::Table(_)) || matches!(kind, BlockKind::Table(_));
                if structural {
                    index.invalidate();
                }
                Ok(Outcome {
                    inverse: Step::SetBlockKind {
                        block: *block,
                        kind: old,
                    },
                    structural,
                })
            }
            Step::SetAttr { block, attr } => {
                let path = block_path(doc, index, *block)?;
                let Some(target) = order::block_at_mut(&mut doc.blocks, &path) else {
                    return err("no block");
                };
                let old = match (&mut target.kind, attr) {
                    (BlockKind::Image(data), Attr::Image(new)) => {
                        Attr::Image(std::mem::replace(data, new.clone()))
                    }
                    (BlockKind::CodeBlock { lang }, Attr::CodeLang(new)) => {
                        Attr::CodeLang(std::mem::replace(lang, new.clone()))
                    }
                    (BlockKind::TaskItem { checked }, Attr::Checked(new)) => {
                        Attr::Checked(std::mem::replace(checked, *new))
                    }
                    (BlockKind::OrderedItem { number }, Attr::Number(new)) => {
                        Attr::Number(std::mem::replace(number, *new))
                    }
                    _ => return err("attribute does not fit the block kind"),
                };
                text_outcome(Step::SetAttr {
                    block: *block,
                    attr: old,
                })
            }
            Step::InsertBlock {
                parent,
                index: at,
                block,
            } => {
                // Ids come from `Document::alloc_id` or from a removed block;
                // checking costs a walk of the document, so only in tests.
                if cfg!(debug_assertions) && order::find_loc(doc, block.id).is_some() {
                    return err(format!("id {:?} exists", block.id));
                }
                let parent_path = match parent {
                    Some(parent) => {
                        let path = block_path(doc, index, *parent)?;
                        match order::block_at(&doc.blocks, &path) {
                            Some(p) if order::holds_children(&p.kind) => path,
                            _ => return err("parent cannot hold children"),
                        }
                    }
                    None => Path::new(),
                };
                let Some(siblings) = order::siblings_mut(&mut doc.blocks, &parent_path) else {
                    return err("no parent");
                };
                if *at > siblings.len() {
                    return err("index out of range");
                }
                siblings.insert(*at, (**block).clone());
                index.inserted(&parent_path, *at, block);
                Ok(Outcome {
                    inverse: Step::RemoveBlock { block: block.id },
                    structural: true,
                })
            }
            Step::RemoveBlock { block } => {
                let path = block_path(doc, index, *block)?;
                let parent = parent_id(doc, &path);
                let (at, parent_path) = path.split_last().map(|(a, p)| (*a, p)).unwrap_or((0, &[]));
                let Some(siblings) = order::siblings_mut(&mut doc.blocks, parent_path) else {
                    return err("no parent");
                };
                if at >= siblings.len() {
                    return err("index out of range");
                }
                let removed = siblings.remove(at);
                index.removed(parent_path, at, &removed);
                Ok(Outcome {
                    inverse: Step::InsertBlock {
                        parent,
                        index: at,
                        block: Box::new(removed),
                    },
                    structural: true,
                })
            }
            Step::MoveBlock {
                block,
                parent,
                index: at,
            } => {
                let path = block_path(doc, index, *block)?;
                let old_parent = parent_id(doc, &path);
                let mut target = match parent {
                    Some(parent) => {
                        let target = block_path(doc, index, *parent)?;
                        match order::block_at(&doc.blocks, &target) {
                            Some(p) if order::holds_children(&p.kind) => {}
                            _ => return err("parent cannot hold children"),
                        }
                        if target.starts_with(&path) {
                            return err("cannot move a block into itself");
                        }
                        target
                    }
                    None => Path::new(),
                };
                let Some((&old_index, old_parent_path)) = path.split_last() else {
                    return err("empty path");
                };
                // Taking the block out shifts later siblings and their
                // descendants, which may include the target.
                let depth = old_parent_path.len();
                if target.len() > depth
                    && target.starts_with(old_parent_path)
                    && target[depth] > old_index
                {
                    target[depth] -= 1;
                }
                let len = match order::siblings(&doc.blocks, &target) {
                    Some(siblings) => {
                        siblings.len() - usize::from(target.as_slice() == old_parent_path)
                    }
                    None => return err("no target"),
                };
                if *at > len {
                    return err("index out of range");
                }
                let Some(siblings) = order::siblings_mut(&mut doc.blocks, old_parent_path) else {
                    return err("no parent");
                };
                let moved = siblings.remove(old_index);
                index.removed(old_parent_path, old_index, &moved);
                let Some(siblings) = order::siblings_mut(&mut doc.blocks, &target) else {
                    return err("no target");
                };
                siblings.insert(*at, moved);
                index.inserted(&target, *at, &siblings[*at]);
                Ok(Outcome {
                    inverse: Step::MoveBlock {
                        block: *block,
                        parent: old_parent,
                        index: old_index,
                    },
                    structural: true,
                })
            }
            Step::TableOp { table, op } => {
                let path = block_path(doc, index, *table)?;
                let old: Vec<BlockId> = match order::block_at(&doc.blocks, &path).map(|b| &b.kind) {
                    Some(BlockKind::Table(data)) => {
                        data.rows.iter().flatten().map(|c| c.id).collect()
                    }
                    _ => Vec::new(),
                };
                let outcome = apply_table(doc, &path, *table, op)?;
                if outcome.structural
                    && let Some(block) = order::block_at(&doc.blocks, &path)
                {
                    index.table_changed(&path, &old, block);
                }
                Ok(outcome)
            }
        }
    }
}

fn apply_table(
    doc: &mut Document,
    path: &[usize],
    table: BlockId,
    op: &TableOp,
) -> Result<Outcome, StepError> {
    let fresh_ids = |cells: &[Cell], doc: &Document| {
        cells
            .iter()
            .all(|cell| order::find_loc(doc, cell.id).is_none())
    };
    if let TableOp::InsertRow { cells, .. } | TableOp::InsertColumn { cells, .. } = op
        && cfg!(debug_assertions)
        && !fresh_ids(cells, doc)
    {
        return err("cell id exists");
    }
    let data = match order::block_at_mut(&mut doc.blocks, path).map(|block| &mut block.kind) {
        Some(BlockKind::Table(data)) => data,
        _ => return err("not a table"),
    };
    let width = data.align.len();
    let height = data.rows.len();
    let wrap = |op, structural| {
        Ok(Outcome {
            inverse: Step::TableOp { table, op },
            structural,
        })
    };
    match op {
        TableOp::InsertRow { index, cells } => {
            if *index > height || cells.len() != width {
                return err("row does not fit");
            }
            data.rows.insert(*index, cells.clone());
            wrap(TableOp::RemoveRow { index: *index }, true)
        }
        TableOp::RemoveRow { index } => {
            if *index >= height || height == 1 {
                return err("cannot remove row");
            }
            let cells = data.rows.remove(*index);
            wrap(
                TableOp::InsertRow {
                    index: *index,
                    cells,
                },
                true,
            )
        }
        TableOp::MoveRow { from, to } => {
            if *from >= height || *to >= height {
                return err("row out of range");
            }
            let row = data.rows.remove(*from);
            data.rows.insert(*to, row);
            wrap(
                TableOp::MoveRow {
                    from: *to,
                    to: *from,
                },
                true,
            )
        }
        TableOp::InsertColumn {
            index,
            align,
            cells,
        } => {
            if *index > width || cells.len() != height {
                return err("column does not fit");
            }
            data.align.insert(*index, *align);
            for (row, cell) in data.rows.iter_mut().zip(cells) {
                row.insert(*index, cell.clone());
            }
            wrap(TableOp::RemoveColumn { index: *index }, true)
        }
        TableOp::RemoveColumn { index } => {
            if *index >= width || width == 1 {
                return err("cannot remove column");
            }
            let align = data.align.remove(*index);
            let cells = data.rows.iter_mut().map(|row| row.remove(*index)).collect();
            wrap(
                TableOp::InsertColumn {
                    index: *index,
                    align,
                    cells,
                },
                true,
            )
        }
        TableOp::MoveColumn { from, to } => {
            if *from >= width || *to >= width {
                return err("column out of range");
            }
            let align = data.align.remove(*from);
            data.align.insert(*to, align);
            for row in &mut data.rows {
                let cell = row.remove(*from);
                row.insert(*to, cell);
            }
            wrap(
                TableOp::MoveColumn {
                    from: *to,
                    to: *from,
                },
                true,
            )
        }
        TableOp::SetAlign { column, align } => {
            let Some(slot) = data.align.get_mut(*column) else {
                return err("column out of range");
            };
            let old = std::mem::replace(slot, *align);
            wrap(
                TableOp::SetAlign {
                    column: *column,
                    align: old,
                },
                false,
            )
        }
    }
}
