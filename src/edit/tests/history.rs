//! Undo, redo and grouping, and that undo restores the source byte for byte.

use super::*;

#[test]
fn typing_is_grouped_by_time_and_words() {
    let mut state = ed("x‸\n\n# keep\n");
    let mut clock = Clock::new();
    type_str(&mut state, &mut clock, "hello world");
    assert_eq!(state.doc.blocks[0].content.text, "xhello world");
    state.undo();
    assert_eq!(state.doc.blocks[0].content.text, "xhello ");
    state.undo();
    assert_eq!(state.doc.blocks[0].content.text, "x");
    assert!(!state.can_undo());
    state.redo();
    state.redo();
    assert_eq!(state.doc.blocks[0].content.text, "xhello world");
    assert_eq!(caret(&state), "xhello world‸");
}

#[test]
fn a_pause_starts_a_new_entry() {
    let mut state = ed("x‸\n");
    let mut clock = Clock::new();
    type_str(&mut state, &mut clock, "ab");
    clock.tick(600);
    type_str(&mut state, &mut clock, "cd");
    state.undo();
    assert_eq!(state.doc.blocks[0].content.text, "xab");
}

#[test]
fn moving_the_caret_starts_a_new_entry() {
    let mut state = ed("x‸\n");
    let mut clock = Clock::new();
    type_str(&mut state, &mut clock, "ab");
    state.move_left(Unit::Grapheme, false);
    type_str(&mut state, &mut clock, "c");
    state.undo();
    assert_eq!(state.doc.blocks[0].content.text, "xab");
}

#[test]
fn backspaces_are_grouped() {
    let mut state = ed("hello‸\n");
    let mut clock = Clock::new();
    for _ in 0..3 {
        state.delete_backward(Unit::Grapheme, clock.now());
    }
    assert_eq!(state.doc.blocks[0].content.text, "he");
    state.undo();
    assert_eq!(state.doc.blocks[0].content.text, "hello");
}

#[test]
fn undoing_everything_restores_the_source_byte_for_byte() {
    let source = "Title\n=====\n\n* one\n*   two\n\n1) x\n\n| a |  b |\n|---|:-:|\n| 1 | 2 |\n\n> quote\n> more\n";
    let mut state = ed(source);
    let original = state.doc.blocks.clone();
    let mut clock = Clock::new();
    state.move_to_doc_start(false);
    type_str(&mut state, &mut clock, "New ");
    state.move_to_doc_end(false);
    type_str(&mut state, &mut clock, "\nend");
    state.select_all();
    state.select_all();
    state.toggle_mark(crate::model::Mark::Bold);
    state.move_to_doc_start(false);
    state.set_block(BlockKind::Heading(2));
    assert_ne!(state.markdown(), source);
    while state.undo() {}
    assert_eq!(state.markdown(), source);
    assert_eq!(state.doc.blocks, original);
}

#[test]
fn a_new_document_clears_the_history() {
    let mut state = typed("a‸\n", "bc");
    state.set_document(Document::parse("x\n"));
    assert!(!state.can_undo());
    assert!(state.take_changes().structure);
}

#[test]
fn changes_report_blocks_and_structure() {
    let mut state = ed("a‸\n\nb\n");
    type_str(&mut state, &mut Clock::new(), "x");
    let changes = state.take_changes();
    assert_eq!(changes.blocks, vec![state.doc.blocks[0].id]);
    assert!(!changes.structure && changes.content && changes.selection);
    enter(&mut state);
    assert!(state.take_changes().structure);
    state.move_left(Unit::Grapheme, false);
    let changes = state.take_changes();
    assert!(changes.selection && !changes.content);
}
