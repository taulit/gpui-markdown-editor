//! Block commands: block kinds, indentation, tasks, moving and inserting
//! blocks.

use crate::edit::Selection;
use crate::edit::history::EditKind;
use crate::edit::ops::{Abort, EditResult};
use crate::edit::order::{self, Loc};
use crate::edit::state::EditorState;
use crate::edit::step::{Attr, Step};
use crate::edit::transaction::Tr;
use crate::model::{Align, Block, BlockId, BlockKind, Cell, ImageData, Inline, TableData, TextPos};

/// The most rows or columns [`EditorState::insert_table`] creates.
const MAX_TABLE_SIZE: usize = 64;

/// Whether a block of kind `a` already is what `b` asks for: the same
/// variant, and for headings the same level (H1 -> H2 changes the level,
/// H2 -> H2 toggles back to a paragraph).
fn same_variant(a: &BlockKind, b: &BlockKind) -> bool {
    match (a, b) {
        (BlockKind::Heading(x), BlockKind::Heading(y)) => x == y,
        _ => std::mem::discriminant(a) == std::mem::discriminant(b),
    }
}

impl EditorState {
    /// Turns every selected block into `kind`. If all of them already are of
    /// that kind (ignoring its attributes), they become paragraphs: the
    /// block shortcuts toggle.
    pub(crate) fn set_block(&mut self, kind: BlockKind) -> bool {
        let settable = matches!(
            kind,
            BlockKind::Paragraph
                | BlockKind::Heading(1..=6)
                | BlockKind::BulletItem
                | BlockKind::OrderedItem { .. }
                | BlockKind::TaskItem { .. }
                | BlockKind::Quote
                | BlockKind::CodeBlock { .. }
        );
        if !settable {
            return false;
        }
        let ids = self.selected_blocks();
        if ids.is_empty() {
            return false;
        }
        let all_same = ids.iter().all(|id| {
            self.locate(*id)
                .and_then(|loc| order::block_at(&self.doc.blocks, loc.block_path()))
                .is_some_and(|block| same_variant(&block.kind, &kind))
        });
        let target = if all_same && kind != BlockKind::Paragraph {
            BlockKind::Paragraph
        } else {
            kind
        };
        self.transact(EditKind::Other, None, "", move |tr, sel| {
            for id in ids {
                tr.set_kind(id, target.clone())?;
            }
            Ok(sel.clone())
        })
    }

    /// Selected list items without a selected ancestor.
    fn selected_items(&self) -> Vec<BlockId> {
        self.selected_roots()
            .into_iter()
            .filter(|id| {
                self.locate(*id)
                    .and_then(|loc| order::block_at(&self.doc.blocks, loc.block_path()))
                    .is_some_and(|block| order::is_item(&block.kind))
            })
            .collect()
    }

    /// Makes each selected item the last child of the item above it. Its
    /// children move along.
    pub(crate) fn indent(&mut self) -> bool {
        let ids = self.selected_items();
        if ids.is_empty() {
            return false;
        }
        self.transact(EditKind::Other, None, "", move |tr, sel| {
            let mut moved = false;
            for id in ids {
                let (parent, index) = tr.parent(id)?;
                let Some(previous) = index.checked_sub(1) else {
                    continue;
                };
                let (prev_id, prev_children, prev_is_item, last_ordered) = {
                    let siblings = siblings(tr, parent)?;
                    let Some(prev) = siblings.get(previous) else {
                        continue;
                    };
                    (
                        prev.id,
                        prev.children.len(),
                        order::is_item(&prev.kind),
                        prev.children
                            .last()
                            .is_some_and(|c| matches!(c.kind, BlockKind::OrderedItem { .. })),
                    )
                };
                if !prev_is_item {
                    continue;
                }
                if let BlockKind::OrderedItem { number } = tr.block(id)?.kind
                    && !last_ordered
                    && number != 1
                {
                    tr.step(Step::SetAttr {
                        block: id,
                        attr: Attr::Number(1),
                    })?;
                }
                tr.move_block(id, Some(prev_id), prev_children)?;
                moved = true;
            }
            if moved {
                Ok(sel.clone())
            } else {
                Err(Abort::NotApplicable)
            }
        })
    }

