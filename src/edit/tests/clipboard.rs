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
    assert_eq!(
        state.selected_markdown(),
        "# tle\n\n- **bold** item\n- other\n"
    );
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
    assert_eq!(
        md(&state),
        "| a |\n|---|\n\nx\n\n| b   |\n| --- |\n| c   |\n"
    );
}

#[test]
fn markdown_into_a_cell_is_flattened() {
    let mut state = ed("| a‸ |\n|---|\n");
    state.insert_markdown("# x\n\ny");
    assert_eq!(md(&state), "| ax y |\n| ---- |\n");
}

#[test]
fn select_all_covers_node_blocks_at_the_edges() {
    let source = "---\n\ntext‸\n\n| a | b |\n|---|---|\n| c | d |\n\n![i](i.png)\n";
    let mut state = ed(source);
    state.select_all();
    state.select_all();
    assert_eq!(state.selection, Selection::All);
    assert_eq!(
        state.copy_text(),
        "---\n\ntext\n\n| a   | b   |\n| --- | --- |\n| c   | d   |\n\n![i](i.png)"
    );
    assert!(state.delete_selection());
    assert_eq!(md(&state), "\n");
    assert_eq!(state.doc.blocks.len(), 1);
    state.undo();
    assert_eq!(state.markdown(), source.replace(CARET, ""));

    // The public command selects everything at once; typing replaces it.
    let mut state = ed(source);
    state.exec(
        &crate::command::Command::SelectAll,
        std::time::Instant::now(),
    );
    state.select_all();
    type_str(&mut state, &mut Clock::new(), "x");
    assert_eq!(md(&state), "x\n");
}

#[test]
fn deleting_a_range_that_covers_a_table_removes_it() {
    let mut state = ed("| «a | b |\n|---|---|\n| c | d |\n\nnext‸ line\n");
    backspace(&mut state);
    assert_eq!(md(&state), "&#32;line\n");
}
