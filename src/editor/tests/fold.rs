//! Folding headings.

use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};

use super::*;
use crate::command::Command;
use crate::meta::DocMeta;

const DOC: &str = "# A\n\na\n\n## B\n\nb\n\n# C\n\nc\n";

fn hidden(editor: &Entity<MarkdownEditor>, cx: &mut VisualTestContext) -> Vec<String> {
    editor.read_with(cx, |e, _| {
        e.document()
            .blocks()
            .iter()
            .filter(|b| e.hidden.contains(&b.id()))
            .map(|b| b.text().to_string())
            .collect()
    })
}

fn folds(editor: &Entity<MarkdownEditor>, cx: &mut VisualTestContext) -> Vec<usize> {
    editor.read_with(cx, |e, _| {
        e.meta().folded_headings.iter().copied().collect()
    })
}

/// The window point of the fold chevron of top-level block `ix`.
fn chevron(
    editor: &Entity<MarkdownEditor>,
    cx: &mut VisualTestContext,
    ix: usize,
) -> gpui::Point<gpui::Pixels> {
    let b = block(editor, cx, ix);
    let p = point_of(
        editor,
        cx,
        TextPos {
            block: b,
            offset: 0,
        },
    );
    gpui::point(p.x - gpui::px(17.), p.y)
}

fn height_of(editor: &Entity<MarkdownEditor>, cx: &mut VisualTestContext, ix: usize) -> f32 {
    let b = block(editor, cx, ix);
    editor.update_in(cx, |editor, window, _| {
        let ts = window.text_system().clone();
        let view = editor.view.clone();
        let h = view
            .query(editor.layout_input(), &ts)
            .block_bounds(b)
            .unwrap()
            .size
            .height;
        h.as_f32()
    })
}

#[gpui::test]
fn chevron_folds_the_section_and_stores_it(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, DOC);
    let events = record_events(&editor, cx);
    let p = chevron(&editor, cx, 0);
    cx.simulate_mouse_move(p, None, Modifiers::none());
    cx.simulate_click(p, Modifiers::none());
    assert_eq!(hidden(&editor, cx), ["a", "B", "b"]);
    assert_eq!(folds(&editor, cx), [0]);
    assert_eq!(count(&events, &EditorEvent::MetaChanged), 1);
    assert_eq!(count(&events, &EditorEvent::Changed), 0);
    assert_eq!(md(&editor, cx), DOC, "the Markdown is untouched");
    assert_eq!(height_of(&editor, cx, 1), 0.);
    assert!(height_of(&editor, cx, 4) > 0.);

    // Clicking again unfolds.
    let p = chevron(&editor, cx, 0);
    cx.simulate_click(p, Modifiers::none());
    assert!(hidden(&editor, cx).is_empty());
    assert!(folds(&editor, cx).is_empty());
    assert_eq!(count(&events, &EditorEvent::MetaChanged), 2);
    assert!(height_of(&editor, cx, 1) > 0.);
}

#[gpui::test]
fn caret_skips_folded_content(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, DOC);
    // The caret inside a section being folded moves to its heading.
    let b = block(&editor, cx, 1);
    select(
        &editor,
        cx,
        TextPos {
            block: b,
            offset: 1,
        },
        TextPos {
            block: b,
            offset: 1,
        },
    );
    press(cx, "up");
    press(cx, "secondary-.");
    assert_eq!(hidden(&editor, cx), ["a", "B", "b"]);
    assert_eq!(caret(&editor, cx), "A‸");

    press(cx, "right");
    assert_eq!(caret(&editor, cx), "‸C");
    press(cx, "left");
    assert_eq!(caret(&editor, cx), "A‸");
    press(cx, "down");
    assert_eq!(caret(&editor, cx), "C‸");
    press(cx, "up");
    assert_eq!(caret(&editor, cx), "A‸");

    // The command toggles back.
    editor.update_in(cx, |e, window, cx| {
        e.command(Command::FoldHeading, window, cx)
    });
    assert!(hidden(&editor, cx).is_empty());
}

#[gpui::test]
fn set_meta_restores_folds_and_find_unfolds(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, DOC);
    let events = record_events(&editor, cx);
    let mut meta = DocMeta::default();
    meta.folded_headings.insert(1);
    editor.update(cx, |e, cx| e.set_meta(meta.clone(), cx));
    cx.run_until_parked();
    assert_eq!(hidden(&editor, cx), ["b"]);
    assert_eq!(count(&events, &EditorEvent::MetaChanged), 0);
    assert_eq!(editor.read_with(cx, |e, _| e.meta().clone()), meta);

    // Loading another document keeps the metadata, applied to it.
    editor.update(cx, |e, cx| e.set_markdown("# X\n\n## Y\n\nzz\n", cx));
    cx.run_until_parked();
    assert_eq!(hidden(&editor, cx), ["zz"]);

    // Finding text in a folded section unfolds it.
    press(cx, "secondary-f");
    cx.simulate_input("zz");
    assert_eq!(hidden(&editor, cx), Vec::<String>::new());
    assert!(folds(&editor, cx).is_empty());
    assert_eq!(count(&events, &EditorEvent::MetaChanged), 1);
}

