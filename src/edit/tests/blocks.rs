//! Block commands.

use std::time::Instant;

use super::*;
use crate::command::Command;
use crate::model::Align;

fn md(state: &EditorState) -> String {
    check_invariants(state).unwrap();
    state.markdown()
}

fn exec(state: &mut EditorState, command: Command) -> bool {
    state.exec(&command, Instant::now())
}

#[test]
fn set_block_converts_and_toggles() {
    let mut state = ed("one‸\n");
    assert!(exec(&mut state, Command::SetBlock(BlockKind::Heading(2))));
    assert_eq!(md(&state), "## one\n");
    exec(&mut state, Command::SetBlock(BlockKind::Heading(2)));
    assert_eq!(md(&state), "one\n");
}

#[test]
fn set_block_on_several_blocks() {
    let mut state = ed("«a\n\nb\n\nc‸\n");
    exec(&mut state, Command::SetBlock(BlockKind::TaskItem { checked: false }));
    // The blank lines of the source stay: a loose list.
    assert_eq!(md(&state), "- [ ] a\n\n- [ ] b\n\n- [ ] c\n");
    let mut state = ed("«a\nb\n\nc‸\n");
    exec(&mut state, Command::SetBlock(BlockKind::OrderedItem { number: 1 }));
    assert_eq!(md(&state), "1. a b\n\n2. c\n");
}

#[test]
fn set_block_lifts_children_and_strips_marks_for_code() {
    let mut state = ed("- **a‸**\n  - b\n");
    exec(&mut state, Command::SetBlock(BlockKind::CodeBlock { lang: None }));
    assert_eq!(md(&state), "```\na\n```\n\n- b\n");
}

#[test]
fn toggle_task_and_check_all() {
    let mut state = ed("- [ ] a‸\n- [x] b\n- [ ] c\n");
    exec(&mut state, Command::ToggleTask);
    assert_eq!(md(&state), "- [x] a\n- [x] b\n- [ ] c\n");
    exec(&mut state, Command::CheckAll);
    assert_eq!(md(&state), "- [x] a\n- [x] b\n- [x] c\n");
    exec(&mut state, Command::UncheckAll);
    assert_eq!(md(&state), "- [ ] a\n- [ ] b\n- [ ] c\n");
    assert!(!exec(&mut state, Command::UncheckAll));
    let mut state = ed("plain‸\n");
    assert!(!exec(&mut state, Command::ToggleTask));
}

#[test]
fn move_checked_to_bottom_works_per_run() {
    let mut state = ed("- [x] a‸\n- [ ] b\n- [x] c\n- [ ] d\n  - [x] e\n  - [ ] f\n\ntext\n\n- [x] g\n- [ ] h\n");
    exec(&mut state, Command::MoveCheckedToBottom);
    assert_eq!(
        md(&state),
        "- [ ] b\n- [ ] d\n  - [ ] f\n  - [x] e\n- [x] a\n- [x] c\n\ntext\n\n- [ ] h\n- [x] g\n"
    );
    assert!(!exec(&mut state, Command::MoveCheckedToBottom));
}

#[test]
fn move_block_up_and_down_with_children() {
    let mut state = ed("- a\n- b‸\n  - c\n- d\n");
    exec(&mut state, Command::MoveBlockUp);
    assert_eq!(md(&state), "- b\n  - c\n- a\n- d\n");
    assert!(!exec(&mut state, Command::MoveBlockUp));
    exec(&mut state, Command::MoveBlockDown);
    exec(&mut state, Command::MoveBlockDown);
    assert_eq!(md(&state), "- a\n- d\n- b\n  - c\n");
    assert_eq!(caret(&state), "b‸");
}

#[test]
fn moving_ordered_items_renumbers() {
    let mut state = ed("1. a\n2. b‸\n3. c\n");
    exec(&mut state, Command::MoveBlockUp);
    assert_eq!(md(&state), "1. b\n2. a\n3. c\n");
}

#[test]
fn insert_rule_image_and_table() {
    let mut state = ed("Intro‸\n\nNext\n");
    exec(&mut state, Command::InsertRule);
    assert_eq!(md(&state), "Intro\n\n---\n\nNext\n");
    assert_eq!(caret(&state), "‸Next");

    let mut state = ed("Intro‸\n");
    exec(
        &mut state,
        Command::InsertImage {
            src: "cat.png".into(),
            alt: "Cat".into(),
        },
    );
    assert_eq!(md(&state), "Intro\n\n![Cat](cat.png)\n\n\n");

    let mut state = ed("Intro‸\n");
    exec(&mut state, Command::InsertTable { rows: 2, cols: 2 });
    type_str(&mut state, &mut Clock::new(), "h");
    assert_eq!(
        md(&state),
        "Intro\n\n| h   |     |\n| --- | --- |\n|     |     |\n\n\n"
    );
}

#[test]
fn table_navigation_and_rows() {
    let mut state = ed("| a‸ | b |\n|---|---|\n| c | d |\n");
    let now = Instant::now();
    state.tab(now);
    assert_eq!(caret(&state), "b‸");
    state.tab(now);
    state.tab(now);
    assert_eq!(caret(&state), "d‸");
    state.tab(now);
    assert_eq!(caret(&state), "‸");
    type_str(&mut state, &mut Clock::new(), "e");
    state.shift_tab();
    assert_eq!(caret(&state), "d‸");
    enter(&mut state);
    assert_eq!(caret(&state), "‸");
    assert_eq!(
        md(&state),
        "| a   | b   |\n| --- | --- |\n| c   | d   |\n| e   |     |\n"
    );
}

#[test]
fn table_columns_rows_and_alignment() {
    let mut state = ed("| a‸ | b |\n|---|---|\n| c | d |\n");
    assert!(state.table_insert_column(true));
    type_str(&mut state, &mut Clock::new(), "x");
    assert!(state.table_set_align(Align::Right));
    assert!(state.table_move_column(true));
    assert!(state.table_move_row(true));
    assert_eq!(
        md(&state),
        "| c   | d   |     |\n| --- | --- | --: |\n| a   | b   |   x |\n"
    );
    assert!(state.table_remove_row());
    assert!(state.table_remove_column());
    assert!(!state.table_remove_row());
    while state.undo() {}
    assert_eq!(state.markdown(), "| a | b |\n|---|---|\n| c | d |\n");
}

#[test]
fn backspace_and_delete_around_tables() {
    let mut state = ed("| a | b |\n|---|---|\n\n‸after\n");
    backspace(&mut state);
    assert!(caret(&state).starts_with("<node Table"));
    backspace(&mut state);
    assert_eq!(md(&state), "after\n");
    let mut state = ed("| ‸a | b |\n|---|---|\n");
    assert!(!backspace(&mut state));
    state.move_to_doc_start(false);
    assert!(caret(&state).starts_with("<node Table"));
}

#[test]
fn indent_needs_an_item_above() {
    let mut state = ed("para\n\n- a‸\n");
    assert!(!state.indent());
    assert!(!state.outdent());
}

#[test]
fn fold_heading_and_open_find_are_left_to_the_view() {
    let mut state = ed("# a‸\n");
    assert!(!exec(&mut state, Command::FoldHeading));
    assert!(!exec(&mut state, Command::OpenFind));
}
