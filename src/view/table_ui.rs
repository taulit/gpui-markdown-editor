//! The structure affordances of a table: the "+" bars below and right of
//! it, the handles of the focused row and column and the column borders
//! that can be dragged. Shared by painting and hit testing so that both
//! always agree.

use std::collections::HashMap;

use gpui::{Bounds, Pixels, Point, point, px, size};

use crate::model::{BlockId, BlockKind, Document};

use super::layout::{BlockLayout, RectKind};

/// Something on a table the mouse can act on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Affordance {
    /// The bar below the table: appends a row.
    AddRow { table: BlockId },
    /// The bar right of the table: appends a column.
    AddColumn { table: BlockId },
    /// The handle left of a row: opens the row menu.
    RowHandle { table: BlockId, row: usize },
    /// The handle above a column: opens the column menu.
    ColumnHandle { table: BlockId, col: usize },
    /// The right border of a column: drag to set its width.
    ColumnBorder { table: BlockId, col: usize },
}

/// Thickness of the "+" bars.
const BAR: Pixels = px(12.);
/// Gap between the table and the bars.
const BAR_GAP: Pixels = px(4.);
/// How far around the table the mouse still counts as over it.
pub(crate) const HOVER_MARGIN: Pixels = px(28.);

/// The rows and columns of one laid out table, relative to its block.
#[derive(Clone, Debug)]
pub(crate) struct TableGeom {
    pub table: BlockId,
    pub bounds: Bounds<Pixels>,
    /// Top and bottom of every row.
    pub rows: Vec<(Pixels, Pixels)>,
    /// Left and right of every column.
    pub cols: Vec<(Pixels, Pixels)>,
    /// Row and column of every cell.
    pub cells: HashMap<BlockId, (usize, usize)>,
}

impl TableGeom {
    fn row_at(&self, y: Pixels) -> Option<usize> {
        self.rows.iter().position(|(t, b)| *t <= y && y < *b)
    }

    fn col_at(&self, x: Pixels) -> Option<usize> {
        self.cols.iter().position(|(l, r)| *l <= x && x < *r)
    }

    /// The cell under `p`.
    pub fn cell_at(&self, p: Point<Pixels>) -> Option<(usize, usize)> {
        Some((self.row_at(p.y)?, self.col_at(p.x)?))
    }

    /// The area in which the affordances show.
    pub fn hover_area(&self) -> Bounds<Pixels> {
        self.bounds.dilate(HOVER_MARGIN)
    }

    /// The affordances with their areas. Handles are shown for the focused
    /// row and column only.
    pub fn affordances(
        &self,
        row: Option<usize>,
        col: Option<usize>,
    ) -> Vec<(Affordance, Bounds<Pixels>)> {
        let b = self.bounds;
        let table = self.table;
        let mut out = vec![
            (
                Affordance::AddRow { table },
                Bounds::new(
                    point(b.left(), b.bottom() + BAR_GAP),
                    size(b.size.width, BAR),
                ),
            ),
            (
                Affordance::AddColumn { table },
                Bounds::new(
                    point(b.right() + BAR_GAP, b.top()),
                    size(BAR, b.size.height),
                ),
            ),
        ];
        if let Some((row, (top, bottom))) = row.and_then(|r| Some((r, *self.rows.get(r)?))) {
            let h = px(18.).min(bottom - top);
            out.push((
                Affordance::RowHandle { table, row },
                Bounds::new(
                    point(b.left() - px(16.), top + (bottom - top - h) / 2.),
                    size(px(10.), h),
                ),
            ));
        }
        if let Some((col, (left, right))) = col.and_then(|c| Some((c, *self.cols.get(c)?))) {
            let w = px(18.).min(right - left);
            out.push((
                Affordance::ColumnHandle { table, col },
                Bounds::new(
                    point(left + (right - left - w) / 2., b.top() - px(14.)),
                    size(w, px(10.)),
                ),
            ));
        }
        out
    }

    /// The draggable border right of a column under `p`.
    pub fn border_at(&self, p: Point<Pixels>) -> Option<Affordance> {
        if p.y < self.bounds.top() || p.y > self.bounds.bottom() {
            return None;
        }
        self.cols
            .iter()
            .position(|(_, right)| (p.x - *right).abs() <= px(4.))
            .map(|col| Affordance::ColumnBorder {
                table: self.table,
                col,
            })
    }
}

/// The geometry of every table in `layout`, relative to the block.
pub(crate) fn tables(layout: &BlockLayout, doc: &Document) -> Vec<TableGeom> {
    let rects: HashMap<BlockId, Bounds<Pixels>> = layout
        .rects
        .iter()
        .filter(|(_, kind, _)| *kind == RectKind::Cell)
        .map(|(id, _, r)| (*id, *r))
        .collect();
    let mut out = Vec::new();
    for (id, kind, bounds) in &layout.rects {
        if *kind != RectKind::Table {
            continue;
        }
        let Some(BlockKind::Table(data)) = doc.block(*id).map(|b| b.kind()) else {
            continue;
        };
        let ncols = data.rows.iter().map(Vec::len).max().unwrap_or(0);
        let mut rows: Vec<(Pixels, Pixels)> = Vec::with_capacity(data.rows.len());
        let mut cols: Vec<Option<(Pixels, Pixels)>> = vec![None; ncols];
        let mut cells = HashMap::new();
        for (r, row) in data.rows.iter().enumerate() {
            let mut span = None;
            for (c, cell) in row.iter().enumerate() {
                let Some(rect) = rects.get(&cell.id) else {
                    continue;
                };
                cells.insert(cell.id, (r, c));
                span = Some((rect.top(), rect.bottom()));
                cols[c].get_or_insert((rect.left(), rect.right()));
            }
            // A row without cells keeps its place with zero height.
            let last = rows.last().map_or(bounds.top(), |r| r.1);
            rows.push(span.unwrap_or((last, last)));
        }
        let mut x = bounds.left();
        let cols = cols
            .into_iter()
            .map(|c| {
                let c = c.unwrap_or((x, x));
                x = c.1;
                c
            })
            .collect();
        out.push(TableGeom {
            table: *id,
            bounds: *bounds,
            rows,
            cols,
            cells,
        });
    }
    out
}

/// The affordances shown for `g`, or `None` if the table is not active:
/// the mouse (`hover`, relative to the block) is not near it and the caret
/// is not in it. The handles follow the row and column under the mouse,
/// else those of the caret's cell.
pub(crate) fn shown(
    g: &TableGeom,
    hover: Option<Point<Pixels>>,
    caret: Option<BlockId>,
) -> Option<Vec<(Affordance, Bounds<Pixels>)>> {
    let hovered = hover.filter(|p| g.hover_area().contains(p));
    let caret_cell = caret.and_then(|id| g.cells.get(&id).copied());
    if hovered.is_none() && caret_cell.is_none() {
        return None;
    }
    let row = hovered
        .and_then(|p| g.row_at(p.y))
        .or(caret_cell.map(|c| c.0));
    let col = hovered
        .and_then(|p| g.col_at(p.x))
        .or(caret_cell.map(|c| c.1));
    Some(g.affordances(row, col))
}
