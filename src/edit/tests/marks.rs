//! Toggling marks and links.

use super::*;
use crate::command::Command;
use crate::model::Mark;

fn md(state: &EditorState) -> String {
    check_invariants(state).unwrap();
    state.markdown()
}

#[test]
fn toggle_mark_adds_and_removes() {
    let mut state = ed("one «two‸ three\n");
    assert!(state.toggle_mark(Mark::Bold));
    assert_eq!(md(&state), "one **two** three\n");
    assert!(state.has_mark(&Mark::Bold));
    assert!(state.toggle_mark(Mark::Bold));
    assert_eq!(md(&state), "one two three\n");
}

#[test]
fn toggle_mark_on_a_partly_marked_range_adds() {
    let mut state = ed("«a **b** c‸\n");
    state.toggle_mark(Mark::Bold);
    assert_eq!(md(&state), "**a b c**\n");
}

#[test]
fn toggle_mark_across_blocks_skips_code() {
    let mut state = ed("al«pha\n\n```\ncode\n```\n\n- be‸ta\n");
    state.toggle_mark(Mark::Italic);
    assert_eq!(md(&state), "al*pha*\n\n```\ncode\n```\n\n- *be*ta\n");
}

#[test]
fn toggle_mark_in_table_cells() {
    let mut state = ed("| «a | b‸ |\n|---|---|\n| c | d |\n");
    state.toggle_mark(Mark::Strike);
    assert_eq!(md(&state), "| ~~a~~ | ~~b~~ |\n| ----- | ----- |\n| c     | d     |\n");
}

#[test]
fn set_link_on_a_selection_and_at_a_caret() {
    let mut state = ed("see «docs‸ now\n");
    state.exec(&Command::SetLink(Some("https://x.y".into())), std::time::Instant::now());
    assert_eq!(md(&state), "see [docs](https://x.y) now\n");

    let mut state = ed("see [do‸cs](/old) now\n");
    state.set_link(Some("/new"));
    assert_eq!(md(&state), "see [docs](/new) now\n");
    state.set_link(None);
    assert_eq!(md(&state), "see docs now\n");

    let mut state = ed("go ‸!\n");
    state.set_link(Some("https://x.y"));
    assert_eq!(md(&state), "go <https://x.y>!\n");
    assert_eq!(caret(&state), "go https://x.y‸!");
}

#[test]
fn caret_marks_follow_the_text_before_the_caret() {
    let state = ed("**bold‸** plain\n");
    assert!(state.has_mark(&Mark::Bold));
    let state = ed("**bold** ‸plain\n");
    assert!(!state.has_mark(&Mark::Bold));
}
