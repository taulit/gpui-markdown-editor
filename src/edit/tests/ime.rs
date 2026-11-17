//! IME composition.

use super::*;

#[test]
fn compose_an_accent_and_commit() {
    let mut state = ed("caf‸\n");
    let mut clock = Clock::new();
    state.replace_and_mark_utf16(None, "´", None, clock.now());
    assert_eq!(state.marked_range_utf16(), Some(3..4));
    state.replace_text_in_range_utf16(None, "é", clock.now());
    assert_eq!(state.marked_range_utf16(), None);
    assert_eq!(caret(&state), "café‸");
    assert_eq!(state.markdown(), "café\n");
    state.undo();
    assert_eq!(state.markdown(), "caf\n");
}

#[test]
fn pinyin_preedit_with_inner_selection() {
    let mut state = ed("‸x\n");
    let mut clock = Clock::new();
    state.replace_and_mark_utf16(None, "ni", Some(2..2), clock.now());
    state.replace_and_mark_utf16(None, "nih", Some(3..3), clock.now());
    assert_eq!(caret(&state), "nih‸x");
    state.replace_and_mark_utf16(None, "你好", Some(1..2), clock.now());
    assert_eq!(caret(&state), "你«好‸x");
    assert_eq!(state.selected_range_utf16(), Some((1..2, false)));
    state.replace_text_in_range_utf16(None, "你好", clock.now());
    assert_eq!(caret(&state), "你好‸x");
    assert_eq!(state.text_for_range_utf16(0..2), Some(("你好".to_string(), 0..2)));
    check_invariants(&state).unwrap();
}

#[test]
fn mark_delete_and_type_again_does_not_panic() {
    let mut state = ed("ab‸\n");
    let mut clock = Clock::new();
    state.replace_and_mark_utf16(None, "xyz", None, clock.now());
    // The marked text disappears behind the IME's back.
    state.select_all();
    backspace(&mut state);
    assert_eq!(state.marked_range_utf16(), None);
    state.replace_and_mark_utf16(None, "q", None, clock.now());
    state.replace_text_in_range_utf16(None, "Q", clock.now());
    assert_eq!(state.markdown(), "Q\n");
    check_invariants(&state).unwrap();

    // A stale range from the IME is clamped.
    let mut state = ed("ab‸\n");
    state.replace_and_mark_utf16(Some(10..20), "z", Some(5..9), clock.now());
    check_invariants(&state).unwrap();
    assert_eq!(state.markdown(), "abz\n");
    state.marked = Some((state.doc.blocks[0].id, 1..99));
    assert_eq!(state.marked_range_utf16(), None);
    state.replace_text_in_range_utf16(Some(7..3), "!", clock.now());
    check_invariants(&state).unwrap();
}

#[test]
fn a_stale_marked_range_in_another_node_is_dropped() {
    let mut state = ed("ab‸\n\ncd\n");
    let mut clock = Clock::new();
    state.replace_and_mark_utf16(None, "x", None, clock.now());
    let last = state.last_pos().unwrap();
    state.put_selection(Selection::caret(last));
    assert_eq!(state.marked_range_utf16(), None);
    state.replace_text_in_range_utf16(None, "y", clock.now());
    assert_eq!(state.markdown(), "abx\n\ncdy\n");
}

#[test]
fn utf16_conversions_use_the_caret_node() {
    let state = ed("😀a‸\n");
    assert_eq!(state.offset_to_utf16(5), 3);
    assert_eq!(state.utf16_to_offset(2), 4);
    assert_eq!(state.utf16_to_offset(1), 0);
}
