//! Tables: typing in cells, navigation, cell rectangles, structure handles
//! and column widths.

use gpui::{Modifiers, MouseButton, TestAppContext};

use super::*;
use crate::model::BlockKind;

fn pos(block: BlockId, offset: usize) -> TextPos {
    TextPos { block, offset }
}

const TABLE: &str = "| a | bb |\n|---|---|\n| ccc | d |\n";

/// The id of cell (`row`, `col`) of the table at top-level block `ix`.
pub(super) fn cell(
    editor: &Entity<MarkdownEditor>,
    cx: &mut VisualTestContext,
    ix: usize,
    row: usize,
    col: usize,
) -> BlockId {
    editor.read_with(cx, |e, _| match e.document().blocks()[ix].kind() {
        BlockKind::Table(t) => t.rows[row][col].id(),
        _ => panic!("not a table"),
    })
}

/// The window bounds of a block or cell.
pub(super) fn bounds_of(
    editor: &Entity<MarkdownEditor>,
    cx: &mut VisualTestContext,
    id: BlockId,
) -> gpui::Bounds<Pixels> {
    editor.update_in(cx, |editor, window, _| {
        let ts = window.text_system().clone();
        let view = editor.view.clone();
        view.query(editor.layout_input(), &ts)
            .block_bounds(id)
            .expect("bounds")
    })
}

#[gpui::test]
fn clicks_land_in_the_cell_under_the_mouse(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, TABLE);
    let a = cell(&editor, cx, 0, 0, 0);
    let d = cell(&editor, cx, 0, 1, 1);

    // Right of the short text, near the border of the next cell.
    let r = bounds_of(&editor, cx, a);
    cx.simulate_click(point(r.right() - px(8.), r.center().y), Modifiers::none());
    assert_eq!(caret(&editor, cx), "a‸");

    // Below the text of a cell in a taller row still hits that cell.
    let r = bounds_of(&editor, cx, d);
    cx.simulate_click(
        point(r.left() + px(4.), r.bottom() - px(2.)),
        Modifiers::none(),
    );
    let head = editor.read_with(cx, |e, _| e.state.selection.head().unwrap());
    assert_eq!(head.block, d);

    cx.simulate_input("x");
    assert_eq!(
        md(&editor, cx),
        "| a   | bb  |\n| --- | --- |\n| ccc | xd  |\n"
    );
    press(cx, "secondary-z");
    assert_eq!(md(&editor, cx), TABLE);
}

#[gpui::test]
fn arrows_tab_and_enter_move_between_cells(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, &format!("{TABLE}\nafter\n"));
    let bb = cell(&editor, cx, 0, 0, 1);
    select(&editor, cx, pos(bb, 1), pos(bb, 1));
    press(cx, "down");
    assert_eq!(caret(&editor, cx), "d‸");
    press(cx, "up");
    assert_eq!(caret(&editor, cx), "b‸b");
    press(cx, "shift-tab");
    assert_eq!(caret(&editor, cx), "a‸");
    press(cx, "tab");
    assert_eq!(caret(&editor, cx), "bb‸");
    press(cx, "enter");
    assert_eq!(caret(&editor, cx), "d‸");
    press(cx, "down");
    assert_eq!(caret(&editor, cx), "after‸");
}

#[gpui::test]
fn marks_and_selection_in_a_cell(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, TABLE);
    let c = cell(&editor, cx, 0, 1, 0);
    let start = point_of(&editor, cx, pos(c, 0));
    let end = point_of(&editor, cx, pos(c, 2));
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    assert_eq!(caret(&editor, cx), "«cc‸c");
    press(cx, "secondary-b");
    assert_eq!(
        md(&editor, cx),
        "| a       | bb  |\n| ------- | --- |\n| **cc**c | d   |\n"
    );
}

const GRID: &str = "| h1 | h2 | h3 |\n|---|:-:|---|\n| a | b | c |\n| d | e | f |\n";

