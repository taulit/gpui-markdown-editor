//! Copy and paste: the selection as text or Markdown, and inserting
//! Markdown fragments.

use std::collections::HashSet;
use std::sync::Arc;

use crate::edit::Selection;
use crate::edit::history::EditKind;
use crate::edit::inline_ops;
use crate::edit::ops::{Abort, EditResult};
use crate::edit::order::{self, Loc};
use crate::edit::state::EditorState;
use crate::edit::transaction::Tr;
use crate::model::{Block, BlockId, BlockKind, Document, Inline, TextPos};

impl EditorState {
    /// The selected text without formatting. Blocks are separated by line
    /// breaks.
    pub(crate) fn selected_text(&self) -> String {
        let Some((start, end)) = self.ordered() else {
            return String::new();
        };
        let (Some(from), Some(to)) = (self.locate(start.block), self.locate(end.block)) else {
            return String::new();
        };
        let mut parts = Vec::new();
        for stop in order::stops_between(&self.doc, &from, &to) {
            let Some(inline) = order::inline_at(&self.doc, &stop.loc) else {
                continue;
            };
            let a = if stop.id == start.block { start.offset } else { 0 };
            let b = if stop.id == end.block { end.offset } else { inline.text.len() };
            parts.push(inline.text.get(a..b).unwrap_or_default().to_string());
        }
        parts.join("\n")
    }

    /// The selection as Markdown in the normalized style. Partly selected
    /// blocks contribute their selected text; tables are copied whole.
    pub(crate) fn selected_markdown(&self) -> String {
        let blocks = match &self.selection {
            Selection::Node(id) => self
                .locate(*id)
                .and_then(|loc| order::block_at(&self.doc.blocks, loc.block_path()))
                .map(|block| vec![detach(block.clone())])
                .unwrap_or_default(),
            Selection::Text { .. } => {
                let Some((start, end)) = self.ordered() else {
                    return String::new();
                };
                if start == end {
                    return String::new();
                }
                let (Some(from), Some(to)) = (self.locate(start.block), self.locate(end.block))
                else {
                    return String::new();
                };
                let mut selected = HashSet::new();
                for stop in order::stops_between(&self.doc, &from, &to) {
                    match &stop.loc {
                        Loc::Cell { table, .. } => {
                            if let Some(table) = order::block_at(&self.doc.blocks, table) {
                                selected.insert(table.id);
                            }
                        }
                        Loc::Block(_) => {
                            selected.insert(stop.id);
                        }
                    }
                }
                prune(&self.doc.blocks, &selected, start, end)
            }
        };
        let fragment = Document {
            blocks,
            source: Arc::from(""),
            next_id: self.doc.next_id,
            extensions: self.doc.extensions.clone(),
        };
        fragment.normalized()
    }

    /// Parses `markdown` and inserts its blocks at the selection. Text that
    /// is only paragraphs is inserted inline: the first paragraph joins the
    /// current block, the text after the caret joins the last.
    pub(crate) fn insert_markdown(&mut self, markdown: &str) -> bool {
        let fragment = Document::parse_with(markdown, self.doc.extensions.clone());
        let blocks = fragment.blocks;
        if blocks.is_empty() {
            return false;
        }
        let inline_only = blocks
            .iter()
            .all(|b| b.kind == BlockKind::Paragraph && b.children.is_empty());
        let in_cell = self
            .selection
            .head()
            .and_then(|head| self.locate(head.block))
            .is_some_and(|loc| loc.is_cell());
        if inline_only && !in_cell {
            let lines = blocks.into_iter().map(|b| b.content).collect();
            return self.insert_inline_lines(lines);
        }
        if in_cell {
            let mut text = String::new();
            flatten(&blocks, &mut text);
            return self.type_text(text.trim(), std::time::Instant::now());
        }
        let selection = self.selection.clone();
        let range = self.ordered();
        self.transact(EditKind::Other, None, "", move |tr, _| {
            let blocks: Vec<Block> = blocks.into_iter().map(|b| fresh(tr, b)).collect();
            match selection {
                Selection::Node(id) => insert_blocks_after(tr, id, blocks),
                Selection::Text { .. } => {
                    let (start, end) = range.ok_or(Abort::NotApplicable)?;
                    let pos = tr.delete_range(start, end)?;
                    if tr.is_cell(pos.block) {
                        let mut text = String::new();
                        flatten(&blocks, &mut text);
                        let text = text.trim().to_string();
                        let len = text.len();
                        tr.replace(pos.block, pos.offset..pos.offset, Inline::plain(text))?;
                        return Ok(Selection::caret(TextPos {
                            block: pos.block,
                            offset: pos.offset + len,
                        }));
                    }
                    insert_blocks_at(tr, pos, blocks)
                }
            }
        })
    }
}

