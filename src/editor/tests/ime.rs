//! IME composition through the input handler.

use gpui::{EntityInputHandler as _, TestAppContext};

use super::*;
use crate::view::ViewSource as _;

fn mark(
    editor: &Entity<MarkdownEditor>,
    cx: &mut VisualTestContext,
    range: Option<std::ops::Range<usize>>,
    text: &str,
    selected: Option<std::ops::Range<usize>>,
) {
    editor.update_in(cx, |e, window, cx| {
        e.replace_and_mark_text_in_range(range, text, selected, window, cx)
    });
}

fn commit(
    editor: &Entity<MarkdownEditor>,
    cx: &mut VisualTestContext,
    range: Option<std::ops::Range<usize>>,
    text: &str,
) {
    editor.update_in(cx, |e, window, cx| {
        e.replace_text_in_range(range, text, window, cx)
    });
}

fn marked(
    editor: &Entity<MarkdownEditor>,
    cx: &mut VisualTestContext,
) -> Option<std::ops::Range<usize>> {
    editor.update_in(cx, |e, window, cx| e.marked_text_range(window, cx))
}

#[gpui::test]
fn compose_and_commit(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "caf\n");
    press(cx, "end");
    mark(&editor, cx, None, "´", None);
    assert_eq!(marked(&editor, cx), Some(3..4));
    // The marked text is drawn underlined.
    let underlined = editor.read_with(cx, |e, _| e.view_input().marked);
    assert_eq!(underlined.map(|(_, r)| r), Some(3..5));
    commit(&editor, cx, None, "é");
    assert_eq!(marked(&editor, cx), None);
    assert_eq!(caret(&editor, cx), "café‸");
    assert_eq!(md(&editor, cx), "café\n");
    press(cx, "secondary-z");
    assert_eq!(md(&editor, cx), "caf\n");
}

#[gpui::test]
fn pinyin_with_inner_selection(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "x\n");
    mark(&editor, cx, None, "ni", Some(2..2));
    mark(&editor, cx, None, "nih", Some(3..3));
    assert_eq!(caret(&editor, cx), "nih‸x");
    mark(&editor, cx, None, "你好", Some(1..2));
    assert_eq!(caret(&editor, cx), "你«好‸x");
    let selected = editor.update_in(cx, |e, window, cx| e.selected_text_range(false, window, cx));
    assert_eq!(selected.map(|s| s.range), Some(1..2));
    commit(&editor, cx, None, "你好");
    assert_eq!(caret(&editor, cx), "你好‸x");
    let mut adjusted = None;
    let text = editor.update_in(cx, |e, window, cx| {
        e.text_for_range(0..9, &mut adjusted, window, cx)
    });
    assert_eq!(text.as_deref(), Some("你好x"));
    assert_eq!(adjusted, Some(0..3));
}

#[gpui::test]
fn mark_delete_and_type_again_does_not_panic(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "ab\n");
    press(cx, "end");
    mark(&editor, cx, None, "xyz", None);
    assert_eq!(marked(&editor, cx), Some(2..5));
    // The marked text disappears behind the input method's back.
    press(cx, "secondary-a backspace");
    assert_eq!(marked(&editor, cx), None);
    // The input method still believes in its old ranges.
    mark(&editor, cx, Some(2..5), "q", None);
    commit(&editor, cx, Some(4..9), "Q");
    commit(&editor, cx, None, "!");
    assert_eq!(md(&editor, cx), "qQ!\n");
    // Moving the caret ends the composition.
    mark(&editor, cx, None, "z", None);
    press(cx, "home");
    assert_eq!(marked(&editor, cx), None);
    // An explicit range wins over the caret, clamped to the text.
    commit(&editor, cx, Some(100..200), "<");
    assert_eq!(md(&editor, cx), "qQ!z<\n");
}

#[gpui::test]
fn candidate_window_follows_the_caret(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "hello world\n");
    press(cx, "end");
    mark(&editor, cx, None, "ab", None);
    let id = block(&editor, cx, 0);
    let caret_point = point_of(
        &editor,
        cx,
        TextPos {
            block: id,
            offset: 11,
        },
    );
    let bounds = editor
        .update_in(cx, |e, window, cx| {
            e.bounds_for_range(11..13, Default::default(), window, cx)
        })
        .expect("bounds");
    assert!((bounds.left() - caret_point.x).abs() < px(2.));
    assert!(bounds.size.width > px(0.));

    let index = editor.update_in(cx, |e, window, cx| {
        e.character_index_for_point(caret_point, window, cx)
    });
    assert_eq!(index, Some(11));
}
