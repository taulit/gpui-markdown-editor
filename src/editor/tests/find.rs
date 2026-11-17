//! The find bar.

use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};

use super::*;
use crate::command::Command;
use crate::options::UiParts;

const NOTE: &str = "Milk and bread\n\nmore milk\n\nNo MILK here? milk.\n";

/// The number of matches, the current index and the label.
fn state(editor: &Entity<MarkdownEditor>, cx: &mut VisualTestContext) -> Option<(usize, usize)> {
    editor.read_with(cx, |editor, _| {
        editor
            .find
            .as_ref()
            .map(|f| (f.matches.len(), f.current.map_or(usize::MAX, |c| c + 1)))
    })
}

#[gpui::test]
fn ctrl_f_finds_steps_and_escape_selects_the_match(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, NOTE);
    press(cx, "secondary-f");
    assert_eq!(state(&editor, cx), Some((0, usize::MAX)));
    assert!(
        !editor_focused(&editor, cx),
        "the query field has the focus"
    );

    cx.simulate_input("milk");
    assert_eq!(state(&editor, cx), Some((4, 1)));
    let label = cx.debug_bounds("find-count");
    assert!(label.is_some(), "the bar is drawn");

    press(cx, "enter");
    assert_eq!(state(&editor, cx), Some((4, 2)));
    press(cx, "f3 f3");
    assert_eq!(state(&editor, cx), Some((4, 4)));
    press(cx, "enter");
    assert_eq!(state(&editor, cx), Some((4, 1)), "wraps around");
    press(cx, "shift-enter");
    assert_eq!(state(&editor, cx), Some((4, 4)));
    press(cx, "shift-f3");
    assert_eq!(state(&editor, cx), Some((4, 3)));

    press(cx, "escape");
    assert_eq!(state(&editor, cx), None);
    assert!(editor_focused(&editor, cx));
    assert_eq!(caret(&editor, cx), "No «MILK‸ here? milk.");

    // The query is remembered.
    press(cx, "secondary-f");
    assert_eq!(state(&editor, cx).map(|s| s.0), Some(4));
}

#[gpui::test]
fn case_toggle_and_selection_as_query(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, NOTE);
    let b = block(&editor, cx, 1);
    select(
        &editor,
        cx,
        TextPos {
            block: b,
            offset: 5,
        },
        TextPos {
            block: b,
            offset: 9,
        },
    );
    press(cx, "secondary-f");
    assert_eq!(state(&editor, cx), Some((4, 2)), "starts at the selection");
    press(cx, "alt-c");
    assert_eq!(state(&editor, cx), Some((2, 1)));
    let case = cx.debug_bounds("ui-find-case").expect("case button");
    cx.simulate_click(case.center(), Modifiers::none());
    assert_eq!(state(&editor, cx).map(|s| s.0), Some(4));
}

#[gpui::test]
fn matches_follow_edits_and_close_button(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, NOTE);
    editor.update_in(cx, |editor, window, cx| {
        editor.command(Command::OpenFind, window, cx)
    });
    cx.simulate_input("bread");
    assert_eq!(state(&editor, cx), Some((1, 1)));
    // Typing in the document while the bar is open updates the matches.
    editor.update_in(cx, |editor, window, cx| {
        let pos = editor.state.last_pos().unwrap();
        editor.state.set_selection(Selection::caret(pos));
        editor.command(Command::InsertText(" bread".into()), window, cx);
    });
    assert_eq!(state(&editor, cx), Some((2, 1)));

    let close = cx.debug_bounds("ui-find-close").expect("close button");
    cx.simulate_click(close.center(), Modifiers::none());
    assert_eq!(state(&editor, cx), None);
    assert!(editor_focused(&editor, cx));
    assert!(md(&editor, cx).ends_with("milk. bread\n"));
}

#[gpui::test]
fn find_bar_can_be_turned_off(cx: &mut TestAppContext) {
    let options = EditorOptions {
        ui: UiParts {
            find_bar: false,
            ..UiParts::ALL
        },
        ..options()
    };
    let (editor, cx) = open_with(cx, NOTE, options);
    press(cx, "secondary-f");
    editor.update_in(cx, |editor, window, cx| {
        editor.command(Command::OpenFind, window, cx)
    });
    assert_eq!(state(&editor, cx), None);
    // The public search is independent of the bar.
    assert_eq!(editor.read_with(cx, |e, _| e.find("milk").len()), 4);
}

#[gpui::test]
fn host_highlights_scroll_to_the_first_one(cx: &mut gpui::TestAppContext) {
    let doc: String = (0..200).map(|i| format!("line {i}\n\n")).collect();
    let (editor, cx) = open(cx, &doc);
    let top = |editor: &gpui::Entity<MarkdownEditor>, cx: &mut gpui::VisualTestContext| {
        editor.update_in(cx, |e, window, _| {
            let ts = window.text_system().clone();
            let view = e.view.clone();
            view.query(e.layout_input(), &ts).scroll_top()
        })
    };
    assert_eq!(top(&editor, cx), gpui::px(0.));
    let before = caret(&editor, cx);
    let hits = editor.read_with(cx, |e, _| e.find("line 150"));
    assert_eq!(hits.len(), 1);
    editor.update(cx, |e, cx| e.highlight(hits.clone(), cx));
    cx.run_until_parked();
    assert!(top(&editor, cx) > gpui::px(1000.));
    assert_eq!(caret(&editor, cx), before, "the caret stays");
    let target = point_of(&editor, cx, hits[0].start);
    assert!(target.y > gpui::px(0.) && target.y < gpui::px(600.));
}