    /// Moves each selected nested item up one level, behind its parent item.
    /// The items below it in the same list become its children.
    pub(crate) fn outdent(&mut self) -> bool {
        let ids = self.selected_items();
        self.outdent_blocks(ids)
    }

    pub(crate) fn outdent_blocks(&mut self, ids: Vec<BlockId>) -> bool {
        if ids.is_empty() {
            return false;
        }
        self.transact(EditKind::Other, None, "", move |tr, sel| {
            let mut moved = false;
            for id in ids {
                let (Some(parent), index) = tr.parent(id)? else {
                    continue;
                };
                if !order::is_item(&tr.block(parent)?.kind) {
                    continue;
                }
                let following: Vec<BlockId> = tr.block(parent)?.children[index + 1..]
                    .iter()
                    .map(|b| b.id)
                    .collect();
                for follower in following {
                    let end = tr.block(id)?.children.len();
                    tr.move_block(follower, Some(id), end)?;
                }
                let (grandparent, parent_index) = tr.parent(parent)?;
                tr.move_block(id, grandparent, parent_index + 1)?;
                moved = true;
            }
            if moved {
                Ok(sel.clone())
            } else {
                Err(Abort::NotApplicable)
            }
        })
    }

    /// Ticks the selected task items, or unticks them if all are ticked.
    pub(crate) fn toggle_task(&mut self) -> bool {
        let tasks: Vec<(BlockId, bool)> = self
            .selected_blocks()
            .into_iter()
            .filter_map(|id| {
                let loc = self.locate(id)?;
                match order::block_at(&self.doc.blocks, loc.block_path())?.kind {
                    BlockKind::TaskItem { checked } => Some((id, checked)),
                    _ => None,
                }
            })
            .collect();
        if tasks.is_empty() {
            return false;
        }
        let target = !tasks.iter().all(|(_, checked)| *checked);
        self.set_checked(tasks, target)
    }

    /// Ticks or unticks one task item (a click on its checkbox), keeping
    /// the selection.
    pub(crate) fn toggle_task_item(&mut self, id: BlockId) -> bool {
        let checked = self
            .locate(id)
            .and_then(|loc| order::block_at(&self.doc.blocks, loc.block_path()))
            .and_then(|block| match block.kind {
                BlockKind::TaskItem { checked } => Some(checked),
                _ => None,
            });
        match checked {
            Some(checked) => self.set_checked(vec![(id, checked)], !checked),
            None => false,
        }
    }

    fn set_checked(&mut self, tasks: Vec<(BlockId, bool)>, target: bool) -> bool {
        let changes: Vec<BlockId> = tasks
            .into_iter()
            .filter(|(_, checked)| *checked != target)
            .map(|(id, _)| id)
            .collect();
        if changes.is_empty() {
            return false;
        }
        self.transact(EditKind::Other, None, "", move |tr, sel| {
            for id in changes {
                tr.step(Step::SetAttr {
                    block: id,
                    attr: Attr::Checked(target),
                })?;
            }
            Ok(sel.clone())
        })
    }

    /// Ticks (or unticks) every task item of the document.
    pub(crate) fn check_all(&mut self, checked: bool) -> bool {
        fn collect(blocks: &[Block], out: &mut Vec<(BlockId, bool)>) {
            for block in blocks {
                if let BlockKind::TaskItem { checked } = block.kind {
                    out.push((block.id, checked));
                }
                collect(&block.children, out);
            }
        }
        let mut tasks = Vec::new();
        collect(&self.doc.blocks, &mut tasks);
        self.set_checked(tasks, checked)
    }

