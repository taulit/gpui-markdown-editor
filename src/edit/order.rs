//! Document order: where a node lives in the tree, and which node comes
//! before or after it.
//!
//! Every block is a *stop* in document order: its own content comes first,
//! then its children. Blocks with text (paragraphs, headings, items, quotes,
//! code, raw) are text stops. Rules, images, custom blocks and tables are
//! node stops; a table is followed by its cells, which are text stops.
//!
//! Positions are resolved through a [`PathIndex`], a map from id to path that
//! is rebuilt lazily after structural changes. Until it is rebuilt, lookups
//! fall back to a depth-first search, so a burst of structural edits does not
//! pay for a rebuild per step.

use std::cell::{Cell as StdCell, RefCell};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

use smallvec::SmallVec;

use crate::model::{Block, BlockId, BlockKind, Document, Inline};

/// Child indices from the top level down to a block.
pub(crate) type Path = SmallVec<[usize; 4]>;

/// Where a text or node stop lives in the tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Loc {
    /// A block at this path.
    Block(Path),
    /// A cell of the table at `table`.
    Cell {
        /// Path of the table block.
        table: Path,
        /// Row index, 0 is the header.
        row: usize,
        /// Column index.
        col: usize,
    },
}

impl Loc {
    /// The path of the block, or of the table for a cell.
    pub(crate) fn block_path(&self) -> &[usize] {
        match self {
            Loc::Block(path) | Loc::Cell { table: path, .. } => path,
        }
    }

    /// Index of the top-level block that contains this stop.
    pub(crate) fn top(&self) -> usize {
        self.block_path().first().copied().unwrap_or(0)
    }

    pub(crate) fn is_cell(&self) -> bool {
        matches!(self, Loc::Cell { .. })
    }

    fn key(&self) -> impl Iterator<Item = usize> + '_ {
        let (path, cell) = match self {
            Loc::Block(path) => (path, None),
            Loc::Cell { table, row, col } => (table, Some([*row, *col])),
        };
        path.iter().copied().chain(cell.into_iter().flatten())
    }
}

/// Compares two stops in document order.
pub(crate) fn cmp_loc(a: &Loc, b: &Loc) -> Ordering {
    a.key().cmp(b.key())
}

/// `true` if `ancestor` is a strict ancestor of the block or cell at `loc`.
pub(crate) fn is_ancestor(ancestor: &[usize], loc: &Loc) -> bool {
    let path = loc.block_path();
    match loc {
        Loc::Block(_) => path.len() > ancestor.len() && path.starts_with(ancestor),
        Loc::Cell { .. } => path.starts_with(ancestor),
    }
}

/// Block kinds whose content is editable text.
pub(crate) fn has_text(kind: &BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::Paragraph
            | BlockKind::Heading(_)
            | BlockKind::BulletItem
            | BlockKind::OrderedItem { .. }
            | BlockKind::TaskItem { .. }
            | BlockKind::Quote
            | BlockKind::CodeBlock { .. }
            | BlockKind::Raw
    )
}

/// Block kinds that may have children.
pub(crate) fn holds_children(kind: &BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::BulletItem
            | BlockKind::OrderedItem { .. }
            | BlockKind::TaskItem { .. }
            | BlockKind::Quote
    )
}

/// List and task items.
pub(crate) fn is_item(kind: &BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::BulletItem | BlockKind::OrderedItem { .. } | BlockKind::TaskItem { .. }
    )
}

/// Text that is code or source: no marks, no input rules.
pub(crate) fn is_literal(kind: &BlockKind) -> bool {
    matches!(kind, BlockKind::CodeBlock { .. } | BlockKind::Raw)
}

pub(crate) fn block_at<'a>(blocks: &'a [Block], path: &[usize]) -> Option<&'a Block> {
    let (first, rest) = path.split_first()?;
    let mut block = blocks.get(*first)?;
    for index in rest {
        block = block.children.get(*index)?;
    }
    Some(block)
}

pub(crate) fn block_at_mut<'a>(blocks: &'a mut [Block], path: &[usize]) -> Option<&'a mut Block> {
    let (first, rest) = path.split_first()?;
    let mut block = blocks.get_mut(*first)?;
    for index in rest {
        block = block.children.get_mut(*index)?;
    }
    Some(block)
}

