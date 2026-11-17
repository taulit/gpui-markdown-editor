//! The link popover and the link bubble.

use gpui::{ClipboardItem, Entity, Focusable as _, Modifiers, TestAppContext, VisualTestContext};

use super::*;
use crate::model::BlockKind;
use crate::options::UiParts;

fn prompt_open(editor: &Entity<MarkdownEditor>, cx: &mut VisualTestContext) -> bool {
    editor.read_with(cx, |editor, _| editor.prompt.is_some())
}

fn editor_focused(editor: &Entity<MarkdownEditor>, cx: &mut VisualTestContext) -> bool {
    cx.update(|window, cx| editor.focus_handle(cx).is_focused(window))
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector}"));
    cx.simulate_click(bounds.center(), Modifiers::none());
}

/// Selects `range` of the first block.
fn select_in_first(
    editor: &Entity<MarkdownEditor>,
    cx: &mut VisualTestContext,
    range: std::ops::Range<usize>,
) {
    let b = block(editor, cx, 0);
    select(
        editor,
        cx,
        TextPos {
            block: b,
            offset: range.start,
        },
        TextPos {
            block: b,
            offset: range.end,
        },
    );
}

#[gpui::test]
fn ctrl_k_asks_for_the_url_and_enter_applies(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "Milk and bread\n");
    select_in_first(&editor, cx, 9..14);
    press(cx, "secondary-k");
    assert!(prompt_open(&editor, cx));
    assert!(!editor_focused(&editor, cx));
    assert!(cx.debug_bounds("ui-link-apply").is_some());
    cx.simulate_input("https://example.com");
    press(cx, "enter");
    assert!(!prompt_open(&editor, cx));
    assert!(editor_focused(&editor, cx));
    assert_eq!(md(&editor, cx), "Milk and [bread](https://example.com)\n");

    // Undo is one step.
    press(cx, "secondary-z");
    assert_eq!(md(&editor, cx), "Milk and bread\n");
}

#[gpui::test]
fn escape_cancels_and_keeps_the_selection(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "Milk and bread\n");
    select_in_first(&editor, cx, 0..4);
    press(cx, "secondary-k");
    cx.simulate_input("https://x.org");
    press(cx, "escape");
    assert!(!prompt_open(&editor, cx));
    assert!(editor_focused(&editor, cx));
    assert_eq!(md(&editor, cx), "Milk and bread\n");
    assert_eq!(caret(&editor, cx), "«Milk‸ and bread");
}

#[gpui::test]
fn clipboard_url_is_suggested_and_caret_inserts_linked_text(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "See you\n");
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("https://c.d/e".into())));
    select_in_first(&editor, cx, 4..4);
    press(cx, "secondary-k");
    press(cx, "enter");
    assert_eq!(md(&editor, cx), "See <https://c.d/e>you\n");
}

#[gpui::test]
fn bubble_opens_edits_and_removes_links(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "Go to [the shop](https://shop.example) now\n");
    let events = record_events(&editor, cx);
    select_in_first(&editor, cx, 8..8);
    assert!(cx.debug_bounds("link-url").is_some(), "the bubble is shown");

    click(cx, "ui-link-open");
    assert_eq!(
        count(
            &events,
            &EditorEvent::LinkClicked("https://shop.example".into())
        ),
        1
    );

    click(cx, "ui-link-edit");
    assert!(prompt_open(&editor, cx));
    assert!(cx.debug_bounds("ui-link-remove").is_some(), "existing link");
    // The field holds the URL, selected: typing replaces it.
    cx.simulate_input("https://other.example");
    press(cx, "enter");
    assert_eq!(
        md(&editor, cx),
        "Go to [the shop](https://other.example) now\n"
    );

    select_in_first(&editor, cx, 8..8);
    click(cx, "ui-link-unlink");
    assert_eq!(md(&editor, cx), "Go to the shop now\n");
    assert!(cx.debug_bounds("link-url").is_none());

    // Remove in the popover.
    press(cx, "secondary-z");
    select_in_first(&editor, cx, 8..8);
    press(cx, "secondary-k");
    click(cx, "ui-link-remove");
    assert!(!prompt_open(&editor, cx));
    assert_eq!(md(&editor, cx), "Go to the shop now\n");
}

#[gpui::test]
fn image_prompt_from_the_slash_menu(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "");
    type_text(cx, "/imag");
    press(cx, "enter");
    assert!(prompt_open(&editor, cx));
    cx.simulate_input("photos/lake.png");
    press(cx, "enter");
    let kind = editor.read_with(cx, |e, _| e.document().blocks()[0].kind().clone());
    assert!(matches!(kind, BlockKind::Image(_)), "{kind:?}");
    assert_eq!(md(&editor, cx), "![](photos/lake.png)\n");
}

#[gpui::test]
fn without_the_popover_ctrl_k_uses_the_clipboard(cx: &mut TestAppContext) {
    let options = EditorOptions {
        ui: UiParts {
            link_popover: false,
            ..UiParts::ALL
        },
        ..options()
    };
    let (editor, cx) = open_with(cx, "Milk\n", options);
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("https://m.example".into())));
    select_in_first(&editor, cx, 0..4);
    press(cx, "secondary-k");
    assert!(!prompt_open(&editor, cx));
    assert_eq!(md(&editor, cx), "[Milk](https://m.example)\n");
    select_in_first(&editor, cx, 2..2);
    assert!(cx.debug_bounds("link-url").is_none(), "no bubble");
}

#[gpui::test]
fn links_and_tags_use_the_accent_color(cx: &mut gpui::TestAppContext) {
    let (editor, cx) = open(cx, "");
    let (style, primary) = editor.read_with(cx, |e, cx| {
        use gpui_component::ActiveTheme as _;
        (e.style().clone(), cx.theme().primary)
    });
    assert_eq!(style.link_color, primary);
    assert_eq!(style.tag_color, primary);
    assert_eq!(style.quote_bar_color, primary);
}