    /// In every run of sibling task items, moves the ticked ones below the
    /// open ones, keeping their order.
    pub(crate) fn move_checked_to_bottom(&mut self) -> bool {
        struct Run {
            parent: Option<BlockId>,
            start: usize,
            order: Vec<BlockId>,
        }
        fn collect(blocks: &[Block], parent: Option<BlockId>, out: &mut Vec<Run>) {
            let mut index = 0;
            while index < blocks.len() {
                let start = index;
                while index < blocks.len()
                    && matches!(blocks[index].kind, BlockKind::TaskItem { .. })
                {
                    index += 1;
                }
                if index > start {
                    let run = &blocks[start..index];
                    let checked =
                        |b: &Block| matches!(b.kind, BlockKind::TaskItem { checked: true });
                    let order: Vec<BlockId> = run
                        .iter()
                        .filter(|b| !checked(b))
                        .chain(run.iter().filter(|b| checked(b)))
                        .map(|b| b.id)
                        .collect();
                    if order.iter().zip(run).any(|(id, b)| *id != b.id) {
                        out.push(Run {
                            parent,
                            start,
                            order,
                        });
                    }
                } else {
                    index += 1;
                }
            }
            for block in blocks {
                collect(&block.children, Some(block.id), out);
            }
        }
        let mut runs = Vec::new();
        collect(&self.doc.blocks, None, &mut runs);
        if runs.is_empty() {
            return false;
        }
        self.transact(EditKind::Other, None, "", move |tr, sel| {
            for run in runs {
                for (offset, id) in run.order.into_iter().enumerate() {
                    let target = run.start + offset;
                    if tr.parent(id)?.1 != target {
                        tr.move_block(id, run.parent, target)?;
                    }
                }
            }
            Ok(sel.clone())
        })
    }

    /// Alt+Up/Down: moves the selected sibling blocks (with their children)
    /// past their neighbour.
    pub(crate) fn move_block(&mut self, down: bool) -> bool {
        let mut ids = match &self.selection {
            Selection::Node(id) => vec![*id],
            Selection::Cells { .. } => Vec::new(),
            Selection::Text { .. } | Selection::All => self.selected_roots(),
        };
        if ids.is_empty() {
            ids.extend(self.current_block());
        }
        let paths: Vec<order::Path> = ids
            .iter()
            .filter_map(|id| match self.locate(*id)? {
                Loc::Block(path) => Some(path),
                Loc::Cell { table, .. } => Some(table),
            })
            .collect();
        let (Some(first), Some(last)) = (paths.first(), paths.last()) else {
            return false;
        };
        let parent = &first[..first.len() - 1];
        let contiguous = paths.iter().enumerate().all(|(n, path)| {
            path.len() == first.len()
                && path.starts_with(parent)
                && path.last() == first.last().map(|i| i + n).as_ref()
        });
        let (from, to) = if contiguous {
            (*first.last().unwrap_or(&0), *last.last().unwrap_or(&0))
        } else {
            let index = *first.last().unwrap_or(&0);
            (index, index)
        };
        let Some(siblings) = order::siblings(&self.doc.blocks, parent) else {
            return false;
        };
        let neighbour = if down {
            siblings.get(to + 1).map(|b| (b.id, from))
        } else {
            from.checked_sub(1)
                .and_then(|i| siblings.get(i))
                .map(|b| (b.id, to))
        };
        let Some((neighbour, target)) = neighbour else {
            return false;
        };
        let parent_id = if parent.is_empty() {
            None
        } else {
            order::block_at(&self.doc.blocks, parent).map(|b| b.id)
        };
        self.transact(EditKind::Other, None, "", move |tr, sel| {
            tr.move_block(neighbour, parent_id, target)?;
            Ok(sel.clone())
        })
    }

    /// The block new blocks are inserted after: the block at the end of the
    /// selection, or the table of a cell.
    fn insertion_anchor(&self) -> Option<BlockId> {
        let pos = match &self.selection {
            Selection::Node(id) => return Some(*id),
            Selection::All => return self.doc.blocks.last().map(|b| b.id),
            Selection::Text { .. } => self.ordered()?.1,
            Selection::Cells { head, .. } => return self.cell_at(*head).map(|at| at.table),
        };
        match self.locate(pos.block)? {
            Loc::Block(_) => Some(pos.block),
            Loc::Cell { table, .. } => order::block_at(&self.doc.blocks, &table).map(|b| b.id),
        }
    }

