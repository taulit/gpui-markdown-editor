//! Typing, input rules, block keys, shortcuts, history and keyboard
//! movement.

use gpui::TestAppContext;

use super::*;
use crate::model::BlockKind;

fn kinds(editor: &Entity<MarkdownEditor>, cx: &mut VisualTestContext) -> Vec<BlockKind> {
    editor.read_with(cx, |editor, _| {
        editor
            .document()
            .blocks()
            .iter()
            .map(|b| b.kind().clone())
            .collect()
    })
}

#[gpui::test]
fn block_input_rules(cx: &mut TestAppContext) {
    let cases: &[(&str, BlockKind, &str)] = &[
        ("# Title", BlockKind::Heading(1), "# Title"),
        ("## Sub", BlockKind::Heading(2), "## Sub"),
        ("### Small", BlockKind::Heading(3), "### Small"),
        ("- item", BlockKind::BulletItem, "- item"),
        ("* item", BlockKind::BulletItem, "- item"),
        ("1. first", BlockKind::OrderedItem { number: 1 }, "1. first"),
        (
            "[] task",
            BlockKind::TaskItem { checked: false },
            "- [ ] task",
        ),
        (
            "[ ] task",
            BlockKind::TaskItem { checked: false },
            "- [ ] task",
        ),
        (
            "- [ ] task",
            BlockKind::TaskItem { checked: false },
            "- [ ] task",
        ),
        ("> quote", BlockKind::Quote, "> quote"),
        ("```", BlockKind::CodeBlock { lang: None }, "```\n```"),
    ];
    for (typed, kind, markdown) in cases {
        let mut cx = cx.new_app();
        let (editor, cx) = open(&mut cx, "");
        cx.simulate_input(typed);
        assert_eq!(kinds(&editor, cx)[0], *kind, "{typed:?}");
        assert_eq!(md(&editor, cx).trim_end(), *markdown, "{typed:?}");
    }

    let (editor, cx) = open(cx, "");
    cx.simulate_input("---");
    assert_eq!(kinds(&editor, cx)[0], BlockKind::Rule);
    assert_eq!(caret(&editor, cx), "‸");
}

#[gpui::test]
fn inline_input_rules(cx: &mut TestAppContext) {
    let cases = [
        ("a **b** c", "a **b** c"),
        ("a *b* c", "a *b* c"),
        ("a ~~b~~ c", "a ~~b~~ c"),
        ("a `b` c", "a `b` c"),
        ("a ==b== c", "a ==b== c"),
    ];
    for (typed, markdown) in cases {
        let mut cx = cx.new_app();
        let (editor, cx) = open(&mut cx, "");
        cx.simulate_input(typed);
        assert_eq!(md(&editor, cx).trim_end(), markdown, "{typed:?}");
        // The delimiters are gone from the text: the mark is real.
        let text = editor.read_with(cx, |e, _| e.document().blocks()[0].text().to_string());
        assert_eq!(text, "a b c", "{typed:?}");
    }
}

#[gpui::test]
fn input_rules_can_be_turned_off(cx: &mut TestAppContext) {
    let options = EditorOptions {
        input_rules: false,
        ..options()
    };
    let (editor, cx) = open_with(cx, "", options);
    cx.simulate_input("# a **x** ==y==");
    assert_eq!(kinds(&editor, cx)[0], BlockKind::Paragraph);
    let block = editor.read_with(cx, |e, _| e.document().blocks()[0].text().to_string());
    assert_eq!(block, "# a **x** ==y==");
    // Written escaped, so the file reads back as the same literal text.
    assert_eq!(md(&editor, cx).trim_end(), r"\# a \*\*x\*\* \==y==");
}

#[gpui::test]
fn an_empty_paragraph_is_an_edit_but_not_written(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "a\n\nb\n");
    press(cx, "end enter");
    assert_eq!(md(&editor, cx), "a\n\nb\n");
    assert!(editor.read_with(cx, |e, _| e.is_dirty()));
}

#[gpui::test]
fn backspace_right_after_a_rule_undoes_it(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "");
    cx.simulate_input("# ");
    assert_eq!(kinds(&editor, cx)[0], BlockKind::Heading(1));
    press(cx, "backspace");
    assert_eq!(kinds(&editor, cx)[0], BlockKind::Paragraph);
    assert_eq!(caret(&editor, cx), "# ‸");
}