/// The children list that holds the block at `path`.
pub(crate) fn siblings_mut<'a>(
    blocks: &'a mut Vec<Block>,
    parent: &[usize],
) -> Option<&'a mut Vec<Block>> {
    if parent.is_empty() {
        Some(blocks)
    } else {
        Some(&mut block_at_mut(blocks, parent)?.children)
    }
}

pub(crate) fn siblings<'a>(blocks: &'a [Block], parent: &[usize]) -> Option<&'a [Block]> {
    if parent.is_empty() {
        Some(blocks)
    } else {
        Some(&block_at(blocks, parent)?.children)
    }
}

/// The text of a text stop. `None` for node stops and missing nodes.
pub(crate) fn inline_at<'a>(doc: &'a Document, loc: &Loc) -> Option<&'a Inline> {
    match loc {
        Loc::Block(path) => {
            let block = block_at(&doc.blocks, path)?;
            has_text(&block.kind).then_some(&block.content)
        }
        Loc::Cell { table, row, col } => match &block_at(&doc.blocks, table)?.kind {
            BlockKind::Table(data) => Some(&data.rows.get(*row)?.get(*col)?.content),
            _ => None,
        },
    }
}

pub(crate) fn inline_at_mut<'a>(doc: &'a mut Document, loc: &Loc) -> Option<&'a mut Inline> {
    match loc {
        Loc::Block(path) => {
            let block = block_at_mut(&mut doc.blocks, path)?;
            has_text(&block.kind).then_some(&mut block.content)
        }
        Loc::Cell { table, row, col } => match &mut block_at_mut(&mut doc.blocks, table)?.kind {
            BlockKind::Table(data) => Some(&mut data.rows.get_mut(*row)?.get_mut(*col)?.content),
            _ => None,
        },
    }
}

/// Finds a block or cell by depth-first search.
pub(crate) fn find_loc(doc: &Document, id: BlockId) -> Option<Loc> {
    fn walk(blocks: &[Block], id: BlockId, path: &mut Path) -> Option<Loc> {
        for (index, block) in blocks.iter().enumerate() {
            path.push(index);
            if block.id == id {
                return Some(Loc::Block(path.clone()));
            }
            if let BlockKind::Table(data) = &block.kind {
                for (row, cells) in data.rows.iter().enumerate() {
                    if let Some(col) = cells.iter().position(|cell| cell.id == id) {
                        return Some(Loc::Cell {
                            table: path.clone(),
                            row,
                            col,
                        });
                    }
                }
            }
            if let Some(found) = walk(&block.children, id, path) {
                return Some(found);
            }
            path.pop();
        }
        None
    }
    walk(&doc.blocks, id, &mut Path::new())
}

/// Lookups that miss the map before it is rebuilt.
const REBUILD_AFTER: u32 = 8;

/// Block ids are small sequential integers: a multiplicative hash spreads
/// them well and is much cheaper than SipHash when the map is rebuilt.
#[derive(Default)]
struct IdHasher(u64);

impl Hasher for IdHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 =
                (self.0.rotate_left(5) ^ u64::from(*byte)).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
        }
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0 ^ value).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }
}

type IdMap = HashMap<BlockId, Loc, BuildHasherDefault<IdHasher>>;

/// A lazily rebuilt map from id to [`Loc`].
#[derive(Debug, Default)]
pub(crate) struct PathIndex {
    map: RefCell<Option<IdMap>>,
    misses: StdCell<u32>,
}

impl Clone for PathIndex {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl PathIndex {
    /// Forgets all paths. Call after inserting, removing or moving blocks,
    /// rows or columns.
    pub(crate) fn invalidate(&self) {
        self.map.borrow_mut().take();
        self.misses.set(0);
    }

    /// Builds the map now instead of on demand.
    pub(crate) fn build(&self, doc: &Document) {
        *self.map.borrow_mut() = Some(build_map(doc));
    }

