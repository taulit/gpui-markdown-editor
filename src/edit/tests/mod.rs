//! Tests of the editing core.
//!
//! Documents are written as Markdown with markers for the selection: `‸` is
//! the caret (the head of a selection) and `«` the anchor of a range. The
//! markers must stand inside text, not in front of block syntax: `- a‸b`,
//! `- ‸` for an empty item.

mod blocks;
mod clipboard;
mod history;
mod ime;
mod marks;
mod order_perf;
mod properties;
mod rules;
mod text;

use std::time::{Duration, Instant};

use super::order::{self, Loc};
use super::{EditorState, Selection, Unit, inline_ops};
use crate::extension::Extensions;
use crate::extensions::{Highlight, Tag, WikiLink};
use crate::model::{Block, BlockKind, Document, TextPos};

pub(super) const CARET: char = '‸';
pub(super) const ANCHOR: char = '«';

pub(super) fn extensions() -> Extensions {
    Extensions::new(vec![Box::new(WikiLink), Box::new(Tag), Box::new(Highlight)])
}

/// The position of each marker: text stop ordinal and byte offset in the
/// text without markers.
fn find_markers(doc: &Document) -> (Option<(usize, usize)>, Option<(usize, usize)>) {
    let mut caret = None;
    let mut anchor = None;
    let mut ordinal = 0;
    let mut stop = order::first_stop(doc);
    while let Some(current) = stop {
        if let Some(inline) = order::inline_at(doc, &current.loc) {
            let mut removed = 0;
            for (index, ch) in inline.text.char_indices() {
                if ch == CARET {
                    caret = Some((ordinal, index - removed));
                    removed += ch.len_utf8();
                } else if ch == ANCHOR {
                    anchor = Some((ordinal, index - removed));
                    removed += ch.len_utf8();
                }
            }
        }
        ordinal += 1;
        stop = order::next_stop(doc, &current.loc);
    }
    (caret, anchor)
}

fn pos_at(doc: &Document, ordinal: usize, offset: usize) -> TextPos {
    let mut stop = order::first_stop(doc);
    for _ in 0..ordinal {
        stop = stop.and_then(|s| order::next_stop(doc, &s.loc));
    }
    let stop = stop.expect("marker in a stop");
    TextPos {
        block: stop.id,
        offset,
    }
}

/// An editor state for Markdown with selection markers.
pub(super) fn ed(marked: &str) -> EditorState {
    let clean: String = marked.chars().filter(|c| *c != CARET && *c != ANCHOR).collect();
    let with_markers = Document::parse_with(marked, extensions());
    let (caret, anchor) = find_markers(&with_markers);
    let mut state = EditorState::new(Document::parse_with(&clean, extensions()));
    if let Some((ordinal, offset)) = caret {
        let head = pos_at(&state.doc, ordinal, offset);
        let anchor = anchor.map_or(head, |(o, off)| pos_at(&state.doc, o, off));
        assert!(state.set_selection(Selection::Text { anchor, head }) || state.selection == Selection::Text { anchor, head });
        assert_eq!(state.selection, Selection::Text { anchor, head }, "markers in {marked:?}");
    }
    state.take_changes();
    state
}

/// The text of the caret's node with the markers inserted.
pub(super) fn caret(state: &EditorState) -> String {
    match &state.selection {
        Selection::Node(id) => {
            let kind = state
                .locate(*id)
                .and_then(|loc| order::block_at(&state.doc.blocks, loc.block_path()))
                .map(|b| format!("{:?}", b.kind))
                .unwrap_or_default();
            format!("<node {kind}>")
        }
        Selection::Text { anchor, head } => {
            let text = state.text_of(head.block).expect("caret node").text.clone();
            let mut marks = vec![(head.offset, CARET)];
            if anchor.block == head.block && anchor != head {
                marks.push((anchor.offset, ANCHOR));
            }
            marks.sort_by(|a, b| b.0.cmp(&a.0));
            let mut out = text;
            for (offset, mark) in marks {
                out.insert(offset, mark);
            }
            out
        }
    }
}

