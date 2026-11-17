//! Copy and paste.

use super::*;

fn md(state: &EditorState) -> String {
    check_invariants(state).unwrap();
    state.markdown()
}

#[test]
fn selected_text_and_markdown() {
    let state = ed("# Ti«tle\n\n- **bold** item\n- other‸ one\n");
    assert_eq!(state.selected_text(), "tle\nbold item\nother");
    assert_eq!(state.selected_markdown(), "# tle\n\n- **bold** item\n- other\n");
    let state = ed("a **b«old‸** c\n");
    assert_eq!(state.selected_markdown(), "**old**\n");
}

#[test]
fn selected_markdown_of_a_child_without_its_parent() {
    let state = ed("- parent\n  - ch«ild‸\n");
    assert_eq!(state.selected_markdown(), "- ild\n");
}

#[test]
fn selected_markdown_of_a_node() {
    let mut state = ed("a‸\n\n---\n");
    state.move_right(Unit::Grapheme, false);
    assert_eq!(state.selected_markdown(), "---\n");
}

#[test]
fn a_single_paragraph_is_inserted_inline() {
    let mut state = ed("ab‸cd\n");
    assert!(state.insert_markdown("**x** y"));
    assert_eq!(md(&state), "ab**x** ycd\n");
    assert_eq!(caret(&state), "abx y‸cd");
}

#[test]
fn several_paragraphs_merge_with_the_ends() {
    let mut state = ed("ab‸cd\n");
    state.insert_markdown("one\n\ntwo\n\nthree");
    assert_eq!(md(&state), "abone\n\ntwo\n\nthreecd\n");
}

#[test]
fn blocks_split_the_current_block() {
    let mut state = ed("ab‸cd\n");
    state.insert_markdown("# Head\n\n- item\n");
    assert_eq!(md(&state), "ab\n\n# Head\n\n- item\n\ncd\n");
    assert_eq!(caret(&state), "item‸");
}

#[test]
fn blocks_replace_an_empty_paragraph() {
    let mut state = ed("x‸\n\nafter\n");
    backspace(&mut state);
    state.insert_markdown("- [ ] a\n- [ ] b\n");
    assert_eq!(md(&state), "- [ ] a\n- [ ] b\n\nafter\n");
    while state.undo() {}
    assert_eq!(state.markdown(), "x\n\nafter\n");
}

#[test]
fn inserted_tables_get_fresh_ids() {
    let mut state = ed("| a |\n|---|\n\nx‸\n");
    state.insert_markdown("| b |\n|---|\n| c |\n");
    check_invariants(&state).unwrap();
    assert_eq!(md(&state), "| a |\n|---|\n\nx\n\n| b   |\n| --- |\n| c   |\n");
}

#[test]
fn markdown_into_a_cell_is_flattened() {
    let mut state = ed("| a‸ |\n|---|\n");
    state.insert_markdown("# x\n\ny");
    assert_eq!(md(&state), "| ax y |\n| ---- |\n");
}