    pub(crate) fn locate(&self, doc: &Document, id: BlockId) -> Option<Loc> {
        if let Some(map) = self.map.borrow().as_ref() {
            return map.get(&id).cloned();
        }
        let misses = self.misses.get() + 1;
        self.misses.set(misses);
        if misses < REBUILD_AFTER {
            return find_loc(doc, id);
        }
        let map = build_map(doc);
        let found = map.get(&id).cloned();
        *self.map.borrow_mut() = Some(map);
        found
    }
}

impl PathIndex {
    /// Updates the map after `block` was inserted at `index` among the
    /// children of the block at `parent` (empty: the top level).
    pub(crate) fn inserted(&self, parent: &[usize], index: usize, block: &Block) {
        let mut map = self.map.borrow_mut();
        let Some(map) = map.as_mut() else {
            return;
        };
        shift(map, parent, index, true);
        let mut path: Path = parent.into();
        path.push(index);
        add_subtree(map, block, &mut path);
    }

    /// Updates the map after `block` was removed from `index` among the
    /// children of the block at `parent`.
    pub(crate) fn removed(&self, parent: &[usize], index: usize, block: &Block) {
        let mut map = self.map.borrow_mut();
        let Some(map) = map.as_mut() else {
            return;
        };
        remove_subtree(map, block);
        shift(map, parent, index, false);
    }

    /// Updates the cells of the table at `path` after rows or columns
    /// changed. `old` are the cell ids before the change.
    pub(crate) fn table_changed(&self, path: &[usize], old: &[BlockId], block: &Block) {
        let mut map = self.map.borrow_mut();
        let Some(map) = map.as_mut() else {
            return;
        };
        for id in old {
            map.remove(id);
        }
        add_cells(map, block, path);
    }
}

/// Moves the paths of later siblings (and their descendants) of a changed
/// child list by one.
fn shift(map: &mut IdMap, parent: &[usize], index: usize, insert: bool) {
    let depth = parent.len();
    for loc in map.values_mut() {
        let path = match loc {
            Loc::Block(path) | Loc::Cell { table: path, .. } => path,
        };
        if path.len() > depth && path[depth] >= index && path.starts_with(parent) {
            if insert {
                path[depth] += 1;
            } else if path[depth] > index {
                path[depth] -= 1;
            }
        }
    }
}

fn add_cells(map: &mut IdMap, block: &Block, path: &[usize]) {
    if let BlockKind::Table(data) = &block.kind {
        for (row, cells) in data.rows.iter().enumerate() {
            for (col, cell) in cells.iter().enumerate() {
                let loc = Loc::Cell {
                    table: path.into(),
                    row,
                    col,
                };
                map.insert(cell.id, loc);
            }
        }
    }
}

fn add_subtree(map: &mut IdMap, block: &Block, path: &mut Path) {
    map.insert(block.id, Loc::Block(path.clone()));
    add_cells(map, block, path);
    for (index, child) in block.children.iter().enumerate() {
        path.push(index);
        add_subtree(map, child, path);
        path.pop();
    }
}

fn remove_subtree(map: &mut IdMap, block: &Block) {
    map.remove(&block.id);
    if let BlockKind::Table(data) = &block.kind {
        for cell in data.rows.iter().flatten() {
            map.remove(&cell.id);
        }
    }
    for child in &block.children {
        remove_subtree(map, child);
    }
}

fn build_map(doc: &Document) -> IdMap {
    fn walk(blocks: &[Block], path: &mut Path, map: &mut IdMap) {
        for (index, block) in blocks.iter().enumerate() {
            path.push(index);
            map.insert(block.id, Loc::Block(path.clone()));
            if let BlockKind::Table(data) = &block.kind {
                for (row, cells) in data.rows.iter().enumerate() {
                    for (col, cell) in cells.iter().enumerate() {
                        let loc = Loc::Cell {
                            table: path.clone(),
                            row,
                            col,
                        };
                        map.insert(cell.id, loc);
                    }
                }
            }
            walk(&block.children, path, map);
            path.pop();
        }
    }
    let mut map = IdMap::with_capacity_and_hasher(doc.blocks.len() * 2, Default::default());
    walk(&doc.blocks, &mut Path::new(), &mut map);
    map
}

/// A text or node stop in document order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Stop {
    pub(crate) id: BlockId,
    pub(crate) loc: Loc,
    /// `true` for text stops, `false` for node stops.
    pub(crate) text: bool,
}