#[gpui::test]
fn enter_in_lists_and_checklists(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "");
    type_text(cx, "- one\ntwo\n\nafter");
    assert_eq!(md(&editor, cx), "- one\n- two\n\nafter\n");

    let mut app1 = cx.new_app();
    let (editor, cx) = open(&mut app1, "");
    type_text(cx, "[] milk\nbread\n\nend");
    assert_eq!(md(&editor, cx), "- [ ] milk\n- [ ] bread\n\nend\n");

    // A new item after a ticked one starts unticked.
    let mut app2 = cx.new_app();
    let (editor, cx) = open(&mut app2, "- [x] done\n");
    press(cx, "doc-end");
    type_text(cx, "\nnext");
    assert_eq!(md(&editor, cx), "- [x] done\n- [ ] next\n");

    // Shift+Enter is a line break inside the item.
    let mut app3 = cx.new_app();
    let (editor, cx) = open(&mut app3, "");
    cx.simulate_input("- a");
    press(cx, "shift-enter");
    cx.simulate_input("b");
    assert_eq!(kinds(&editor, cx), [BlockKind::BulletItem]);
    assert_eq!(caret(&editor, cx), "a\nb‸");
}

#[gpui::test]
fn tab_and_shift_tab_indent(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "- one\n- two\n");
    press(cx, "doc-end tab");
    assert_eq!(md(&editor, cx), "- one\n  - two\n");
    press(cx, "shift-tab");
    assert_eq!(md(&editor, cx), "- one\n- two\n");
}

#[gpui::test]
fn mark_shortcuts_toggle_marks(cx: &mut TestAppContext) {
    let cases = [
        ("secondary-b", "say **hello**\n"),
        ("secondary-i", "say *hello*\n"),
        ("secondary-u", "say <u>hello</u>\n"),
        ("secondary-shift-x", "say ~~hello~~\n"),
        ("secondary-e", "say `hello`\n"),
        ("secondary-shift-h", "say ==hello==\n"),
    ];
    for (keys, markdown) in cases {
        let mut cx = cx.new_app();
        let (editor, cx) = open(&mut cx, "say hello\n");
        press(cx, &format!("end word-shift-left {keys}"));
        assert_eq!(md(&editor, cx), markdown, "{keys}");
        // Toggling again removes the mark; the block was edited, so it is
        // written normalized but equal.
        press(cx, keys);
        assert_eq!(md(&editor, cx), "say hello\n", "{keys}");
    }

    // At a caret the mark applies to the text typed next.
    let (editor, cx) = open(cx, "");
    cx.simulate_input("a ");
    press(cx, "secondary-b");
    cx.simulate_input("b");
    press(cx, "secondary-b");
    cx.simulate_input(" c");
    assert_eq!(md(&editor, cx), "a **b** c\n");
}

#[gpui::test]
fn block_shortcuts(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "one\n\ntwo\n");
    press(cx, "secondary-alt-1");
    assert_eq!(md(&editor, cx), "# one\n\ntwo\n");
    press(cx, "secondary-alt-2");
    assert_eq!(md(&editor, cx), "## one\n\ntwo\n");
    press(cx, "secondary-alt-3");
    assert_eq!(md(&editor, cx), "### one\n\ntwo\n");
    press(cx, "secondary-alt-0");
    assert_eq!(md(&editor, cx), "one\n\ntwo\n");

    press(cx, "secondary-shift-l");
    assert_eq!(md(&editor, cx), "- [ ] one\n\ntwo\n");
    press(cx, "secondary-enter");
    assert_eq!(md(&editor, cx), "- [x] one\n\ntwo\n");
    press(cx, "secondary-enter");
    assert_eq!(md(&editor, cx), "- [ ] one\n\ntwo\n");

    press(cx, "alt-down");
    assert_eq!(md(&editor, cx), "two\n\n- [ ] one\n");
    press(cx, "alt-up");
    assert_eq!(md(&editor, cx), "- [ ] one\n\ntwo\n");
}

