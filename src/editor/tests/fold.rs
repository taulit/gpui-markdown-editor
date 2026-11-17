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
