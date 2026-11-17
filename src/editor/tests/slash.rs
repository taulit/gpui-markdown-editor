//! The slash menu.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};

use super::*;
use crate::command::Command;
use crate::model::BlockKind;
use crate::options::{SlashItem, UiParts};

/// The labels of the open menu, or `None`.
fn menu(editor: &Entity<MarkdownEditor>, cx: &mut VisualTestContext) -> Option<Vec<String>> {
    editor.read_with(cx, |editor, _| {
        editor
            .slash
            .as_ref()
            .map(|m| m.entries.iter().map(|e| e.label.to_string()).collect())
    })
}

fn kind(editor: &Entity<MarkdownEditor>, cx: &mut VisualTestContext, ix: usize) -> BlockKind {
    editor.read_with(cx, |editor, _| {
        editor.document().blocks()[ix].kind().clone()
    })
}

#[gpui::test]
fn typing_a_slash_filters_and_enter_applies(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "");
    type_text(cx, "/");
    assert_eq!(menu(&editor, cx).unwrap().len(), 12);
    assert!(cx.debug_bounds("slash-text").is_some(), "the menu is drawn");
    type_text(cx, "he");
    assert_eq!(
        menu(&editor, cx).unwrap(),
        ["Heading 1", "Heading 2", "Heading 3"]
    );
    press(cx, "down down up");
    press(cx, "enter");
    assert_eq!(menu(&editor, cx), None);
    assert_eq!(kind(&editor, cx, 0), BlockKind::Heading(2));
    assert_eq!(caret(&editor, cx), "‸");
    type_text(cx, "Plan");
    assert_eq!(md(&editor, cx), "## Plan\n");

    // Undo reverts the typing, the block change, then brings `/he` back.
    press(cx, "secondary-z");
    press(cx, "secondary-z");
    assert_eq!(kind(&editor, cx, 0), BlockKind::Paragraph);
    press(cx, "secondary-z");
    assert_eq!(caret(&editor, cx), "«/he‸");
}

#[gpui::test]
fn slash_only_opens_at_line_start_or_after_a_space(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "");
    type_text(cx, "a/");
    assert_eq!(menu(&editor, cx), None);
    type_text(cx, " /tab");
    assert_eq!(menu(&editor, cx).unwrap(), ["Table"]);
    // A space after a query without results ends the menu.
    type_text(cx, "xx ");
    assert_eq!(menu(&editor, cx), None);
    assert_eq!(caret(&editor, cx), "a/ /tabxx ‸");
}

#[gpui::test]
fn no_slash_menu_in_code_blocks(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "```\ncode\n```\n");
    press(cx, "end");
    type_text(cx, " /");
    assert_eq!(menu(&editor, cx), None);
}

#[gpui::test]
fn escape_and_backspace_close_the_menu(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "Note\n");
    press(cx, "end");
    type_text(cx, " /q");
    assert!(menu(&editor, cx).is_some());
    press(cx, "escape");
    assert_eq!(menu(&editor, cx), None);
    assert_eq!(caret(&editor, cx), "Note /q‸");

    press(cx, "backspace backspace");
    type_text(cx, "/");
    assert!(menu(&editor, cx).is_some());
    press(cx, "backspace");
    assert_eq!(menu(&editor, cx), None);
    // Leaving the query with the caret closes it too.
    type_text(cx, "/");
    press(cx, "left");
    assert_eq!(menu(&editor, cx), None);
}

#[gpui::test]
fn click_chooses_and_host_items_run(cx: &mut TestAppContext) {
    let called = Rc::new(Cell::new(false));
    let flag = called.clone();
    let options = EditorOptions {
        slash_items: vec![
            SlashItem::new("meeting", "Meeting notes", move |editor, window, cx| {
                flag.set(true);
                editor.command(Command::InsertText("Attendees".into()), window, cx);
            })
            .keywords(["template"]),
        ],
        ..options()
    };
    let (editor, cx) = open_with(cx, "", options);
    type_text(cx, "/tab");
    let table = cx.debug_bounds("slash-table").expect("table entry");
    cx.simulate_click(table.center(), Modifiers::none());
    assert_eq!(menu(&editor, cx), None);
    // An empty paragraph is replaced by the table.
    assert!(matches!(kind(&editor, cx, 0), BlockKind::Table(_)));

    press(cx, "doc-end");
    type_text(cx, "/templ");
    assert_eq!(menu(&editor, cx).unwrap(), ["Meeting notes"]);
    press(cx, "tab");
    assert!(called.get());
    assert_eq!(caret(&editor, cx), "Attendees‸");
}

#[gpui::test]
fn slash_menu_can_be_turned_off(cx: &mut TestAppContext) {
    let options = EditorOptions {
        ui: UiParts {
            slash_menu: false,
            ..UiParts::ALL
        },
        ..options()
    };
    let (editor, cx) = open_with(cx, "", options);
    type_text(cx, "/");
    assert_eq!(menu(&editor, cx), None);
    press(cx, "enter");
    assert_eq!(editor.read_with(cx, |e, _| e.document().blocks().len()), 2);
    assert_eq!(
        editor.read_with(cx, |e, _| e.document().blocks()[0].text().to_string()),
        "/"
    );
}
