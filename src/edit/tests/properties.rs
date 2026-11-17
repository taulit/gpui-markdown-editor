//! Property tests: random keystrokes and commands on random small documents.
//!
//! After every step the model invariants hold. Undoing everything gives back
//! the original model and byte-identical Markdown, redoing it all gives back
//! the edited state, and the normalized Markdown of the edited document is
//! stable under parsing. Empty paragraphs are the only part of the model
//! the Markdown leaves out.

use std::time::{Duration, Instant};

use proptest::prelude::*;

use super::*;
use crate::command::Command;
use crate::model::Mark;

#[rustfmt::skip]
const BLOCKS: &[&str] = &[
    "plain text", "**bold** and *it*", "[link](/u) x", "# Head", "## Sub", "- item", "- [ ] task",
    "- [x] done", "1. one", "2. two", "  - nested", "   1. deep", "> quote", "```\ncode\nmore\n```",
    "---", "| a | b |\n|---|--:|\n| c | d |", "![img](i.png)", "word `code` ==hi==", "<div>raw</div>",
    "ä 日本 🎉", "* star", "a  \nb", "[[Note]] #tag",
];

const CHARS: &[&str] = &[
    "a", "b", " ", "*", "`", "#", "-", "[", "]", ">", "1", ".", "é", "🎉", "=", "~", "x",
];

const MARKDOWN: &[&str] = &[
    "**x**",
    "one\n\ntwo",
    "# h\n\n- a\n- b",
    "| q |\n|---|\n| r |",
    "- [ ] t",
    "> z",
];

#[derive(Clone, Debug)]
enum Op {
    Type(usize),
    Backspace,
    BackspaceWord,
    Delete,
    Enter,
    SoftBreak,
    Tab,
    ShiftTab,
    Left(bool),
    Right(bool),
    WordLeft(bool),
    WordRight(bool),
    DocStart,
    DocEnd(bool),
    SelectAll,
    Select(usize, usize, usize, usize),
    Mark(usize),
    Block(usize),
    Command(usize),
    Markdown(usize),
    Plain,
    ImeMark(usize),
    ImeCommit,
    Undo,
    Redo,
    Pause,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        8 => (0..CHARS.len()).prop_map(Op::Type),
        2 => Just(Op::Backspace),
        1 => Just(Op::BackspaceWord),
        2 => Just(Op::Delete),
        2 => Just(Op::Enter),
        1 => Just(Op::SoftBreak),
        1 => Just(Op::Tab),
        1 => Just(Op::ShiftTab),
        1 => any::<bool>().prop_map(Op::Left),
        1 => any::<bool>().prop_map(Op::Right),
        1 => any::<bool>().prop_map(Op::WordLeft),
        1 => any::<bool>().prop_map(Op::WordRight),
        1 => Just(Op::DocStart),
        1 => any::<bool>().prop_map(Op::DocEnd),
        1 => Just(Op::SelectAll),
        3 => (0..40usize, 0..40usize, 0..40usize, 0..40usize).prop_map(|(a, b, c, d)| Op::Select(a, b, c, d)),
        2 => (0..7usize).prop_map(Op::Mark),
        2 => (0..8usize).prop_map(Op::Block),
        3 => (0..14usize).prop_map(Op::Command),
        1 => (0..MARKDOWN.len()).prop_map(Op::Markdown),
        1 => Just(Op::Plain),
        1 => (0..3usize).prop_map(Op::ImeMark),
        1 => Just(Op::ImeCommit),
        3 => Just(Op::Undo),
        1 => Just(Op::Redo),
        1 => Just(Op::Pause),
    ]
}

fn document() -> impl Strategy<Value = String> {
    prop::collection::vec((0..BLOCKS.len(), any::<bool>()), 0..7).prop_map(|picks| {
        let mut out = String::new();
        for (index, tight) in picks {
            out.push_str(BLOCKS[index]);
            out.push_str(if tight { "\n" } else { "\n\n" });
        }
        out
    })
}

fn stops(state: &EditorState) -> Vec<order::Stop> {
    let mut out = Vec::new();
    let mut stop = order::first_stop(&state.doc);
    while let Some(current) = stop {
        stop = order::next_stop(&state.doc, &current.loc);
        if current.text {
            out.push(current);
        }
    }
    out
}

fn pos(state: &EditorState, stop: usize, offset: usize) -> Option<TextPos> {
    let stops = stops(state);
    let stop = stops.get(stop % stops.len().max(1))?;
    let text = &order::inline_at(&state.doc, &stop.loc)?.text;
    Some(TextPos {
        block: stop.id,
        offset: inline_ops::floor_boundary(text, offset % (text.len() + 1)),
    })
}

