//! Typing, deleting, Enter, Tab and movement.

use std::time::Instant;

use super::*;
use crate::model::Mark;

fn md(state: &EditorState) -> String {
    check_invariants(state).unwrap();
    state.markdown()
}

#[test]
fn typing_inserts_at_the_caret_and_keeps_other_blocks() {
    let state = typed("# Title\n\nHel‸lo\n\n*  odd   list\n", "p!");
    assert_eq!(md(&state), "# Title\n\nHelp!lo\n\n*  odd   list\n");
    assert_eq!(caret(&state), "Help!‸lo");
}

#[test]
fn typing_replaces_the_selection() {
    let state = typed("one «two‸ three\n", "2");
    assert_eq!(md(&state), "one 2 three\n");
}

#[test]
fn typing_inside_a_mark_extends_it_and_at_its_end_inherits_formatting() {
    let state = typed("a **bo‸ld** c\n", "X");
    assert_eq!(md(&state), "a **boXld** c\n");
    let state = typed("a **bold‸** c\n", "er");
    assert_eq!(md(&state), "a **bolder** c\n");
}

#[test]
fn links_and_code_end_at_their_end() {
    let state = typed("a [link‸](/u) c\n", "s");
    assert_eq!(md(&state), "a [link](/u)s c\n");
    let state = typed("a `code‸` c\n", "s");
    assert_eq!(md(&state), "a `code`s c\n");
    let state = typed("a [li‸nk](/u) c\n", "-");
    assert_eq!(md(&state), "a [li-nk](/u) c\n");
}

#[test]
fn pending_marks_apply_to_the_next_typed_text() {
    let mut state = ed("a‸ z\n");
    assert!(state.toggle_mark(Mark::Bold));
    type_str(&mut state, &mut Clock::new(), "b");
    state.toggle_mark(Mark::Bold);
    type_str(&mut state, &mut Clock::new(), "c");
    assert_eq!(md(&state), "a**b**c z\n");
}

#[test]
fn backspace_and_delete_remove_graphemes_and_words() {
    let mut state = ed("ae\u{301}‸x 🎉\n");
    backspace(&mut state);
    assert_eq!(caret(&state), "a‸x 🎉");
    let mut state = ed("one two‸!\n");
    state.delete_backward(Unit::Word, Instant::now());
    assert_eq!(md(&state), "one !\n");
    let mut state = ed("x‸one two\n");
    state.delete_forward(Unit::Word, Instant::now());
    assert_eq!(md(&state), "x two\n");
    delete(&mut state);
    assert_eq!(md(&state), "xtwo\n");
}

#[test]
fn backspace_at_the_start_of_a_heading_makes_it_a_paragraph() {
    let mut state = ed("Intro\n\n## ‸Head\n");
    backspace(&mut state);
    assert_eq!(md(&state), "Intro\n\nHead\n");
    backspace(&mut state);
    assert_eq!(md(&state), "IntroHead\n");
    assert_eq!(caret(&state), "Intro‸Head");
}

#[test]
fn backspace_in_list_items_and_quotes() {
    let mut state = ed("- a\n- ‸b\n");
    backspace(&mut state);
    assert_eq!(md(&state), "- a\n\nb\n");
    let mut state = ed("> ‸quote\n");
    backspace(&mut state);
    assert_eq!(md(&state), "quote\n");
}

#[test]
fn backspace_after_a_rule_selects_it_and_delete_removes_it() {
    let mut state = ed("a\n\n---\n\n‸b\n");
    backspace(&mut state);
    assert_eq!(caret(&state), "<node Rule>");
    backspace(&mut state);
    assert_eq!(md(&state), "a\n\nb\n");
    assert_eq!(caret(&state), "‸b");
}

#[test]
fn backspace_in_an_empty_paragraph_after_an_image_removes_it() {
    let mut state = ed("![i](x.png)\n\nx‸\n");
    backspace(&mut state);
    assert!(backspace(&mut state));
    assert_eq!(md(&state), "![i](x.png)\n");
    assert_eq!(caret(&state), "<node Image(ImageData { src: \"x.png\", alt: \"i\", title: None, width: None })>");
}