/// Drags from the middle of one cell to the middle of another.
fn drag_cells(
    editor: &Entity<MarkdownEditor>,
    cx: &mut VisualTestContext,
    from: BlockId,
    to: BlockId,
) {
    let a = bounds_of(editor, cx, from).center();
    let b = bounds_of(editor, cx, to).center();
    cx.simulate_mouse_down(a, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(b, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(b, MouseButton::Left, Modifiers::none());
}

#[gpui::test]
fn dragging_across_cells_selects_a_rectangle(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, GRID);
    let (h2, f) = (cell(&editor, cx, 0, 0, 1), cell(&editor, cx, 0, 2, 2));
    drag_cells(&editor, cx, h2, f);
    let selection = editor.read_with(cx, |e, _| e.state.selection.clone());
    assert_eq!(
        selection,
        Selection::Cells {
            anchor: h2,
            head: f
        }
    );

    press(cx, "secondary-c");
    let copied = cx.read_from_clipboard().and_then(|i| i.text()).unwrap();
    assert_eq!(
        copied,
        "| h2  | h3  |\n| :-: | --- |\n|  b  | c   |\n|  e  | f   |"
    );

    press(cx, "delete");
    assert_eq!(
        md(&editor, cx),
        "| h1  |     |     |\n| --- | :-: | --- |\n| a   |     |     |\n| d   |     |     |\n"
    );
    assert_eq!(caret(&editor, cx), "‸");
    press(cx, "secondary-z");
    assert_eq!(md(&editor, cx), GRID);
}

#[gpui::test]
fn typing_over_a_rectangle_replaces_the_anchor_cell(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, GRID);
    let (a, e) = (cell(&editor, cx, 0, 1, 0), cell(&editor, cx, 0, 2, 1));
    drag_cells(&editor, cx, a, e);
    cx.simulate_input("x");
    assert_eq!(caret(&editor, cx), "x‸");
    assert_eq!(
        md(&editor, cx),
        "| h1  | h2  | h3  |\n| --- | :-: | --- |\n| x   |  b  | c   |\n| d   |  e  | f   |\n"
    );
    // Arrows collapse a rectangle to its head cell.
    drag_cells(&editor, cx, a, e);
    press(cx, "right");
    assert_eq!(caret(&editor, cx), "e‸");
}

#[gpui::test]
fn cut_a_rectangle(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, GRID);
    let (b, c) = (cell(&editor, cx, 0, 1, 1), cell(&editor, cx, 0, 1, 2));
    drag_cells(&editor, cx, c, b);
    press(cx, "secondary-x");
    let copied = cx.read_from_clipboard().and_then(|i| i.text()).unwrap();
    assert_eq!(copied, "|  b  | c   |\n| :-: | --- |");
    assert_eq!(caret(&editor, cx), "‸");
}

/// The window bounds of the affordance `a`, found by scanning around the
/// table with the view's own hit testing.
fn affordance_point(
    editor: &Entity<MarkdownEditor>,
    cx: &mut VisualTestContext,
    want: impl Fn(&crate::view::Affordance) -> bool,
) -> Point<Pixels> {
    editor.update_in(cx, |editor, window, _| {
        let ts = window.text_system().clone();
        let view = editor.view.clone();
        let caret = editor.state.selection.head().map(|h| h.block);
        let mut q = view.query(editor.layout_input(), &ts);
        for y in (0..600).step_by(2) {
            for x in (0..800).step_by(2) {
                let p = point(px(x as f32), px(y as f32));
                if q.affordance_at(p, Some(p), caret).is_some_and(|a| want(&a)) {
                    return p;
                }
            }
        }
        panic!("affordance not found");
    })
}

fn menu_click(cx: &mut VisualTestContext, entry: &'static str) {
    cx.run_until_parked();
    let selector: &'static str = Box::leak(format!("table-menu-{entry}").into_boxed_str());
    let bounds = cx.debug_bounds(selector).expect("menu entry");
    cx.simulate_click(bounds.center(), Modifiers::none());
}

use crate::view::Affordance;