fn run(state: &mut EditorState, op: &Op, now: &mut Instant) {
    *now += Duration::from_millis(30);
    let t = *now;
    match op {
        Op::Type(index) => {
            state.insert_text(CHARS[*index], t);
        }
        Op::Backspace => {
            state.delete_backward(Unit::Grapheme, t);
        }
        Op::BackspaceWord => {
            state.delete_backward(Unit::Word, t);
        }
        Op::Delete => {
            state.delete_forward(Unit::Grapheme, t);
        }
        Op::Enter => {
            state.enter(t);
        }
        Op::SoftBreak => {
            state.soft_break(t);
        }
        Op::Tab => {
            state.tab(t);
        }
        Op::ShiftTab => {
            state.shift_tab();
        }
        Op::Left(extend) => {
            state.move_left(Unit::Grapheme, *extend);
        }
        Op::Right(extend) => {
            state.move_right(Unit::Grapheme, *extend);
        }
        Op::WordLeft(extend) => {
            state.move_left(Unit::Word, *extend);
        }
        Op::WordRight(extend) => {
            state.move_right(Unit::Word, *extend);
        }
        Op::DocStart => {
            state.move_to_doc_start(false);
        }
        Op::DocEnd(extend) => {
            state.move_to_doc_end(*extend);
        }
        Op::SelectAll => {
            state.select_all();
        }
        Op::Select(a, b, c, d) => {
            if let (Some(anchor), Some(head)) = (pos(state, *a, *b), pos(state, *c, *d)) {
                state.set_selection(Selection::Text { anchor, head });
            }
        }
        Op::Mark(index) => {
            let mark = [
                Mark::Bold,
                Mark::Italic,
                Mark::Strike,
                Mark::Code,
                Mark::Underline,
                Mark::Highlight,
                Mark::Link {
                    url: "/x".into(),
                    title: None,
                },
            ][*index]
                .clone();
            state.exec(&Command::ToggleMark(mark), t);
        }
        Op::Block(index) => {
            let kind = [
                BlockKind::Paragraph,
                BlockKind::Heading(1),
                BlockKind::Heading(3),
                BlockKind::BulletItem,
                BlockKind::OrderedItem { number: 1 },
                BlockKind::TaskItem { checked: false },
                BlockKind::Quote,
                BlockKind::CodeBlock { lang: None },
            ][*index]
                .clone();
            state.exec(&Command::SetBlock(kind), t);
        }
        Op::Command(index) => {
            let command = match index {
                0 => Command::Indent,
                1 => Command::Outdent,
                2 => Command::ToggleTask,
                3 => Command::CheckAll,
                4 => Command::UncheckAll,
                5 => Command::MoveCheckedToBottom,
                6 => Command::MoveBlockUp,
                7 => Command::MoveBlockDown,
                8 => Command::InsertRule,
                9 => Command::InsertTable { rows: 2, cols: 2 },
                10 => Command::InsertImage {
                    src: "p.png".into(),
                    alt: "p".into(),
                },
                11 => Command::SetLink(Some("/l".into())),
                12 => Command::SetLink(None),
                _ => Command::SelectAll,
            };
            state.exec(&command, t);
        }
        Op::Markdown(index) => {
            state.insert_markdown(MARKDOWN[*index]);
        }
        Op::Plain => {
            state.insert_plain_text("p\nq");
        }
        Op::ImeMark(len) => {
            let text = &"ｎｉｈ"[..len * 3];
            state.replace_and_mark_utf16(None, text, None, t);
        }
        Op::ImeCommit => {
            state.replace_text_in_range_utf16(None, "你", t);
        }
        Op::Undo => {
            state.undo();
        }
        Op::Redo => {
            state.redo();
        }
        Op::Pause => {
            *now += Duration::from_secs(1);
        }
    }
}

/// `false` if a formatting span starts or ends next to punctuation, which the
/// writer cannot always express with `*` delimiters (`a**\***` does not open
/// emphasis). The Markdown property tests leave such text out as well.
fn writer_safe(blocks: &[Block]) -> bool {
    fn inline_safe(inline: &crate::model::Inline) -> bool {
        let text = &inline.text;
        let punct = |ch: Option<char>| ch.is_some_and(|c| !c.is_alphanumeric());
        // Delimiters of two spans side by side count as punctuation as well.
        let boundaries: Vec<usize> = inline
            .marks
            .iter()
            .filter(|span| inline_ops::is_formatting(&span.mark))
            .flat_map(|span| [span.range.start, span.range.end])
            .filter(|at| *at > 0 && *at < text.len())
            .collect();
        let shared = boundaries
            .iter()
            .enumerate()
            .any(|(i, a)| boundaries[i + 1..].contains(a));
        // A line break inside a link is written in ways that do not read
        // back the same (see the report of the editing core).
        let break_in_link = inline.marks.iter().any(|span| {
            matches!(span.mark, Mark::Link { .. }) && text[span.range.clone()].contains('\n')
        });
        !shared
            && !break_in_link
            && inline.marks.iter().all(|span| {
                if !inline_ops::is_formatting(&span.mark) || matches!(span.mark, Mark::Code) {
                    return true;
                }
                let inside_start = text[span.range.start..].chars().next();
                let inside_end = text[..span.range.end].chars().next_back();
                let before = text[..span.range.start].chars().next_back();
                let after = text[span.range.end..].chars().next();
                !(punct(inside_start)
                    || punct(inside_end)
                    || punct(before) && before != Some(' ')
                    || punct(after) && after != Some(' '))
            })
    }
    blocks.iter().all(|block| {
        // An item or quote without text whose first child is a paragraph
        // reads back with that paragraph as its text.
        let lifts_child = block.content.text.is_empty()
            && block
                .children
                .first()
                .is_some_and(|c| c.kind == BlockKind::Paragraph);
        // Line breaks at the edges of a block have no Markdown form.
        let edge_break = !order::is_literal(&block.kind)
            && (block.content.text.starts_with('\n') || block.content.text.ends_with('\n'));
        !lifts_child
            && !edge_break
            && inline_safe(&block.content)
            && match &block.kind {
                BlockKind::Table(data) => {
                    data.rows.iter().flatten().all(|c| inline_safe(&c.content))
                }
                _ => true,
            }
            && writer_safe(&block.children)
    })
}

