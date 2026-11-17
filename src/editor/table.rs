//! Table structure UI: the "+" bars, the row and column menus.

use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _,
    Pixels, Point, SharedString, StatefulInteractiveElement as _, Styled as _, Window, anchored,
    deferred, div, point, prelude::FluentBuilder as _, px,
};
use gpui_component::ActiveTheme as _;

use super::MarkdownEditor;
use crate::edit::Selection;
use crate::model::{Align, BlockId};
use crate::view::Affordance;

/// The open row or column menu.
#[derive(Clone, Copy, Debug)]
pub(super) struct TableMenu {
    /// The handle the menu belongs to.
    pub handle: Affordance,
    /// Window position of its top-left corner.
    pub position: Point<Pixels>,
    /// The entry chosen by Enter, moved with Up and Down.
    pub selected: usize,
}

/// A column border being dragged.
#[derive(Clone, Debug)]
pub(super) struct ColumnDrag {
    pub table: BlockId,
    pub col: usize,
    /// Window x where the drag started.
    pub start_x: Pixels,
    /// The widths of all columns when the drag started.
    pub start: Vec<f32>,
}

impl ColumnDrag {
    /// The column widths with the mouse at `x`. All columns get a width so
    /// that the others stay as they are.
    pub fn widths_at(&self, x: Pixels) -> Vec<f32> {
        let mut widths = self.start.clone();
        if let Some(w) = widths.get_mut(self.col) {
            *w = (*w + (x - self.start_x).as_f32())
                .max(crate::view::MIN_COLUMN.as_f32())
                .round();
        }
        widths
    }
}

/// Width of a column inserted into a table with stored widths.
const NEW_COLUMN_WIDTH: f32 = 120.;

/// The stored width of a column, kept with the ids of its cells: cells
/// stay with their column when rows or columns are added, moved or removed,
/// and undo brings removed ones back with their ids. So widths follow
/// every structure edit, from any place, and its undo and redo.
#[derive(Clone, Debug)]
pub(super) struct WidthRecord {
    cells: Vec<BlockId>,
    width: f32,
}

/// Width records by table id. Records of removed columns and tables are
/// kept (a few per table) for undo.
pub(super) type WidthRecords = std::collections::HashMap<BlockId, Vec<WidthRecord>>;

/// How many records of removed columns a table keeps.
const KEPT_RECORDS: usize = 32;

/// The cell ids of each column of `table`.
fn columns(doc: &crate::model::Document, table: BlockId) -> Option<Vec<Vec<BlockId>>> {
    let crate::model::BlockKind::Table(t) = doc.block(table)?.kind() else {
        return None;
    };
    let n = t.rows.iter().map(Vec::len).max().unwrap_or(0);
    Some(
        (0..n)
            .map(|c| {
                t.rows
                    .iter()
                    .filter_map(|r| r.get(c))
                    .map(|c| c.id())
                    .collect()
            })
            .collect(),
    )
}

/// Replaces the records of the present columns `cols` by `widths`,
/// keeping those of removed columns.
fn rerecord(records: &mut Vec<WidthRecord>, cols: Vec<Vec<BlockId>>, widths: &[f32]) {
    let present: std::collections::HashSet<BlockId> = cols.iter().flatten().copied().collect();
    records.retain(|r| !r.cells.iter().any(|c| present.contains(c)));
    let excess = records.len().saturating_sub(KEPT_RECORDS);
    records.drain(..excess);
    records.extend(
        cols.into_iter()
            .zip(widths)
            .map(|(cells, width)| WidthRecord {
                cells,
                width: *width,
            }),
    );
}

/// An entry of a row or column menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TableAction {
    InsertBefore,
    InsertAfter,
    MoveBefore,
    MoveAfter,
    Delete,
    Align(Align),
}

/// The entries of the row or column menu: id, label, action.
fn menu_entries(row: bool) -> Vec<(&'static str, &'static str, TableAction)> {
    let mut entries = if row {
        vec![
            (
                "insert-above",
                "Insert row above",
                TableAction::InsertBefore,
            ),
            ("insert-below", "Insert row below", TableAction::InsertAfter),
            ("move-up", "Move row up", TableAction::MoveBefore),
            ("move-down", "Move row down", TableAction::MoveAfter),
        ]
    } else {
        vec![
            (
                "insert-left",
                "Insert column left",
                TableAction::InsertBefore,
            ),
            (
                "insert-right",
                "Insert column right",
                TableAction::InsertAfter,
            ),
            ("move-left", "Move column left", TableAction::MoveBefore),
            ("move-right", "Move column right", TableAction::MoveAfter),
            ("align-left", "Align left", TableAction::Align(Align::Left)),
            (
                "align-center",
                "Align center",
                TableAction::Align(Align::Center),
            ),
            (
                "align-right",
                "Align right",
                TableAction::Align(Align::Right),
            ),
        ]
    };
    entries.push((
        "delete",
        if row { "Delete row" } else { "Delete column" },
        TableAction::Delete,
    ));
    entries
}