pub(crate) fn stop_at(doc: &Document, loc: &Loc) -> Option<Stop> {
    match loc {
        Loc::Block(path) => {
            let block = block_at(&doc.blocks, path)?;
            Some(Stop {
                id: block.id,
                loc: loc.clone(),
                text: has_text(&block.kind),
            })
        }
        Loc::Cell { table, row, col } => match &block_at(&doc.blocks, table)?.kind {
            BlockKind::Table(data) => Some(Stop {
                id: data.rows.get(*row)?.get(*col)?.id,
                loc: loc.clone(),
                text: true,
            }),
            _ => None,
        },
    }
}

fn first_cell(block: &Block, path: &[usize]) -> Option<Loc> {
    match &block.kind {
        BlockKind::Table(data) if data.rows.first().is_some_and(|row| !row.is_empty()) => {
            Some(Loc::Cell {
                table: path.into(),
                row: 0,
                col: 0,
            })
        }
        _ => None,
    }
}

/// The stop after the whole subtree of the block at `path`.
pub(crate) fn next_after_subtree(doc: &Document, path: &[usize]) -> Option<Stop> {
    let mut path: Path = path.into();
    loop {
        let index = path.pop()?;
        let siblings = siblings(&doc.blocks, &path)?;
        if index + 1 < siblings.len() {
            path.push(index + 1);
            return stop_at(doc, &Loc::Block(path));
        }
    }
}

pub(crate) fn next_stop(doc: &Document, loc: &Loc) -> Option<Stop> {
    match loc {
        Loc::Block(path) => {
            let block = block_at(&doc.blocks, path)?;
            if let Some(cell) = first_cell(block, path) {
                return stop_at(doc, &cell);
            }
            if !block.children.is_empty() {
                let mut child = path.clone();
                child.push(0);
                return stop_at(doc, &Loc::Block(child));
            }
            next_after_subtree(doc, path)
        }
        Loc::Cell { table, row, col } => {
            if let BlockKind::Table(data) = &block_at(&doc.blocks, table)?.kind {
                let width = data.rows.get(*row).map_or(0, Vec::len);
                let next = if col + 1 < width {
                    Some((*row, col + 1))
                } else if data.rows.get(row + 1).is_some_and(|r| !r.is_empty()) {
                    Some((row + 1, 0))
                } else {
                    None
                };
                if let Some((row, col)) = next {
                    return stop_at(
                        doc,
                        &Loc::Cell {
                            table: table.clone(),
                            row,
                            col,
                        },
                    );
                }
            }
            next_after_subtree(doc, table)
        }
    }
}

/// The last stop in the subtree of the block at `path`.
pub(crate) fn last_in_subtree(doc: &Document, path: &[usize]) -> Option<Stop> {
    let mut path: Path = path.into();
    loop {
        let block = block_at(&doc.blocks, &path)?;
        if let BlockKind::Table(data) = &block.kind
            && let Some(row) = data.rows.len().checked_sub(1)
            && let Some(col) = data.rows[row].len().checked_sub(1)
        {
            return stop_at(
                doc,
                &Loc::Cell {
                    table: path,
                    row,
                    col,
                },
            );
        }
        match block.children.len().checked_sub(1) {
            Some(last) => path.push(last),
            None => return stop_at(doc, &Loc::Block(path)),
        }
    }
}

pub(crate) fn prev_stop(doc: &Document, loc: &Loc) -> Option<Stop> {
    match loc {
        Loc::Block(path) => {
            let mut path = path.clone();
            let index = path.pop()?;
            if index > 0 {
                path.push(index - 1);
                last_in_subtree(doc, &path)
            } else if path.is_empty() {
                None
            } else {
                stop_at(doc, &Loc::Block(path))
            }
        }
        Loc::Cell { table, row, col } => {
            let previous = if *col > 0 {
                Some((*row, col - 1))
            } else if *row > 0 {
                match &block_at(&doc.blocks, table)?.kind {
                    BlockKind::Table(data) => data
                        .rows
                        .get(row - 1)
                        .and_then(|r| r.len().checked_sub(1))
                        .map(|col| (row - 1, col)),
                    _ => None,
                }
            } else {
                None
            };
            match previous {
                Some((row, col)) => stop_at(
                    doc,
                    &Loc::Cell {
                        table: table.clone(),
                        row,
                        col,
                    },
                ),
                None => stop_at(doc, &Loc::Block(table.clone())),
            }
        }
    }
}