#[gpui::test]
fn ordinals_follow_structural_edits(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, DOC);
    let c = block(&editor, cx, 4);
    select(
        &editor,
        cx,
        TextPos {
            block: c,
            offset: 0,
        },
        TextPos {
            block: c,
            offset: 0,
        },
    );
    press(cx, "secondary-.");
    assert_eq!(folds(&editor, cx), [2]);
    // A new heading before it shifts its ordinal.
    press(cx, "doc-start");
    press(cx, "end");
    type_text(cx, "\n## New");
    assert_eq!(folds(&editor, cx), [3]);
    // Turning the folded heading into a paragraph drops the fold.
    let c = block(&editor, cx, 5);
    select(
        &editor,
        cx,
        TextPos {
            block: c,
            offset: 0,
        },
        TextPos {
            block: c,
            offset: 0,
        },
    );
    press(cx, "secondary-alt-0");
    assert!(folds(&editor, cx).is_empty());
    assert!(hidden(&editor, cx).is_empty());
}

/// Puts the caret at `offset` in top-level block `ix`.
fn caret_at(editor: &Entity<MarkdownEditor>, cx: &mut VisualTestContext, ix: usize, offset: usize) {
    let b = block(editor, cx, ix);
    let pos = TextPos { block: b, offset };
    select(editor, cx, pos, pos);
}

#[gpui::test]
fn enter_at_the_end_of_a_folded_heading_unfolds_it(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "# A\n\na\n\n# C\n");
    caret_at(&editor, cx, 0, 1);
    press(cx, "secondary-.");
    assert_eq!(hidden(&editor, cx), ["a"]);
    press(cx, "enter");
    type_text(cx, "typed");
    // The new paragraph belongs to the section, so the section opens
    // instead of hiding it (or the caret jumping into "C").
    assert_eq!(md(&editor, cx), "# A\n\ntyped\n\na\n\n# C\n");
    assert_eq!(caret(&editor, cx), "typed‸");
    assert!(hidden(&editor, cx).is_empty());
    assert!(folds(&editor, cx).is_empty());
}

#[gpui::test]
fn movement_skips_hidden_node_blocks(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "# A\n\n---\n");
    caret_at(&editor, cx, 0, 1);
    press(cx, "secondary-.");
    assert_eq!(hidden(&editor, cx), [""]);
    press(cx, "right");
    assert_eq!(caret(&editor, cx), "A‸", "the hidden rule is not selected");
    press(cx, "delete");
    assert_eq!(md(&editor, cx), "# A\n\n---\n", "nothing is deleted unseen");

    let (editor, cx) = open(cx, "# A\n\n---\n\n# C\n");
    caret_at(&editor, cx, 0, 1);
    press(cx, "secondary-.");
    press(cx, "right");
    assert_eq!(caret(&editor, cx), "‸C");
    press(cx, "left");
    assert_eq!(caret(&editor, cx), "A‸");

    // A node selection inside a section being folded leaves it.
    let (editor, cx) = open(cx, "# A\n\n---\n\n# C\n");
    let rule = block(&editor, cx, 1);
    editor.update(cx, |e, cx| e.__select_cells_or_node(rule, None, cx));
    let heading = block(&editor, cx, 0);
    editor.update_in(cx, |e, window, cx| e.toggle_fold(heading, window, cx));
    assert_eq!(caret(&editor, cx), "‸C");
}

#[gpui::test]
fn a_block_dropped_after_a_folded_section_unfolds_it(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "P\n\n# A\n\na\n");
    caret_at(&editor, cx, 1, 0);
    press(cx, "secondary-.");
    assert_eq!(hidden(&editor, cx), ["a"]);
    let from = editor.update_in(cx, |editor, window, _| {
        let ts = window.text_system().clone();
        let view = editor.view.clone();
        let mut q = view.query(editor.layout_input(), &ts);
        q.drag_handle(0).expect("handle").center()
    });
    let a = block(&editor, cx, 1);
    let to = point_of(
        &editor,
        cx,
        TextPos {
            block: a,
            offset: 0,
        },
    );
    let to = gpui::point(to.x, to.y + gpui::px(40.));
    cx.simulate_mouse_move(from, None, Modifiers::none());
    cx.simulate_mouse_down(from, gpui::MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, gpui::MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(to, gpui::MouseButton::Left, Modifiers::none());
    assert_eq!(md(&editor, cx), "# A\n\na\n\nP\n");
    assert!(hidden(&editor, cx).is_empty(), "P stays visible");
    assert!(folds(&editor, cx).is_empty());
}

#[gpui::test]
fn a_heading_merged_into_a_folded_section_unfolds_it(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "# A\n\na\n\n## B\n\nb\n\n# C\n\nc\n");
    caret_at(&editor, cx, 2, 0);
    press(cx, "secondary-.");
    assert_eq!(hidden(&editor, cx), ["b"]);
    caret_at(&editor, cx, 4, 0);
    press(cx, "backspace");
    assert_eq!(caret(&editor, cx), "‸C");
    assert!(hidden(&editor, cx).is_empty(), "C and c stay visible");
    assert!(folds(&editor, cx).is_empty());
}