    /// Inserts a node block (rule, image, table) below the current block. An
    /// empty paragraph is kept below it for the caret.
    fn insert_node_block(&mut self, kind: BlockKind) -> bool {
        let Some(anchor) = self.insertion_anchor() else {
            return false;
        };
        self.transact(EditKind::Other, None, "", move |tr, _| {
            let block = tr.new_block(kind, Inline::default());
            let new = block.id;
            let anchor_block = tr.block(anchor)?;
            let reuse = anchor_block.kind == BlockKind::Paragraph
                && anchor_block.content.text.is_empty()
                && anchor_block.children.is_empty();
            if reuse {
                tr.insert_before(anchor, block)?;
            } else {
                tr.insert_after(anchor, block)?;
            }
            caret_after_node(tr, new)
        })
    }

    pub(crate) fn insert_rule(&mut self) -> bool {
        self.insert_node_block(BlockKind::Rule)
    }

    pub(crate) fn insert_image(&mut self, src: &str, alt: &str) -> bool {
        self.insert_node_block(BlockKind::Image(ImageData {
            src: src.to_string().into(),
            alt: alt.to_string().into(),
            title: None,
            width: None,
        }))
    }

    /// Inserts an empty table and puts the caret into its first cell.
    pub(crate) fn insert_table(&mut self, rows: usize, cols: usize) -> bool {
        let rows = rows.clamp(1, MAX_TABLE_SIZE);
        let cols = cols.clamp(1, MAX_TABLE_SIZE);
        let Some(anchor) = self.insertion_anchor() else {
            return false;
        };
        self.transact(EditKind::Other, None, "", move |tr, _| {
            let data = TableData {
                align: vec![Align::None; cols],
                rows: (0..rows)
                    .map(|_| (0..cols).map(|_| new_cell(tr)).collect())
                    .collect(),
            };
            let first = data.rows[0][0].id;
            let block = tr.new_block(BlockKind::Table(data), Inline::default());
            let anchor_block = tr.block(anchor)?;
            let reuse = anchor_block.kind == BlockKind::Paragraph
                && anchor_block.content.text.is_empty()
                && anchor_block.children.is_empty();
            if reuse {
                tr.insert_before(anchor, block)?;
            } else {
                let new = tr.insert_after(anchor, block)?;
                ensure_text_after(tr, new)?;
            }
            Ok(Selection::caret(TextPos {
                block: first,
                offset: 0,
            }))
        })
    }
}

pub(crate) fn new_cell(tr: &mut Tr<'_>) -> Cell {
    Cell {
        id: tr.alloc_id(),
        content: Inline::default(),
    }
}

fn siblings<'t>(tr: &'t Tr<'_>, parent: Option<BlockId>) -> EditResult<&'t [Block]> {
    match parent {
        None => Ok(&tr.doc().blocks),
        Some(id) => Ok(&tr.block(id)?.children),
    }
}

/// Makes sure a text block follows the node `id`; returns its id.
pub(crate) fn ensure_text_after(tr: &mut Tr<'_>, id: BlockId) -> EditResult<BlockId> {
    let Loc::Block(path) = tr.loc(id)? else {
        return Err(Abort::NotApplicable);
    };
    match order::next_after_subtree(tr.doc(), &path) {
        Some(stop) if stop.text && !stop.loc.is_cell() => Ok(stop.id),
        _ => {
            let block = tr.new_block(BlockKind::Paragraph, Inline::default());
            Ok(tr.insert_after(id, block)?)
        }
    }
}

/// The caret at the start of the text block after the node `id`.
fn caret_after_node(tr: &mut Tr<'_>, id: BlockId) -> EditResult<Selection> {
    let next = ensure_text_after(tr, id)?;
    Ok(Selection::caret(TextPos {
        block: next,
        offset: 0,
    }))
}