#[gpui::test]
fn undo_groups_typing_by_word(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "");
    cx.simulate_input("hello world");
    assert_eq!(md(&editor, cx), "hello world\n");
    press(cx, "secondary-z");
    // A trailing space is escaped so that it survives.
    assert_eq!(md(&editor, cx), "hello&#32;\n");
    press(cx, "secondary-z");
    assert_eq!(md(&editor, cx), "");
    press(cx, "secondary-shift-z");
    assert_eq!(md(&editor, cx), "hello&#32;\n");
    press(cx, "secondary-y");
    assert_eq!(md(&editor, cx), "hello world\n");

    // Moving the caret starts a new group.
    press(cx, "left");
    cx.simulate_input("X");
    press(cx, "end");
    cx.simulate_input("Y");
    assert_eq!(md(&editor, cx), "hello worlXdY\n");
    press(cx, "secondary-z");
    assert_eq!(md(&editor, cx), "hello worlXd\n");
}

#[gpui::test]
fn shift_arrows_select_across_blocks(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "ab\n\ncd\n");
    press(cx, "end shift-right shift-right");
    let selected = editor.read_with(cx, |e, _| e.state.selected_text());
    assert_eq!(selected, "\nc");
    cx.simulate_input("X");
    assert_eq!(md(&editor, cx), "abXd\n");

    // Shift+Up from the second block to the first, then Ctrl+A twice.
    let mut app4 = cx.new_app();
    let (editor, cx) = open(&mut app4, "first\n\nsecond\n");
    press(cx, "doc-end shift-up");
    let selected = editor.read_with(cx, |e, _| e.state.selected_text());
    assert_eq!(selected, "\nsecond");
    // Ctrl+A selects the block first, then the document.
    press(cx, "doc-end secondary-a");
    assert_eq!(caret(&editor, cx), "«second‸");
    press(cx, "secondary-a");
    let selected = editor.read_with(cx, |e, _| e.state.selected_text());
    assert_eq!(selected, "first\nsecond");
}

#[gpui::test]
fn word_and_line_movement(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "one two three\n");
    press(cx, "word-right");
    assert_eq!(caret(&editor, cx), "one‸ two three");
    press(cx, "word-right word-shift-right");
    assert_eq!(caret(&editor, cx), "one two« three‸");
    press(cx, "home");
    assert_eq!(caret(&editor, cx), "‸one two three");
    press(cx, "shift-end");
    assert_eq!(caret(&editor, cx), "«one two three‸");
    press(cx, "word-backspace");
    // An edited empty paragraph is an empty line.
    assert_eq!(md(&editor, cx), "\n");

    let mut app5 = cx.new_app();
    let (editor, cx) = open(&mut app5, "one two\n");
    press(cx, "end word-backspace");
    assert_eq!(caret(&editor, cx), "one ‸");
    press(cx, "home word-delete");
    assert_eq!(caret(&editor, cx), "‸ ");
    press(cx, "delete");
    assert_eq!(caret(&editor, cx), "‸");
}

#[gpui::test]
fn up_and_down_across_wrapped_lines_keep_the_goal_column(cx: &mut TestAppContext) {
    let long = "word ".repeat(60);
    let doc = format!("{}\n\nshort\n\n{}\n", long.trim_end(), long.trim_end());
    let (editor, cx) = open(cx, &doc);
    let first = block(&editor, cx, 0);
    let short = block(&editor, cx, 1);
    let last = block(&editor, cx, 2);
    // Start in the middle of the first line.
    select(
        &editor,
        cx,
        TextPos {
            block: first,
            offset: 22,
        },
        TextPos {
            block: first,
            offset: 22,
        },
    );
    let start_x = point_of(
        &editor,
        cx,
        TextPos {
            block: first,
            offset: 22,
        },
    )
    .x;

    // Down moves within the wrapped paragraph first, at the same x.
    press(cx, "down");
    let head = editor.read_with(cx, |e, _| e.state.selection.head().unwrap());
    assert_eq!(head.block, first);
    assert!(head.offset > 40, "second visual line: {head:?}");
    let x = point_of(&editor, cx, head).x;
    assert!((x - start_x).abs() < px(10.), "{x:?} vs {start_x:?}");

    // Through the wrapped lines down to "short", where the caret lands at
    // the end, and on into the last paragraph back at the goal column.
    let mut steps = 0;
    while editor.read_with(cx, |e, _| e.state.selection.head().unwrap().block) == first {
        press(cx, "down");
        steps += 1;
        assert!(steps < 10);
    }
    assert_eq!(caret(&editor, cx), "short‸");
    press(cx, "down");
    let head = editor.read_with(cx, |e, _| e.state.selection.head().unwrap());
    assert_eq!(head.block, last);
    assert_eq!(head.offset, 22);

    press(cx, "up");
    assert_eq!(caret(&editor, cx), "short‸");
    let head = editor.read_with(cx, |e, _| e.state.selection.head().unwrap());
    assert_eq!(head.block, short);

    // Up on the first line goes to the start, down on the last to the end.
    press(cx, "doc-start right up");
    assert_eq!(
        editor.read_with(cx, |e, _| e.state.selection.head().unwrap().offset),
        0
    );
    press(cx, "doc-end left down");
    let head = editor.read_with(cx, |e, _| e.state.selection.head().unwrap());
    assert_eq!(head.offset, long.trim_end().len());

    // Shift+Down extends.
    press(cx, "doc-start shift-down");
    let selected = editor.read_with(cx, |e, _| e.state.selected_text());
    assert!(selected.len() > 40, "{selected:?}");
}