impl MarkdownEditor {
    /// Puts the caret into cell (`row`, `col`) of `table`, keeping the
    /// caret where it is if it is in that cell already.
    fn caret_into_cell(&mut self, table: BlockId, row: usize, col: usize) -> bool {
        let id = match self.state.doc.block(table).map(|b| b.kind()) {
            Some(crate::model::BlockKind::Table(t)) => t.rows.get(row).and_then(|r| r.get(col)),
            _ => None,
        }
        .map(|c| c.id());
        let Some(id) = id else {
            return false;
        };
        if self.state.selection.head().is_some_and(|h| h.block == id)
            && matches!(self.state.selection, Selection::Text { .. })
        {
            return true;
        }
        let end = self.state.end_of(id);
        self.state.set_selection(Selection::caret(end));
        true
    }

    /// The row and column of the caret's cell in `table`.
    fn caret_cell_in(&self, table: BlockId) -> Option<(usize, usize)> {
        self.state
            .current_cell()
            .filter(|at| at.table == table)
            .map(|at| (at.row, at.col))
    }

    /// A click on a table affordance. Returns `false` for column borders,
    /// which the caller turns into a resize drag.
    pub(super) fn click_affordance(
        &mut self,
        affordance: Affordance,
        bounds_bottom_left: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.table_menu = None;
        match affordance {
            Affordance::AddRow { table } => {
                let rows = self.table_size(table).0;
                let col = self.caret_cell_in(table).map_or(0, |c| c.1);
                if rows > 0 && self.caret_into_cell(table, rows - 1, col) {
                    self.state.table_insert_row(true);
                }
            }
            Affordance::AddColumn { table } => {
                let cols = self.table_size(table).1;
                let row = self.caret_cell_in(table).map_or(0, |c| c.0);
                if cols > 0 && self.caret_into_cell(table, row, cols - 1) {
                    self.state.table_insert_column(true);
                }
            }
            Affordance::RowHandle { table, row } => {
                let col = self.caret_cell_in(table).map_or(0, |c| c.1);
                self.caret_into_cell(table, row, col);
                self.table_menu = Some(TableMenu {
                    handle: affordance,
                    position: bounds_bottom_left,
                    selected: 0,
                });
            }
            Affordance::ColumnHandle { table, col } => {
                let row = self.caret_cell_in(table).map_or(0, |c| c.0);
                self.caret_into_cell(table, row, col);
                self.table_menu = Some(TableMenu {
                    handle: affordance,
                    position: bounds_bottom_left,
                    selected: 0,
                });
            }
            Affordance::ColumnBorder { .. } => return false,
        }
        self.after_change(false, window, cx);
        true
    }

    /// Rows and columns of `table`.
    fn table_size(&self, table: BlockId) -> (usize, usize) {
        match self.state.doc.block(table).map(|b| b.kind()) {
            Some(crate::model::BlockKind::Table(t)) => {
                (t.rows.len(), t.rows.iter().map(Vec::len).max().unwrap_or(0))
            }
            _ => (0, 0),
        }
    }

    /// Runs an entry of the open menu and closes it.
    pub(super) fn run_table_action(
        &mut self,
        action: TableAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(menu) = self.table_menu.take() else {
            return;
        };
        let row = matches!(menu.handle, Affordance::RowHandle { .. });
        let state = &mut self.state;
        match action {
            TableAction::InsertBefore if row => state.table_insert_row(false),
            TableAction::InsertAfter if row => state.table_insert_row(true),
            TableAction::MoveBefore if row => state.table_move_row(false),
            TableAction::MoveAfter if row => state.table_move_row(true),
            TableAction::Delete if row => state.table_remove_row(),
            TableAction::InsertBefore => state.table_insert_column(false),
            TableAction::InsertAfter => state.table_insert_column(true),
            TableAction::MoveBefore => state.table_move_column(false),
            TableAction::MoveAfter => state.table_move_column(true),
            TableAction::Delete => state.table_remove_column(),
            TableAction::Align(align) => state.table_set_align(align),
        };
        self.after_change(false, window, cx);
    }

    /// A table structure key at the caret's cell. `false` if the caret is
    /// not in a table.
    pub(super) fn table_key(
        &mut self,
        key: super::keys::TableKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        use super::keys::TableKey;
        let (Some(at), Some(head)) = (self.state.current_cell(), self.state.selection.head())
        else {
            return false;
        };
        let state = &mut self.state;
        match key {
            TableKey::InsertRow(below) => state.table_insert_row(below),
            TableKey::InsertColumn(right) => state.table_insert_column(right),
            TableKey::DeleteRow => state.table_remove_row(),
            TableKey::DeleteColumn => state.table_remove_column(),
            TableKey::MoveColumn(right) => state.table_move_column(right),
            TableKey::Menu => {
                let row_open = self.table_menu.is_some_and(|m| {
                    matches!(m.handle, Affordance::RowHandle { table, .. } if table == at.table)
                });
                let handle = if row_open {
                    Affordance::ColumnHandle {
                        table: at.table,
                        col: at.col,
                    }
                } else {
                    Affordance::RowHandle {
                        table: at.table,
                        row: at.row,
                    }
                };
                let ts = window.text_system().clone();
                let view = self.view.clone();
                let bounds = view
                    .query(self.layout_input(), &ts)
                    .block_bounds(head.block);
                let Some(bounds) = bounds else {
                    return true;
                };
                self.table_menu = Some(TableMenu {
                    handle,
                    position: bounds.bottom_left(),
                    selected: 0,
                });
                cx.notify();
                return true;
            }
        };
        self.table_menu = None;
        self.after_change(false, window, cx);
        true
    }