#[gpui::test]
fn plus_bars_add_a_row_and_a_column(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, TABLE);
    let a = cell(&editor, cx, 0, 0, 0);
    select(&editor, cx, pos(a, 0), pos(a, 0));
    let p = affordance_point(&editor, cx, |a| matches!(a, Affordance::AddRow { .. }));
    cx.simulate_click(p, Modifiers::none());
    assert_eq!(
        md(&editor, cx),
        "| a   | bb  |\n| --- | --- |\n| ccc | d   |\n|     |     |\n"
    );
    let p = affordance_point(&editor, cx, |a| matches!(a, Affordance::AddColumn { .. }));
    cx.simulate_click(p, Modifiers::none());
    assert_eq!(
        md(&editor, cx),
        "| a   | bb  |     |\n| --- | --- | --- |\n| ccc | d   |     |\n|     |     |     |\n"
    );
    cx.simulate_input("z");
    assert_eq!(caret(&editor, cx), "z‸");
    press(cx, "secondary-z");
    press(cx, "secondary-z");
    press(cx, "secondary-z");
    assert_eq!(md(&editor, cx), TABLE);
}

#[gpui::test]
fn row_menu_inserts_moves_and_deletes(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, GRID);
    let d = cell(&editor, cx, 0, 2, 0);
    select(&editor, cx, pos(d, 0), pos(d, 0));
    let handle = affordance_point(&editor, cx, |a| {
        matches!(a, Affordance::RowHandle { row: 2, .. })
    });

    cx.simulate_click(handle, Modifiers::none());
    menu_click(cx, "move-up");
    assert_eq!(
        md(&editor, cx),
        "| h1  | h2  | h3  |\n| --- | :-: | --- |\n| d   |  e  | f   |\n| a   |  b  | c   |\n"
    );
    // The handle follows the moved row.
    let handle = affordance_point(&editor, cx, |a| {
        matches!(a, Affordance::RowHandle { row: 1, .. })
    });
    cx.simulate_click(handle, Modifiers::none());
    menu_click(cx, "insert-below");
    assert_eq!(
        editor.read_with(cx, |e, _| e.state.current_cell().unwrap().row),
        2
    );
    let handle = affordance_point(&editor, cx, |a| {
        matches!(a, Affordance::RowHandle { row: 2, .. })
    });
    cx.simulate_click(handle, Modifiers::none());
    menu_click(cx, "delete");
    press(cx, "secondary-z");
    press(cx, "secondary-z");
    press(cx, "secondary-z");
    assert_eq!(md(&editor, cx), GRID);
}

#[gpui::test]
fn column_menu_aligns_moves_and_deletes(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, GRID);
    let h1 = cell(&editor, cx, 0, 0, 0);
    select(&editor, cx, pos(h1, 0), pos(h1, 0));
    let handle = |editor: &Entity<MarkdownEditor>, cx: &mut VisualTestContext, col: usize| {
        affordance_point(
            editor,
            cx,
            move |a| matches!(a, Affordance::ColumnHandle { col: c, .. } if *c == col),
        )
    };
    let p = handle(&editor, cx, 0);
    cx.simulate_click(p, Modifiers::none());
    menu_click(cx, "align-right");
    assert_eq!(
        md(&editor, cx),
        "|  h1 | h2  | h3  |\n| --: | :-: | --- |\n|   a |  b  | c   |\n|   d |  e  | f   |\n"
    );
    let p = handle(&editor, cx, 0);
    cx.simulate_click(p, Modifiers::none());
    menu_click(cx, "move-right");
    assert_eq!(
        md(&editor, cx),
        "| h2  |  h1 | h3  |\n| :-: | --: | --- |\n|  b  |   a | c   |\n|  e  |   d | f   |\n"
    );
    let p = handle(&editor, cx, 1);
    cx.simulate_click(p, Modifiers::none());
    menu_click(cx, "delete");
    assert_eq!(
        md(&editor, cx),
        "| h2  | h3  |\n| :-: | --- |\n|  b  | c   |\n|  e  | f   |\n"
    );
    let p = handle(&editor, cx, 1);
    cx.simulate_click(p, Modifiers::none());
    menu_click(cx, "insert-left");
    assert_eq!(caret(&editor, cx), "‸");
    // Escape closes an open menu.
    let p = handle(&editor, cx, 0);
    cx.simulate_click(p, Modifiers::none());
    assert!(editor.read_with(cx, |e, _| e.table_menu.is_some()));
    press(cx, "escape");
    assert!(editor.read_with(cx, |e, _| e.table_menu.is_none()));
    for _ in 0..4 {
        press(cx, "secondary-z");
    }
    assert_eq!(md(&editor, cx), GRID);
}