/// Inserts whole blocks at `pos`, splitting the block there if needed.
fn insert_blocks_at(tr: &mut Tr<'_>, pos: TextPos, blocks: Vec<Block>) -> EditResult<Selection> {
    let block = tr.block(pos.block)?;
    let len = block.content.text.len();
    let kind = block.kind.clone();
    let empty_paragraph =
        kind == BlockKind::Paragraph && len == 0 && block.children.is_empty();
    let last_is_text = blocks.last().is_some_and(|b| order::has_text(&b.kind));
    let (parent, index) = tr.parent(pos.block)?;
    let (at, follower) = if empty_paragraph {
        (index, Some(pos.block))
    } else if pos.offset == len {
        (index + 1, None)
    } else if pos.offset == 0 {
        (index, Some(pos.block))
    } else {
        let tail_kind = match kind {
            BlockKind::TaskItem { .. } => BlockKind::TaskItem { checked: false },
            other => other,
        };
        let tail = tr.split(pos, tail_kind)?;
        (index + 1, Some(tail))
    };
    let ids: Vec<BlockId> = blocks.iter().map(|b| b.id).collect();
    for (offset, block) in blocks.into_iter().enumerate() {
        tr.insert_block(parent, at + offset, block)?;
    }
    if empty_paragraph && last_is_text {
        tr.remove(pos.block)?;
    }
    caret_after_inserted(tr, &ids, follower)
}

fn insert_blocks_after(tr: &mut Tr<'_>, id: BlockId, blocks: Vec<Block>) -> EditResult<Selection> {
    let (parent, index) = tr.parent(id)?;
    let ids: Vec<BlockId> = blocks.iter().map(|b| b.id).collect();
    for (offset, block) in blocks.into_iter().enumerate() {
        tr.insert_block(parent, index + 1 + offset, block)?;
    }
    caret_after_inserted(tr, &ids, None)
}

/// The caret at the end of the last inserted text, or at the start of
/// `follower`.
fn caret_after_inserted(
    tr: &mut Tr<'_>,
    ids: &[BlockId],
    follower: Option<BlockId>,
) -> EditResult<Selection> {
    let last = ids.last().copied().ok_or(Abort::NotApplicable)?;
    let Loc::Block(path) = tr.loc(last)? else {
        return Err(Abort::NotApplicable);
    };
    if let Some(stop) = order::last_in_subtree(tr.doc(), &path)
        && stop.text
    {
        let len = order::inline_at(tr.doc(), &stop.loc).map_or(0, |i| i.text.len());
        return Ok(Selection::caret(TextPos {
            block: stop.id,
            offset: len,
        }));
    }
    match follower {
        Some(id) if tr.loc(id).is_ok() => Ok(Selection::caret(TextPos {
            block: id,
            offset: 0,
        })),
        _ => match order::next_after_subtree(tr.doc(), &path) {
            Some(stop) if stop.text => Ok(Selection::caret(TextPos {
                block: stop.id,
                offset: 0,
            })),
            _ => Ok(Selection::Node(last)),
        },
    }
}

/// Gives a parsed block and everything in it new ids from the document,
/// and marks it as new.
fn fresh(tr: &mut Tr<'_>, mut block: Block) -> Block {
    block.id = tr.alloc_id();
    block.origin = None;
    block.dirty = true;
    if let BlockKind::Table(data) = &mut block.kind {
        for row in &mut data.rows {
            for cell in row {
                cell.id = tr.alloc_id();
            }
        }
    }
    block.children = std::mem::take(&mut block.children)
        .into_iter()
        .map(|child| fresh(tr, child))
        .collect();
    block
}

/// A copy of a block outside its document.
fn detach(mut block: Block) -> Block {
    block.origin = None;
    block.dirty = true;
    block
}

/// The blocks touched by a selection: selected blocks with their text cut to
/// the selection, unselected parents replaced by their selected children.
fn prune(blocks: &[Block], selected: &HashSet<BlockId>, start: TextPos, end: TextPos) -> Vec<Block> {
    let mut out = Vec::new();
    for block in blocks {
        let children = prune(&block.children, selected, start, end);
        if !selected.contains(&block.id) {
            out.extend(children);
            continue;
        }
        let len = block.content.text.len();
        let from = if block.id == start.block { start.offset } else { 0 };
        let to = if block.id == end.block { end.offset } else { len };
        let content = if from <= to && to <= len && order::has_text(&block.kind) {
            inline_ops::slice(&block.content, from..to)
        } else {
            block.content.clone()
        };
        out.push(Block {
            id: block.id,
            kind: block.kind.clone(),
            content,
            children,
            origin: None,
            dirty: true,
        });
    }
    out
}

fn flatten(blocks: &[Block], out: &mut String) {
    for block in blocks {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&block.content.text.replace(['\n', '\r'], " "));
        if let BlockKind::Table(data) = &block.kind {
            for cell in data.rows.iter().flatten() {
                out.push(' ');
                out.push_str(&cell.content.text);
            }
        }
        flatten(&block.children, out);
    }
}