    /// Records the stored widths of `table` by its cells, after they were
    /// set (loaded or resized).
    pub(super) fn record_widths(&mut self, table: BlockId) {
        let Some(widths) = self.column_widths.get(&table) else {
            self.width_records.remove(&table);
            return;
        };
        let Some(cols) = columns(&self.state.doc, table) else {
            return;
        };
        rerecord(self.width_records.entry(table).or_default(), cols, widths);
    }

    /// Records the widths of every table, after they were loaded.
    pub(super) fn record_all_widths(&mut self) {
        self.width_records.clear();
        let tables: Vec<BlockId> = self.column_widths.keys().copied().collect();
        for table in tables {
            self.record_widths(table);
        }
    }

    /// Derives the widths of the tables with records from their columns
    /// after a structure edit: a column keeps its width wherever it went, a
    /// new column gets [`NEW_COLUMN_WIDTH`], a removed one drops its width,
    /// and undo brings it back.
    pub(super) fn derive_widths(&mut self) {
        let doc = &self.state.doc;
        for (table, records) in &mut self.width_records {
            let Some(cols) = columns(doc, *table) else {
                self.column_widths.remove(table);
                continue;
            };
            let mut widths: Vec<Option<f32>> = cols
                .iter()
                .map(|cells| {
                    records
                        .iter()
                        .find(|r| r.cells.iter().any(|c| cells.contains(c)))
                        .map(|r| r.width)
                })
                .collect();
            // Columns past the stored widths keep the automatic width.
            while widths.last().is_some_and(Option::is_none) {
                widths.pop();
            }
            let widths: Vec<f32> = widths
                .into_iter()
                .map(|w| w.unwrap_or(NEW_COLUMN_WIDTH))
                .collect();
            rerecord(records, cols, &widths);
            if widths.is_empty() {
                self.column_widths.remove(table);
            } else {
                self.column_widths.insert(*table, widths);
            }
        }
    }

    /// Up, Down and Enter in the open menu. `true` if the key was used.
    pub(super) fn table_menu_key(
        &mut self,
        delta: Option<isize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(menu) = &mut self.table_menu else {
            return false;
        };
        let entries = menu_entries(matches!(menu.handle, Affordance::RowHandle { .. }));
        match delta {
            Some(delta) => {
                let n = entries.len() as isize;
                menu.selected = (menu.selected as isize + delta).rem_euclid(n) as usize;
                cx.notify();
            }
            None => {
                if let Some((_, _, action)) = entries.get(menu.selected) {
                    self.run_table_action(*action, window, cx);
                }
            }
        }
        true
    }

    /// Closes the open menu, if any.
    pub(super) fn close_table_menu(&mut self, cx: &mut Context<Self>) {
        if self.table_menu.take().is_some() {
            cx.notify();
        }
    }

    /// The open menu as an overlay element.
    pub(super) fn render_table_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.table_menu?;
        let row = matches!(menu.handle, Affordance::RowHandle { .. });
        let entries = menu_entries(row);
        let count = entries.len();
        let theme = cx.theme();
        let (bg, fg, border, hover) = (
            theme.popover,
            theme.popover_foreground,
            theme.border,
            theme.list_hover,
        );
        let danger = theme.danger;
        let selected_bg = theme.primary.opacity(0.22);
        let items = entries
            .into_iter()
            .enumerate()
            .map(|(index, (id, label, action))| {
                let color = if action == TableAction::Delete {
                    danger
                } else {
                    fg
                };
                let selected = index == menu.selected;
                div()
                    .id(SharedString::from(id))
                    .debug_selector(|| format!("table-menu-{id}"))
                    .role(gpui::Role::MenuItem)
                    .aria_label(label)
                    .aria_selected(selected)
                    .aria_position_in_set(index + 1)
                    .aria_size_of_set(count)
                    .when(selected, |s| s.bg(selected_bg))
                    .px(px(12.))
                    .py(px(6.))
                    .rounded(px(8.))
                    .text_size(px(13.5))
                    .text_color(color)
                    .hover(move |s| s.bg(hover))
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.run_table_action(action, window, cx);
                        }),
                    )
                    .child(label)
            });
        let panel = div()
            .id("table-menu")
            .role(gpui::Role::Menu)
            .aria_label(if row { "Row" } else { "Column" })
            .occlude()
            .min_w(px(190.))
            .p(px(5.))
            .rounded(px(12.))
            .bg(bg)
            .border_1()
            .border_color(border)
            .shadow_lg()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close_table_menu(cx)))
            .children(items);
        Some(
            deferred(
                anchored()
                    .position(menu.position + point(px(0.), px(6.)))
                    .snap_to_window_with_margin(px(8.))
                    .child(panel),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }
}