/// A clock for typing: every call is `step` later than the one before.
pub(super) struct Clock {
    now: Instant,
}

impl Clock {
    pub(super) fn new() -> Self {
        Self {
            now: Instant::now(),
        }
    }

    pub(super) fn tick(&mut self, millis: u64) -> Instant {
        self.now += Duration::from_millis(millis);
        self.now
    }

    pub(super) fn now(&mut self) -> Instant {
        self.tick(10)
    }
}

/// Types `text` one character at a time, 10 ms apart.
pub(super) fn type_str(state: &mut EditorState, clock: &mut Clock, text: &str) {
    for ch in text.chars() {
        let now = clock.now();
        match ch {
            '\n' => {
                state.enter(now);
            }
            ch => {
                state.insert_text(ch.encode_utf8(&mut [0; 4]), now);
            }
        }
    }
}

pub(super) fn typed(marked: &str, text: &str) -> EditorState {
    let mut state = ed(marked);
    type_str(&mut state, &mut Clock::new(), text);
    state
}

pub(super) fn backspace(state: &mut EditorState) -> bool {
    state.delete_backward(Unit::Grapheme, Instant::now())
}

pub(super) fn delete(state: &mut EditorState) -> bool {
    state.delete_forward(Unit::Grapheme, Instant::now())
}

pub(super) fn enter(state: &mut EditorState) -> bool {
    state.enter(Instant::now())
}

/// Checks the invariants that must hold after every edit.
pub(super) fn check_invariants(state: &EditorState) -> Result<(), String> {
    fn walk(blocks: &[Block], seen: &mut Vec<crate::model::BlockId>) -> Result<(), String> {
        let mut previous: Option<u32> = None;
        for block in blocks {
            if seen.contains(&block.id) {
                return Err(format!("duplicate id {:?}", block.id));
            }
            seen.push(block.id);
            inline_ops::check(&block.content)?;
            if !block.children.is_empty() && !order::holds_children(&block.kind) {
                return Err(format!("{:?} has children", block.kind));
            }
            if !order::has_text(&block.kind) && !block.content.text.is_empty() {
                return Err(format!("{:?} has text", block.kind));
            }
            match block.kind {
                BlockKind::OrderedItem { number } => {
                    if let Some(prev) = previous
                        && prev.checked_add(1) != Some(number)
                    {
                        return Err(format!("numbering {prev} then {number}"));
                    }
                    previous = Some(number);
                }
                _ => previous = None,
            }
            if let BlockKind::Table(data) = &block.kind {
                for row in &data.rows {
                    if row.len() != data.align.len() {
                        return Err("ragged table".into());
                    }
                    for cell in row {
                        if seen.contains(&cell.id) {
                            return Err(format!("duplicate id {:?}", cell.id));
                        }
                        seen.push(cell.id);
                        inline_ops::check(&cell.content)?;
                    }
                }
            }
            walk(&block.children, seen)?;
        }
        Ok(())
    }
    walk(&state.doc.blocks, &mut Vec::new())?;
    if state.doc.blocks.is_empty() {
        return Err("no blocks".into());
    }
    match &state.selection {
        Selection::Text { anchor, head } => {
            for pos in [anchor, head] {
                let inline = state
                    .text_of(pos.block)
                    .ok_or_else(|| format!("selection in missing node {pos:?}"))?;
                if !inline.text.is_char_boundary(pos.offset) {
                    return Err(format!("selection off boundary {pos:?} in {:?}", inline.text));
                }
            }
        }
        Selection::Node(id) => {
            let loc = state.locate(*id).ok_or("selected node missing")?;
            let Loc::Block(path) = loc else {
                return Err("cell selected as node".into());
            };
            let block = order::block_at(&state.doc.blocks, &path).ok_or("no block")?;
            if order::has_text(&block.kind) {
                return Err("text block selected as node".into());
            }
        }
    }
    if let Some((node, range)) = &state.marked {
        let inline = state.text_of(*node).ok_or("marked node missing")?;
        if range.end > inline.text.len() {
            return Err("marked range outside text".into());
        }
    }
    Ok(())
}