/// Leaves out what the stability check does not cover: raw blocks (their
/// text is source the user edits as such) and paragraphs of nothing but
/// whitespace. Empty paragraphs stay: the serializer leaves them out.
fn drop_unchecked(blocks: &mut Vec<Block>) {
    blocks.retain(|b| {
        !(b.kind == BlockKind::Raw
            || b.kind == BlockKind::Paragraph
                && !b.content.text.is_empty()
                && b.content.text.trim().is_empty())
    });
    for block in blocks {
        drop_unchecked(&mut block.children);
    }
}

/// Removes the empty paragraphs, which exist only in the editor.
fn drop_empty_paragraphs(blocks: &mut Vec<Block>) {
    blocks.retain(|b| {
        !(b.kind == BlockKind::Paragraph && b.content.text.is_empty() && b.children.is_empty())
    });
    for block in blocks {
        drop_empty_paragraphs(&mut block.children);
    }
}

fn cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(512)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(cases()))]

    #[test]
    fn edits_keep_invariants_and_undo_restores_everything(
        source in document(),
        ops in prop::collection::vec(op(), 1..40),
    ) {
        let mut state = EditorState::new(Document::parse_with(&source, extensions()));
        let original = state.doc.blocks.clone();
        let original_markdown = state.markdown();
        prop_assert_eq!(&original_markdown, &source);
        let mut now = Instant::now();
        for op in &ops {
            run(&mut state, op, &mut now);
            if let Err(error) = check_invariants(&state) {
                return Err(TestCaseError::fail(format!("after {op:?}: {error}\n{:#?}", state.doc.blocks)));
            }
        }
        let edited = state.doc.blocks.clone();
        let edited_markdown = state.markdown();

        // The file is the model without its empty paragraphs: leaving them
        // out of the model changes neither the saved nor the normalized text.
        let mut without_empty = state.doc.clone();
        drop_empty_paragraphs(&mut without_empty.blocks);
        // (A document of nothing but empty paragraphs keeps its final line
        // break, an empty one has none.)
        if !without_empty.blocks.is_empty() {
            prop_assert_eq!(&without_empty.to_markdown(), &edited_markdown);
            prop_assert_eq!(&without_empty.normalized(), &state.doc.normalized());
        }

        let mut content = state.doc.clone();
        drop_unchecked(&mut content.blocks);
        if writer_safe(&content.blocks) {
            let normalized = content.normalized();
            let again = Document::parse_with(&normalized, extensions()).normalized();
            prop_assert_eq!(&again, &normalized, "normalized form is not stable");
        }

        let mut undone = 0;
        while state.undo() {
            undone += 1;
            check_invariants(&state).map_err(TestCaseError::fail)?;
        }
        prop_assert_eq!(&state.markdown(), &original_markdown);
        prop_assert_eq!(&state.doc.blocks, &original);

        for _ in 0..undone {
            prop_assert!(state.redo());
        }
        prop_assert_eq!(&state.doc.blocks, &edited);
        prop_assert_eq!(&state.markdown(), &edited_markdown);
    }
}

/// Cases the property test found in the serializer.
#[test]
fn serializer_regressions() {
    let stable = |state: &EditorState| {
        let normalized = state.doc.normalized();
        assert_eq!(
            Document::parse_with(&normalized, extensions()).normalized(),
            normalized
        );
    };
    // A `#` behind a line break in a heading is not a closing sequence.
    let mut state = ed("x‸\n");
    backspace(&mut state);
    state.set_block(BlockKind::Heading(3));
    state.soft_break(Instant::now());
    state.insert_text("#", Instant::now());
    stable(&state);
    // An item with nothing but a line break below a paragraph line is not
    // a setext underline.
    let mut state = ed("- a\n  - b‸\n");
    state.select_all();
    state.soft_break(Instant::now());
    state.enter(Instant::now());
    state.insert_text("c", Instant::now());
    stable(&state);
}
