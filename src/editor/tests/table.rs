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
    cx.simulate_click(point(r.right() - px(3.), r.center().y), Modifiers::none());
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
