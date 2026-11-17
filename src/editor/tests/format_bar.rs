//! The format bar.

use gpui::{Entity, Focusable as _, Modifiers, MouseButton, TestAppContext, VisualTestContext};

use super::*;
use crate::model::{BlockKind, Mark};
use crate::options::UiParts;

fn shown(cx: &mut VisualTestContext, selector: &'static str) -> bool {
    cx.debug_bounds(selector).is_some()
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector}"));
    cx.simulate_click(bounds.center(), Modifiers::none());
}

fn first_kind(editor: &Entity<MarkdownEditor>, cx: &mut VisualTestContext) -> BlockKind {
    editor.read_with(cx, |e, _| e.document().blocks()[0].kind().clone())
}

#[gpui::test]
fn bar_adapts_to_the_selection(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "Milk and bread\n\nEggs\n");
    // Caret: insert tools.
    assert!(shown(cx, "ui-format-styles"));
    assert!(shown(cx, "ui-format-checklist"));
    assert!(shown(cx, "ui-format-table"));
    assert!(!shown(cx, "ui-format-bold"));

    // Text in one block: marks.
    let b = block(&editor, cx, 0);
    select(
        &editor,
        cx,
        TextPos {
            block: b,
            offset: 0,
        },
        TextPos {
            block: b,
            offset: 4,
        },
    );
    assert!(shown(cx, "ui-format-bold"));
    assert!(shown(cx, "ui-format-highlight"));
    click(cx, "ui-format-bold");
    assert_eq!(md(&editor, cx), "**Milk** and bread\n\nEggs\n");
    assert!(editor.read_with(cx, |e, _| e.state.has_mark(&Mark::Bold)));
    assert!(cx.update(|window, cx| editor.focus_handle(cx).is_focused(window)));

    // Several blocks: block tools.
    let c = block(&editor, cx, 1);
    select(
        &editor,
        cx,
        TextPos {
            block: b,
            offset: 0,
        },
        TextPos {
            block: c,
            offset: 2,
        },
    );
    assert!(shown(cx, "ui-format-list"));
    click(cx, "ui-format-quote");
    assert_eq!(first_kind(&editor, cx), BlockKind::Quote);
}

#[gpui::test]
fn style_popover_sets_the_block_style(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "Milk\n");
    click(cx, "ui-format-styles");
    assert!(shown(cx, "ui-style-heading"));
    click(cx, "ui-style-heading");
    assert_eq!(first_kind(&editor, cx), BlockKind::Heading(2));
    assert!(!shown(cx, "ui-style-heading"), "closes after choosing");
    // Choosing the current style keeps it.
    click(cx, "ui-format-styles");
    click(cx, "ui-style-heading");
    assert_eq!(first_kind(&editor, cx), BlockKind::Heading(2));
    // Escape closes the popover.
    click(cx, "ui-format-styles");
    press(cx, "escape");
    assert!(!shown(cx, "ui-style-heading"));
}

#[gpui::test]
fn bar_hides_in_code_unfocused_and_while_dragging(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "```\ncode\n```\n\nText here\n");
    assert!(!shown(cx, "ui-format-styles"), "in a code block");
    press(cx, "down down");
    assert!(shown(cx, "ui-format-styles"));

    let b = block(&editor, cx, 1);
    let from = point_of(
        &editor,
        cx,
        TextPos {
            block: b,
            offset: 0,
        },
    );
    let to = point_of(
        &editor,
        cx,
        TextPos {
            block: b,
            offset: 4,
        },
    );
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    assert!(!shown(cx, "ui-format-bold"), "hidden while selecting");
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    assert!(shown(cx, "ui-format-bold"));

    cx.update(|window, cx| window.blur(cx));
    cx.run_until_parked();
    assert!(!shown(cx, "ui-format-styles"), "unfocused");
}

#[gpui::test]
fn table_tools_and_link_button(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "| a | b |\n|---|---|\n| 1 | 2 |\n\nNote\n");
    let rows = |editor: &Entity<MarkdownEditor>, cx: &mut VisualTestContext| {
        editor.read_with(cx, |e, _| match e.document().blocks()[0].kind() {
            BlockKind::Table(t) => (t.rows.len(), t.rows[0].len()),
            _ => (0, 0),
        })
    };
    assert!(shown(cx, "ui-format-add-row"));
    click(cx, "ui-format-add-row");
    click(cx, "ui-format-add-column");
    assert_eq!(rows(&editor, cx), (3, 3));
    click(cx, "ui-format-delete-column");
    assert_eq!(rows(&editor, cx), (3, 2));

    press(cx, "doc-end");
    click(cx, "ui-format-link");
    assert!(editor.read_with(cx, |e, _| e.prompt.is_some()));
}

#[gpui::test]
fn format_bar_can_be_turned_off(cx: &mut TestAppContext) {
    let options = EditorOptions {
        ui: UiParts {
            format_bar: false,
            ..UiParts::ALL
        },
        ..options()
    };
    let (_editor, cx) = open_with(cx, "Milk\n", options);
    assert!(!shown(cx, "ui-format-styles"));
}
