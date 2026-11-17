//! Input rules and undoing them.

use super::*;

fn md(state: &EditorState) -> String {
    check_invariants(state).unwrap();
    state.markdown()
}

/// Types `trigger` into an empty paragraph, then `text`; checks the result,
/// that Backspace right after the rule gives back the literal trigger, and
/// that Undo does the same.
fn block_rule(trigger: &str, text: &str, expected: &str) {
    let source = "Intro\n\nx‸\n";
    let mut state = ed(source);
    backspace(&mut state);
    let mut clock = Clock::new();
    type_str(&mut state, &mut clock, trigger);
    type_str(&mut state, &mut clock, text);
    assert_eq!(md(&state), format!("Intro\n\n{expected}"), "rule {trigger:?}");

    // Backspace directly after the rule restores the literal text.
    let mut state = ed(source);
    backspace(&mut state);
    type_str(&mut state, &mut clock, trigger);
    assert!(backspace(&mut state));
    let literal = state.doc.blocks[1].content.text.clone();
    assert_eq!(literal, trigger, "backspace after {trigger:?}");
    assert_eq!(state.doc.blocks[1].kind, BlockKind::Paragraph);

    // Undo does the same, and a second undo removes the typing.
    let mut state = ed(source);
    backspace(&mut state);
    type_str(&mut state, &mut Clock::new(), trigger);
    assert!(state.undo());
    assert_eq!(state.doc.blocks[1].content.text, trigger, "undo after {trigger:?}");
    while state.undo() {}
    assert_eq!(state.markdown(), "Intro\n\nx\n");
}

#[test]
fn heading_rules() {
    block_rule("# ", "Title", "# Title\n");
    block_rule("## ", "Title", "## Title\n");
    block_rule("### ", "Title", "### Title\n");
}

#[test]
fn list_rules() {
    block_rule("- ", "item", "- item\n");
    block_rule("* ", "item", "- item\n");
    block_rule("1. ", "item", "1. item\n");
    block_rule("3. ", "item", "3. item\n");
}

#[test]
fn task_rules() {
    block_rule("[] ", "task", "- [ ] task\n");
    block_rule("[ ] ", "task", "- [ ] task\n");
    // `- ` makes a bullet first, `[ ] ` inside it a task.
    let state = typed("Intro\n\nx‸\n", "");
    let mut state = state;
    backspace(&mut state);
    type_str(&mut state, &mut Clock::new(), "- [ ] task");
    assert_eq!(md(&state), "Intro\n\n- [ ] task\n");
    state.undo();
    state.undo();
    assert_eq!(state.doc.blocks[1].kind, BlockKind::BulletItem);
    assert_eq!(state.doc.blocks[1].content.text, "[ ] ");
}

#[test]
fn quote_code_and_rule_rules() {
    block_rule("> ", "said", "> said\n");
    block_rule("```", "code", "```\ncode\n```\n");
    block_rule("---", "", "---\n\n\n");
}

#[test]
fn rules_only_fire_at_the_start_of_a_paragraph() {
    let state = typed("a‸\n", " # b");
    assert_eq!(md(&state), "a # b\n");
    let state = typed("## ‸\n", "- x");
    assert_eq!(state.doc.blocks[0].kind, BlockKind::Heading(2));
}

#[test]
fn rules_can_be_disabled() {
    let mut state = ed("x‸\n");
    state.input_rules = false;
    backspace(&mut state);
    type_str(&mut state, &mut Clock::new(), "# **a**");
    assert_eq!(state.doc.blocks[0].kind, BlockKind::Paragraph);
    assert_eq!(state.doc.blocks[0].content.text, "# **a**");
}

fn inline_rule(typed_text: &str, expected: &str, literal: &str) {
    let state = typed("x‸\n", typed_text);
    assert_eq!(md(&state), format!("x{expected}\n"), "rule {typed_text:?}");
    let mut state = typed("x‸\n", typed_text);
    assert!(backspace(&mut state));
    assert_eq!(state.doc.blocks[0].content.text, format!("x{literal}"));
    assert!(state.doc.blocks[0].content.marks.is_empty());
}

#[test]
fn inline_mark_rules() {
    inline_rule(" **b**", " **b**", " **b**");
    inline_rule(" *i*", " *i*", " *i*");
    inline_rule(" ~~s~~", " ~~s~~", " ~~s~~");
    inline_rule(" `c`", " `c`", " `c`");
    inline_rule(" ==h==", " ==h==", " ==h==");
    let state = typed("x‸\n", " **b** after");
    assert_eq!(md(&state), "x **b** after\n");
    let state = typed("x‸\n", " *i* after");
    assert_eq!(md(&state), "x *i* after\n");
}

#[test]
fn inline_rules_need_text_without_outer_spaces() {
    let state = typed("x‸\n", " ** b**");
    assert!(state.doc.blocks[0].content.marks.is_empty());
    let state = typed("x‸\n", " 2 * 3 * 4");
    assert!(state.doc.blocks[0].content.marks.is_empty());
}

#[test]
fn no_rules_in_code() {
    let state = typed("```\nx‸\n```\n", " **b**");
    assert_eq!(state.doc.blocks[0].content.text, "x **b**");
    let state = typed("a `x‸y` b\n", "**b**");
    assert_eq!(state.doc.blocks[0].content.text, "a x**b**y b");
}