#[test]
fn delete_at_the_end_joins_the_next_block() {
    let mut state = ed("one‸\n\n- two\n  - three\n");
    delete(&mut state);
    assert_eq!(md(&state), "onetwo\n\n- three\n");
    assert_eq!(caret(&state), "one‸two");
}

#[test]
fn deleting_a_selection_across_blocks_merges_first_and_last() {
    let mut state = ed("# Ti«tle\n\nmiddle\n\n- it‸em\n- next\n");
    backspace(&mut state);
    assert_eq!(md(&state), "# Tiem\n\n- next\n");
    assert_eq!(caret(&state), "Ti‸em");
}

#[test]
fn deleting_from_a_parent_into_its_child() {
    let mut state = ed("- a«b\n  - c\n  - d‸e\n    - f\n");
    backspace(&mut state);
    assert_eq!(md(&state), "- ae\n  - f\n");
}

#[test]
fn enter_splits_paragraphs_and_marks() {
    let mut state = ed("**bo‸ld** text\n");
    enter(&mut state);
    assert_eq!(md(&state), "**bo**\n\n**ld** text\n");
    assert_eq!(caret(&state), "‸ld text");
}

#[test]
fn enter_in_list_items_creates_items_of_the_same_kind() {
    let state = typed("- [x] done‸\n", "\nnext");
    assert_eq!(md(&state), "- [x] done\n- [ ] next\n");
    let state = typed("1. one‸\n2. two\n", "\nnew");
    assert_eq!(md(&state), "1. one\n2. new\n3. two\n");
    let state = typed("- a‸\n", "\nb");
    assert_eq!(md(&state), "- a\n- b\n");
}

#[test]
fn enter_in_an_empty_item_outdents_and_then_ends_the_list() {
    let mut state = typed("- a\n  - b‸\n", "\n");
    assert_eq!(md(&state), "- a\n  - b\n  -\n");
    enter(&mut state);
    assert_eq!(md(&state), "- a\n  - b\n-\n");
    enter(&mut state);
    assert_eq!(md(&state), "- a\n  - b\n\n\n");
    type_str(&mut state, &mut Clock::new(), "text");
    assert_eq!(md(&state), "- a\n  - b\n\ntext\n");
}

#[test]
fn enter_at_the_end_of_a_heading_creates_a_paragraph() {
    let state = typed("# Title‸\n", "\nbody");
    assert_eq!(md(&state), "# Title\n\nbody\n");
    let state = typed("# Ti‸tle\n", "\n");
    assert_eq!(md(&state), "# Ti\n\n# tle\n");
    let mut state = ed("# ‸Title\n");
    enter(&mut state);
    assert_eq!(caret(&state), "‸Title");
    assert_eq!(md(&state), "\n\n# Title\n");
}

#[test]
fn enter_in_code_inserts_newlines_and_exits_on_an_empty_last_line() {
    let mut state = typed("```\nlet x‸\n```\n", "\n");
    assert_eq!(md(&state), "```\nlet x\n\n```\n");
    enter(&mut state);
    assert_eq!(md(&state), "```\nlet x\n```\n\n\n");
    type_str(&mut state, &mut Clock::new(), "after");
    assert_eq!(md(&state), "```\nlet x\n```\n\nafter\n");
}

#[test]
fn shift_enter_inserts_a_hard_break() {
    let mut state = ed("a‸b\n");
    state.soft_break(Instant::now());
    assert_eq!(md(&state), "a\\\nb\n");
}

#[test]
fn tab_indents_and_shift_tab_outdents_list_items() {
    let mut state = ed("- a\n- b‸\n  - c\n- d\n");
    assert!(state.tab(Instant::now()));
    assert_eq!(md(&state), "- a\n  - b\n    - c\n- d\n");
    assert!(state.shift_tab());
    assert_eq!(md(&state), "- a\n- b\n  - c\n- d\n");
    // The first item has nothing to nest under.
    let mut state = ed("- a‸\n- b\n");
    assert!(!state.tab(Instant::now()));
}