#[gpui::test]
fn page_down_moves_the_caret_by_a_screen(cx: &mut TestAppContext) {
    let doc: String = (0..200).map(|i| format!("line {i}\n\n")).collect();
    let (editor, cx) = open(cx, &doc);
    press(cx, "pagedown");
    let head = editor.read_with(cx, |e, _| e.state.selection.head().unwrap());
    let ix = editor.read_with(cx, |e, _| {
        e.document()
            .blocks()
            .iter()
            .position(|b| b.id() == head.block)
            .unwrap()
    });
    assert!(ix > 5, "moved to block {ix}");
    press(cx, "pageup");
    let head = editor.read_with(cx, |e, _| e.state.selection.head().unwrap());
    assert_eq!(head.block, block(&editor, cx, 0));
}

#[gpui::test]
fn changed_and_dirty(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "text\n");
    let events = record_events(&editor, cx);
    assert!(!editor.read_with(cx, |e, _| e.is_dirty()));

    cx.simulate_input("ab");
    assert_eq!(count(&events, &EditorEvent::Changed), 2);
    assert!(editor.read_with(cx, |e, _| e.is_dirty()));

    // Movement is a selection change only.
    press(cx, "right");
    assert_eq!(count(&events, &EditorEvent::Changed), 2);
    assert!(count(&events, &EditorEvent::SelectionChanged) >= 3);

    editor.update(cx, |e, _| e.mark_saved());
    assert!(!editor.read_with(cx, |e, _| e.is_dirty()));
    // Typing on after saving is a new undo step: undoing it is clean again.
    cx.simulate_input("c");
    assert!(editor.read_with(cx, |e, _| e.is_dirty()));
    press(cx, "secondary-z");
    assert!(!editor.read_with(cx, |e, _| e.is_dirty()));
    assert_eq!(md(&editor, cx), "abtext\n");
    // Undoing past the save point is dirty.
    press(cx, "secondary-z");
    assert!(editor.read_with(cx, |e, _| e.is_dirty()));
    assert_eq!(count(&events, &EditorEvent::Changed), 5);

    // A command that does not apply emits nothing.
    let before = count(&events, &EditorEvent::Changed);
    press(cx, "secondary-enter");
    assert_eq!(count(&events, &EditorEvent::Changed), before);
}

const NOTE: &str = "\
Weekend   trip\r\n\
\r\n\
Leaving __Friday__ at 8, see [[Packing list]] and #travel.\r\n\
\r\n\
* [x] Book the cabin\r\n\
* [ ] Check the tires\r\n\
\r\n\
| Day | Plan |\r\n\
|-----|------|\r\n\
| Sat | Hike |\r\n\
\r\n\
> Don't forget the ==charger==.\r\n";

#[gpui::test]
fn markdown_is_byte_identical_after_opening_and_undoing_everything(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, NOTE);
    assert_eq!(md(&editor, cx), NOTE);

    // Edits all over the place: typing, rules, Enter, a checkbox, marks.
    cx.simulate_input("X");
    press(cx, "down down end");
    type_text(cx, "\nnew item");
    press(cx, "secondary-enter up secondary-alt-1");
    press(cx, "doc-end");
    cx.simulate_input(" **bold**");
    press(cx, "doc-start shift-down secondary-b");
    assert_ne!(md(&editor, cx), NOTE);

    for _ in 0..100 {
        press(cx, "secondary-z");
    }
    assert_eq!(md(&editor, cx), NOTE);
    assert!(!editor.read_with(cx, |e, _| e.is_dirty()));
}