pub(crate) fn first_stop(doc: &Document) -> Option<Stop> {
    (!doc.blocks.is_empty()).then(|| stop_at(doc, &Loc::Block(smallvec::smallvec![0])))?
}

pub(crate) fn last_stop(doc: &Document) -> Option<Stop> {
    let last = doc.blocks.len().checked_sub(1)?;
    last_in_subtree(doc, &[last])
}

pub(crate) fn next_text(doc: &Document, loc: &Loc) -> Option<Stop> {
    let mut stop = next_stop(doc, loc)?;
    while !stop.text {
        stop = next_stop(doc, &stop.loc)?;
    }
    Some(stop)
}

pub(crate) fn prev_text(doc: &Document, loc: &Loc) -> Option<Stop> {
    let mut stop = prev_stop(doc, loc)?;
    while !stop.text {
        stop = prev_stop(doc, &stop.loc)?;
    }
    Some(stop)
}

pub(crate) fn first_text(doc: &Document) -> Option<Stop> {
    let stop = first_stop(doc)?;
    if stop.text {
        Some(stop)
    } else {
        next_text(doc, &stop.loc)
    }
}

pub(crate) fn last_text(doc: &Document) -> Option<Stop> {
    let stop = last_stop(doc)?;
    if stop.text {
        Some(stop)
    } else {
        prev_text(doc, &stop.loc)
    }
}

/// All stops from `from` to `to`, both included, in document order.
pub(crate) fn stops_between(doc: &Document, from: &Loc, to: &Loc) -> Vec<Stop> {
    let mut out = Vec::new();
    let Some(mut stop) = stop_at(doc, from) else {
        return out;
    };
    loop {
        let done = stop.loc == *to || cmp_loc(&stop.loc, to) == Ordering::Greater;
        if cmp_loc(&stop.loc, to) != Ordering::Greater {
            out.push(stop.clone());
        }
        if done {
            break;
        }
        match next_stop(doc, &stop.loc) {
            Some(next) => stop = next,
            None => break,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(doc: &Document) -> Vec<(String, bool)> {
        let mut out = Vec::new();
        let mut stop = first_stop(doc);
        while let Some(current) = stop {
            let text = inline_at(doc, &current.loc).map_or("#".to_string(), |i| i.text.clone());
            out.push((text, current.text));
            stop = next_stop(doc, &current.loc);
        }
        out
    }

    #[test]
    fn walks_in_document_order_both_ways() {
        let doc = Document::parse(
            "# T\n\n- a\n  - b\n- c\n\n---\n\n| x | y |\n|---|---|\n| 1 | 2 |\n\n> q\n>\n> r\n",
        );
        let forward = ids(&doc);
        let texts: Vec<&str> = forward.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(
            texts,
            ["T", "a", "b", "c", "#", "#", "x", "y", "1", "2", "q", "r"]
        );
        let mut backward = Vec::new();
        let mut stop = last_stop(&doc);
        while let Some(current) = stop {
            backward.push(current.id);
            stop = prev_stop(&doc, &current.loc);
        }
        backward.reverse();
        let mut forward_ids = Vec::new();
        let mut stop = first_stop(&doc);
        while let Some(current) = stop {
            forward_ids.push(current.id);
            stop = next_stop(&doc, &current.loc);
        }
        assert_eq!(backward, forward_ids);
    }

    #[test]
    fn index_and_search_agree() {
        let doc = Document::parse("- a\n  - b\n\n| x |\n|---|\n| 1 |\n");
        let index = PathIndex::default();
        let mut stop = first_stop(&doc);
        while let Some(current) = stop {
            for _ in 0..4 {
                assert_eq!(index.locate(&doc, current.id), Some(current.loc.clone()));
            }
            stop = next_stop(&doc, &current.loc);
        }
    }
}