#[test]
fn outdent_takes_the_following_siblings_along() {
    let mut state = ed("- a\n  - b‸\n  - c\n");
    assert!(state.outdent());
    assert_eq!(md(&state), "- a\n- b\n  - c\n");
}

#[test]
fn indenting_ordered_items_renumbers_both_lists() {
    let mut state = ed("1. a\n2. b‸\n3. c\n");
    state.indent();
    assert_eq!(md(&state), "1. a\n   1. b\n2. c\n");
}

#[test]
fn tab_in_code_inserts_spaces_and_shift_tab_removes_them() {
    let mut state = ed("```\nfn‸\n```\n");
    state.tab(Instant::now());
    assert_eq!(md(&state), "```\nfn    \n```\n");
    let mut state = ed("```\n    fn‸\n```\n");
    state.shift_tab();
    assert_eq!(md(&state), "```\nfn\n```\n");
    assert_eq!(caret(&state), "fn‸");
}

#[test]
fn movement_crosses_nodes_and_selects_node_blocks() {
    let mut state = ed("a‸b\n\n---\n\ncd\n");
    assert!(state.move_right(Unit::Grapheme, false));
    assert_eq!(caret(&state), "ab‸");
    state.move_right(Unit::Grapheme, false);
    assert_eq!(caret(&state), "<node Rule>");
    state.move_right(Unit::Grapheme, false);
    assert_eq!(caret(&state), "‸cd");
    state.move_left(Unit::Grapheme, false);
    assert_eq!(caret(&state), "<node Rule>");
    state.move_left(Unit::Grapheme, false);
    assert_eq!(caret(&state), "ab‸");
    // Extending skips node blocks.
    state.move_right(Unit::Grapheme, true);
    assert_eq!(caret(&state), "‸cd");
    assert_eq!(state.selected_text(), "\n");
    state.move_to_doc_start(false);
    assert_eq!(caret(&state), "‸ab");
    state.move_to_doc_end(true);
    assert_eq!(state.selected_text(), "ab\ncd");
}

#[test]
fn word_movement() {
    let mut state = ed("‸hello, big world\n");
    state.move_right(Unit::Word, false);
    assert_eq!(caret(&state), "hello‸, big world");
    state.move_right(Unit::Word, false);
    assert_eq!(caret(&state), "hello, big‸ world");
    state.move_left(Unit::Word, true);
    assert_eq!(caret(&state), "hello, ‸big« world");
    state.move_left(Unit::Grapheme, false);
    assert_eq!(caret(&state), "hello, ‸big world");
}

#[test]
fn select_all_selects_the_node_then_the_document() {
    let mut state = ed("a\n\nb‸c\n\nd\n");
    state.select_all();
    assert_eq!(caret(&state), "«bc‸");
    state.select_all();
    assert_eq!(state.selected_text(), "a\nbc\nd");
}

#[test]
fn an_empty_document_stays_empty_until_typed_into() {
    let mut state = ed("");
    assert_eq!(state.markdown(), "");
    type_str(&mut state, &mut Clock::new(), "hi");
    assert_eq!(md(&state), "hi\n");
    state.undo();
    assert_eq!(state.markdown(), "");
}

#[test]
fn typing_with_a_node_selected_starts_a_paragraph_below() {
    let mut state = ed("---\n");
    assert_eq!(caret(&state), "<node Rule>");
    type_str(&mut state, &mut Clock::new(), "x");
    assert_eq!(md(&state), "---\n\nx\n");
}

#[test]
fn plain_text_with_lines_becomes_paragraphs() {
    let mut state = ed("- ab‸cd\n");
    state.insert_plain_text("one\ntwo\nthree");
    assert_eq!(md(&state), "- abone\n\ntwo\n\nthreecd\n");
    assert_eq!(caret(&state), "three‸cd");
    let mut state = ed("```\na‸\n```\n");
    state.insert_plain_text("x\ny");
    assert_eq!(md(&state), "```\nax\ny\n```\n");
}