/// The rendered widths of the columns of the table at top-level block `ix`.
fn column_widths(
    editor: &Entity<MarkdownEditor>,
    cx: &mut VisualTestContext,
    ix: usize,
) -> Vec<f32> {
    let table = block(editor, cx, ix);
    editor.update_in(cx, |editor, window, _| {
        let ts = window.text_system().clone();
        let view = editor.view.clone();
        let g = view
            .query(editor.layout_input(), &ts)
            .table_geom(table)
            .expect("table");
        g.cols
            .iter()
            .map(|(l, r)| (*r - *l).as_f32().round())
            .collect()
    })
}

#[gpui::test]
fn dragging_a_column_border_stores_the_width_in_the_meta(cx: &mut TestAppContext) {
    let md_text = format!("intro\n\n{GRID}\n{TABLE}");
    let (editor, cx) = open(cx, &md_text);
    let events = record_events(&editor, cx);
    let before = column_widths(&editor, cx, 2);
    // The first border of the second table.
    let table = block(&editor, cx, 2);
    let g = editor.update_in(cx, |editor, window, _| {
        let ts = window.text_system().clone();
        let view = editor.view.clone();
        view.query(editor.layout_input(), &ts)
            .table_geom(table)
            .unwrap()
    });
    let start = point(g.cols[0].1, g.bounds.center().y);
    let end = point(start.x + px(60.), start.y);
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());

    let after = column_widths(&editor, cx, 2);
    assert_eq!(after[0], before[0] + 60.);
    assert_eq!(after[1], before[1]);
    assert_eq!(count(&events, &EditorEvent::MetaChanged), 1);
    assert_eq!(count(&events, &EditorEvent::Changed), 0);
    let meta = editor.read_with(cx, |e, _| e.meta().clone());
    assert_eq!(meta.column_widths.keys().copied().collect::<Vec<_>>(), [1]);
    assert_eq!(meta.column_widths[&1][0], before[0] + 60.);
    // The Markdown is untouched.
    assert_eq!(md(&editor, cx), md_text);

    // Removing the table above keeps the widths with their table.
    let first = block(&editor, cx, 1);
    editor.update(cx, |e, _| e.state.set_selection(Selection::Node(first)));
    press(cx, "delete");
    let meta = editor.read_with(cx, |e, _| e.meta().clone());
    assert_eq!(meta.column_widths.keys().copied().collect::<Vec<_>>(), [0]);
    assert_eq!(count(&events, &EditorEvent::MetaChanged), 2);

    // Loading the document again with the stored meta restores the width.
    let saved = editor.read_with(cx, |e, _| (e.markdown(), e.meta().clone()));
    editor.update(cx, |e, cx| {
        e.set_markdown("", cx);
        e.set_meta(Default::default(), cx);
        e.set_markdown(&saved.0, cx);
        e.set_meta(saved.1.clone(), cx);
    });
    cx.run_until_parked();
    let ix = editor.read_with(cx, |e, _| {
        e.document()
            .blocks()
            .iter()
            .position(|b| matches!(b.kind(), BlockKind::Table(_)))
            .unwrap()
    });
    assert_eq!(column_widths(&editor, cx, ix), after);
    assert_eq!(count(&events, &EditorEvent::MetaChanged), 2);
}

#[gpui::test]
fn columns_size_to_their_content(cx: &mut TestAppContext) {
    let long = "word ".repeat(80);
    let md_text = format!("| a | {long} |\n|---|---|\n| b | c |\n");
    let (editor, cx) = open(cx, &md_text);
    let widths = column_widths(&editor, cx, 0);
    // A short column stays narrow, a long one is capped and its text wraps
    // within the content column.
    assert_eq!(widths[0], 48.);
    assert!(widths.iter().sum::<f32>() <= 604.5, "{widths:?}");
}
