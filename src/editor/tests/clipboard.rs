//! Copy, cut and paste.

use gpui::{ClipboardItem, TestAppContext};

use super::*;

fn clipboard(cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
}

fn set_clipboard(cx: &mut VisualTestContext, text: &str) {
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string(text.to_string())));
}

fn range(
    editor: &Entity<MarkdownEditor>,
    cx: &mut VisualTestContext,
    ix: usize,
    from: usize,
    to: usize,
) {
    let id = block(editor, cx, ix);
    select(
        editor,
        cx,
        TextPos {
            block: id,
            offset: from,
        },
        TextPos {
            block: id,
            offset: to,
        },
    );
}

#[gpui::test]
fn copy_and_paste_round_trip(cx: &mut TestAppContext) {
    let source = "# Title\n\nSome **bold** text\n\n- [ ] task\n- item\n";
    let (editor, cx) = open(cx, source);
    press(cx, "secondary-a secondary-a secondary-c");
    assert_eq!(
        clipboard(cx).as_deref(),
        Some("# Title\n\nSome **bold** text\n\n- [ ] task\n- item")
    );

    editor.update(cx, |e, cx| e.set_markdown("", cx));
    press(cx, "secondary-v");
    assert_eq!(md(&editor, cx), source);

    // A partial selection copies its marks; pasted inline, they stay.
    let mut app1 = cx.new_app();
    let (editor, cx) = open(&mut app1, "Some bold text\n");
    range(&editor, cx, 0, 5, 9);
    press(cx, "secondary-b secondary-c");
    assert_eq!(clipboard(cx).as_deref(), Some("**bold**"));
    press(cx, "end space secondary-v");
    assert_eq!(md(&editor, cx), "Some **bold** text **bold**\n");
}

#[gpui::test]
fn cut_removes_the_selection(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "keep cut, keep\n");
    range(&editor, cx, 0, 5, 9);
    press(cx, "secondary-x");
    assert_eq!(clipboard(cx).as_deref(), Some("cut,"));
    assert_eq!(md(&editor, cx), "keep  keep\n");

    // Cut without a selection does nothing.
    press(cx, "secondary-x");
    assert_eq!(md(&editor, cx), "keep  keep\n");
    assert_eq!(clipboard(cx).as_deref(), Some("cut,"));
}

#[gpui::test]
fn paste_plain_text_and_markdown(cx: &mut TestAppContext) {
    // Plain lines become paragraphs, the text keeps its characters.
    let (editor, cx) = open(cx, "");
    set_clipboard(cx, "one * two\nthree");
    press(cx, "secondary-v");
    assert_eq!(md(&editor, cx), "one * two\n\nthree\n");

    // Markdown becomes blocks.
    let mut app2 = cx.new_app();
    let (editor, cx) = open(&mut app2, "");
    set_clipboard(cx, "## Heading\n\n- a\n- b\n");
    press(cx, "secondary-v");
    assert_eq!(md(&editor, cx), "## Heading\n\n- a\n- b\n");

    // In a code block everything is literal.
    let mut app3 = cx.new_app();
    let (editor, cx) = open(&mut app3, "```\ncode\n```\n");
    press(cx, "end");
    set_clipboard(cx, "\n# not a heading");
    press(cx, "secondary-v");
    assert_eq!(md(&editor, cx), "```\ncode\n# not a heading\n```\n");
}

#[gpui::test]
fn pasting_a_url_over_a_selection_links_it(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "visit the site\n");
    range(&editor, cx, 0, 10, 14);
    set_clipboard(cx, "https://example.com");
    press(cx, "secondary-v");
    assert_eq!(md(&editor, cx), "visit the [site](https://example.com)\n");

    // Without a selection the URL is plain text.
    press(cx, "end");
    set_clipboard(cx, " https://example.org");
    press(cx, "secondary-v");
    assert_eq!(
        md(&editor, cx),
        "visit the [site](https://example.com) https://example.org\n"
    );
}

#[gpui::test]
fn ctrl_k_links_with_the_clipboard_url_and_unlinks(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "a word here\n");
    range(&editor, cx, 0, 2, 6);
    // Nothing to link to: no change.
    set_clipboard(cx, "not a url");
    press(cx, "secondary-k");
    assert_eq!(md(&editor, cx), "a word here\n");

    set_clipboard(cx, "https://example.com");
    press(cx, "secondary-k");
    assert_eq!(md(&editor, cx), "a [word](https://example.com) here\n");
    press(cx, "secondary-k");
    assert_eq!(md(&editor, cx), "a word here\n");
}
