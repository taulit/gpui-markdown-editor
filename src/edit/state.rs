//! [`EditorState`]: a document with selection, history and the editing
//! commands, independent of any window.

use std::cmp::Ordering;
use std::ops::Range;
use std::time::Instant;

use unicode_segmentation::UnicodeSegmentation;

use super::Selection;
use super::history::{EditKind, History};
use super::ops::{Abort, EditResult};
use super::order::{self, Loc, PathIndex, Stop};
use super::transaction::{self, Applied, Tr};
use crate::command::Command;
use crate::model::{BlockId, BlockKind, Document, Inline, Mark, TextPos};

/// What changed since the view last asked, see [`EditorState::take_changes`].
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Changes {
    /// Top-level blocks whose content or attributes changed.
    pub(crate) blocks: Vec<BlockId>,
    /// Blocks, rows or columns were inserted, removed or moved: relayout
    /// everything.
    pub(crate) structure: bool,
    /// The document changed (emit `Changed`).
    pub(crate) content: bool,
    /// The selection changed (emit `SelectionChanged`).
    pub(crate) selection: bool,
}

/// How far a movement or deletion reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unit {
    /// One user-perceived character.
    Grapheme,
    /// To the next word boundary.
    Word,
}

/// A document being edited.
#[derive(Clone, Debug)]
pub(crate) struct EditorState {
    pub(crate) doc: Document,
    pub(crate) selection: Selection,
    /// Markdown input rules (`# `, `**x**`, ...) are applied while typing.
    pub(crate) input_rules: bool,
    pub(crate) history: History,
    /// Marks for the next typed text, set by toggling a mark at a caret.
    pub(crate) pending_marks: Option<Vec<Mark>>,
    /// The IME preedit range: node and byte range.
    pub(crate) marked: Option<(BlockId, Range<usize>)>,
    pub(crate) index: PathIndex,
    /// The empty paragraph added to a document without blocks, which must
    /// not change its Markdown while it stays empty.
    placeholder: Option<BlockId>,
    /// An input rule just fired: Backspace undoes it.
    pub(crate) rule_armed: bool,
    changes: Changes,
}

impl EditorState {
    pub(crate) fn new(doc: Document) -> Self {
        let mut state = Self {
            doc,
            selection: Selection::Node(BlockId(0)),
            input_rules: true,
            history: History::default(),
            pending_marks: None,
            marked: None,
            index: PathIndex::default(),
            placeholder: None,
            rule_armed: false,
            changes: Changes::default(),
        };
        state.reset_selection();
        // Built up front so the first keystroke does not pay for it.
        state.index.build(&state.doc);
        state
    }

    pub(crate) fn from_markdown(markdown: &str, extensions: crate::extension::Extensions) -> Self {
        Self::new(Document::parse_with(markdown, extensions))
    }

    /// Replaces the document and clears the history.
    pub(crate) fn set_document(&mut self, doc: Document) {
        let input_rules = self.input_rules;
        *self = Self::new(doc);
        self.input_rules = input_rules;
        self.changes = Changes {
            blocks: Vec::new(),
            structure: true,
            content: true,
            selection: true,
        };
    }

    /// Parses `markdown` with the document's extensions and starts over:
    /// new document, caret at the start, empty history.
    pub(crate) fn set_markdown(&mut self, markdown: &str) {
        let extensions = self.doc.extensions.clone();
        self.set_document(Document::parse_with(markdown, extensions));
    }

    fn reset_selection(&mut self) {
        if self.doc.blocks.is_empty() {
            let id = self.doc.alloc_id();
            let mut block = super::ops::new_block(id, BlockKind::Paragraph, Inline::default());
            block.dirty = false;
            self.doc.blocks.push(block);
            self.placeholder = Some(id);
            self.index.invalidate();
        }
        self.selection = match order::first_text(&self.doc) {
            Some(stop) => Selection::caret(TextPos {
                block: stop.id,
                offset: 0,
            }),
            None => Selection::Node(self.doc.blocks.first().map_or(BlockId(0), |b| b.id)),
        };
    }

    /// The Markdown of the document. Prefer this over
    /// [`Document::to_markdown`]: an empty file stays empty.
    pub(crate) fn markdown(&self) -> String {
        if self.is_placeholder_only() {
            return self.doc.source.to_string();
        }
        self.doc.to_markdown()
    }

    fn is_placeholder_only(&self) -> bool {
        match (self.placeholder, self.doc.blocks.as_slice()) {
            (Some(id), [block]) => {
                block.id == id
                    && block.kind == BlockKind::Paragraph
                    && block.content.text.is_empty()
                    && block.children.is_empty()
                    && !block.dirty
            }
            _ => false,
        }
    }

    /// Takes what changed since the last call.
    pub(crate) fn take_changes(&mut self) -> Changes {
        std::mem::take(&mut self.changes)
    }

    #[allow(dead_code)] // for the format bar of the next phase
    pub(crate) fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    #[allow(dead_code)] // for the format bar of the next phase
    pub(crate) fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    // ----- Positions -----

    pub(crate) fn locate(&self, id: BlockId) -> Option<Loc> {
        self.index.locate(&self.doc, id)
    }

    /// The text of a text node.
    pub(crate) fn text_of(&self, id: BlockId) -> Option<&Inline> {
        order::inline_at(&self.doc, &self.locate(id)?)
    }

    /// Compares two positions in document order. Positions in missing nodes
    /// sort last.
    pub(crate) fn cmp_pos(&self, a: TextPos, b: TextPos) -> Ordering {
        if a.block == b.block {
            return a.offset.cmp(&b.offset);
        }
        match (self.locate(a.block), self.locate(b.block)) {
            (Some(x), Some(y)) => order::cmp_loc(&x, &y),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        }
    }

    /// The selected range in document order, for text selections.
    pub(crate) fn ordered(&self) -> Option<(TextPos, TextPos)> {
        match self.selection {
            Selection::Text { anchor, head } => {
                if self.cmp_pos(anchor, head) == Ordering::Greater {
                    Some((head, anchor))
                } else {
                    Some((anchor, head))
                }
            }
            Selection::All => Some((self.first_pos()?, self.last_pos()?)),
            Selection::Node(_) => None,
        }
    }

    pub(crate) fn first_pos(&self) -> Option<TextPos> {
        order::first_text(&self.doc).map(|stop| TextPos {
            block: stop.id,
            offset: 0,
        })
    }

    pub(crate) fn last_pos(&self) -> Option<TextPos> {
        order::last_text(&self.doc).map(|stop| self.end_of(stop.id))
    }

    pub(crate) fn end_of(&self, id: BlockId) -> TextPos {
        TextPos {
            block: id,
            offset: self.text_of(id).map_or(0, |inline| inline.text.len()),
        }
    }

    /// Clamps a position to its node's text and a `char` boundary. `None`
    /// if the node does not exist or has no text.
    pub(crate) fn clamp(&self, pos: TextPos) -> Option<TextPos> {
        let text = &self.text_of(pos.block)?.text;
        Some(TextPos {
            block: pos.block,
            offset: super::inline_ops::floor_boundary(text, pos.offset),
        })
    }

    /// Makes a selection valid for the current document.
    pub(crate) fn validate(&self, selection: Selection) -> Option<Selection> {
        match selection {
            Selection::Text { anchor, head } => {
                let head = self.clamp(head)?;
                let anchor = self.clamp(anchor).unwrap_or(head);
                Some(Selection::Text { anchor, head })
            }
            Selection::Node(id) => match self.locate(id)? {
                Loc::Block(path) => {
                    let block = order::block_at(&self.doc.blocks, &path)?;
                    (!order::has_text(&block.kind)).then_some(Selection::Node(id))
                }
                Loc::Cell { .. } => None,
            },
            Selection::All => (!self.doc.blocks.is_empty()).then_some(Selection::All),
        }
    }

    // ----- Selection -----

    /// Sets the selection (from a click or the view). Invalid positions are
    /// clamped; a selection of a missing node is ignored.
    pub(crate) fn set_selection(&mut self, selection: Selection) -> bool {
        let Some(selection) = self.validate(selection) else {
            return false;
        };
        self.history.seal();
        self.rule_armed = false;
        self.pending_marks = None;
        self.marked = None;
        self.put_selection(selection)
    }

    /// Sets the selection without touching pending marks or history.
    pub(crate) fn put_selection(&mut self, selection: Selection) -> bool {
        if self.selection == selection {
            return false;
        }
        self.selection = selection;
        self.changes.selection = true;
        true
    }

    /// Ctrl+A: first the text of the current node, then the document.
    pub(crate) fn select_all(&mut self) -> bool {
        if self.doc.blocks.is_empty() {
            return false;
        }
        let whole_doc = Selection::All;
        let target = match self.ordered() {
            Some((start, end)) if start.block == end.block && self.selection != Selection::All => {
                let node_end = self.end_of(start.block);
                let whole_node = start.offset == 0 && end.offset == node_end.offset;
                if whole_node {
                    whole_doc
                } else {
                    Selection::Text {
                        anchor: TextPos {
                            block: start.block,
                            offset: 0,
                        },
                        head: node_end,
                    }
                }
            }
            _ => whole_doc,
        };
        self.set_selection(target)
    }

    fn stop_selection(&self, stop: &Stop, at_end: bool) -> Selection {
        if stop.text {
            let pos = if at_end {
                self.end_of(stop.id)
            } else {
                TextPos {
                    block: stop.id,
                    offset: 0,
                }
            };
            Selection::caret(pos)
        } else {
            Selection::Node(stop.id)
        }
    }

    /// Moves left by a grapheme or word, across nodes. Without `extend` a
    /// range collapses to its start first.
    pub(crate) fn move_left(&mut self, unit: Unit, extend: bool) -> bool {
        self.move_horizontal(unit, extend, false)
    }

    pub(crate) fn move_right(&mut self, unit: Unit, extend: bool) -> bool {
        self.move_horizontal(unit, extend, true)
    }

    fn move_horizontal(&mut self, unit: Unit, extend: bool, forward: bool) -> bool {
        let target = match self.selection.clone() {
            Selection::All => {
                let stop = if forward {
                    order::last_stop(&self.doc)
                } else {
                    order::first_stop(&self.doc)
                };
                match stop {
                    Some(stop) => self.stop_selection(&stop, forward),
                    None => return false,
                }
            }
            Selection::Node(id) => {
                let Some(loc) = self.locate(id) else {
                    return false;
                };
                let stop = if forward {
                    order::next_stop(&self.doc, &loc)
                } else {
                    order::prev_stop(&self.doc, &loc)
                };
                match stop {
                    Some(stop) => self.stop_selection(&stop, !forward),
                    None => return false,
                }
            }
            Selection::Text { anchor, head } => {
                if !extend && anchor != head {
                    let Some((start, end)) = self.ordered() else {
                        return false;
                    };
                    Selection::caret(if forward { end } else { start })
                } else {
                    match self.step_from(head, unit, forward, extend) {
                        Some(Selection::Text { head, .. }) if extend => {
                            Selection::Text { anchor, head }
                        }
                        Some(selection) => selection,
                        None => return false,
                    }
                }
            }
        };
        self.set_selection(target)
    }

    /// The selection one unit away from `pos`. Crossing into another node
    /// lands at its near end; a node block is selected as a whole unless
    /// `skip_nodes`.
    fn step_from(
        &self,
        pos: TextPos,
        unit: Unit,
        forward: bool,
        skip_nodes: bool,
    ) -> Option<Selection> {
        let text = &self.text_of(pos.block)?.text;
        let within = if forward {
            next_boundary(text, pos.offset, unit)
        } else {
            prev_boundary(text, pos.offset, unit)
        };
        if let Some(offset) = within {
            return Some(Selection::caret(TextPos {
                block: pos.block,
                offset,
            }));
        }
        let loc = self.locate(pos.block)?;
        let stop = if skip_nodes {
            if forward {
                order::next_text(&self.doc, &loc)
            } else {
                order::prev_text(&self.doc, &loc)
            }
        } else if forward {
            order::next_stop(&self.doc, &loc)
        } else {
            order::prev_stop(&self.doc, &loc)
        }?;
        Some(self.stop_selection(&stop, !forward))
    }

    pub(crate) fn move_to_doc_start(&mut self, extend: bool) -> bool {
        self.move_to_doc_edge(extend, false)
    }

    pub(crate) fn move_to_doc_end(&mut self, extend: bool) -> bool {
        self.move_to_doc_edge(extend, true)
    }

    fn move_to_doc_edge(&mut self, extend: bool, end: bool) -> bool {
        let anchor = match &self.selection {
            Selection::Text { anchor, .. } if extend => Some(*anchor),
            _ => None,
        };
        let target = match anchor {
            Some(anchor) => {
                let head = if end {
                    self.last_pos()
                } else {
                    self.first_pos()
                };
                match head {
                    Some(head) => Selection::Text { anchor, head },
                    None => return false,
                }
            }
            None => {
                let stop = if end {
                    order::last_stop(&self.doc)
                } else {
                    order::first_stop(&self.doc)
                };
                match stop {
                    Some(stop) => self.stop_selection(&stop, end),
                    None => return false,
                }
            }
        };
        self.set_selection(target)
    }

    // ----- Transactions -----

    /// Runs `f` as one transaction and records it in the history.
    ///
    /// `f` gets the current selection and returns the selection after the
    /// change. If it returns an error, every step is rolled back. A
    /// transaction without steps only sets the selection.
    pub(crate) fn transact(
        &mut self,
        kind: EditKind,
        now: Option<Instant>,
        text: &str,
        f: impl FnOnce(&mut Tr<'_>, &Selection) -> EditResult<Selection>,
    ) -> bool {
        let before = self.selection.clone();
        let mut tr = Tr::new(&mut self.doc, &self.index);
        let result = f(&mut tr, &before).and_then(|after| {
            tr.renumber()?;
            Ok(after)
        });
        let after = match result {
            Ok(after) => after,
            Err(error) => {
                if let Abort::Step(error) = &error {
                    debug_assert!(false, "edit failed: {error}");
                }
                tr.rollback();
                return false;
            }
        };
        if tr.is_empty() {
            drop(tr);
            return self.set_selection(after);
        }
        let (forward, inverse, applied) = tr.finish(before, after.clone());
        self.history.push(forward, inverse, kind, now, text);
        self.record(applied);
        self.rule_armed = false;
        self.pending_marks = None;
        self.marked = None;
        self.put_selection(after);
        true
    }

    fn record(&mut self, applied: Applied) {
        for id in applied.changed {
            if !self.changes.blocks.contains(&id) {
                self.changes.blocks.push(id);
            }
        }
        self.changes.structure |= applied.structure;
        self.changes.content = true;
    }

    pub(crate) fn undo(&mut self) -> bool {
        let Some(entry) = self.history.pop_undo() else {
            return false;
        };
        match transaction::apply(&mut self.doc, &self.index, &entry.inverse) {
            Ok((_, applied)) => {
                self.record(applied);
                self.after_history(entry.inverse.selection_after.clone());
                self.history.push_redo(entry);
                true
            }
            Err(error) => {
                debug_assert!(false, "undo failed: {error}");
                self.history.clear();
                false
            }
        }
    }

    pub(crate) fn redo(&mut self) -> bool {
        let Some(entry) = self.history.pop_redo() else {
            return false;
        };
        match transaction::apply(&mut self.doc, &self.index, &entry.forward) {
            Ok((_, applied)) => {
                self.record(applied);
                self.after_history(entry.forward.selection_after.clone());
                self.history.push_undone(entry);
                true
            }
            Err(error) => {
                debug_assert!(false, "redo failed: {error}");
                self.history.clear();
                false
            }
        }
    }

    fn after_history(&mut self, selection: Selection) {
        self.rule_armed = false;
        self.pending_marks = None;
        self.marked = None;
        let selection = self
            .validate(selection)
            .or_else(|| self.first_pos().map(Selection::caret));
        if let Some(selection) = selection {
            self.put_selection(selection);
        }
    }

    // ----- Commands -----

    /// Runs a public command. Returns `true` if the document or selection
    /// changed. `OpenFind` and `FoldHeading` are view state and return
    /// `false`.
    pub(crate) fn exec(&mut self, command: &Command, now: Instant) -> bool {
        match command {
            Command::ToggleMark(mark) => self.toggle_mark(mark.clone()),
            Command::SetBlock(kind) => self.set_block(kind.clone()),
            Command::Indent => self.indent(),
            Command::Outdent => self.outdent(),
            Command::ToggleTask => self.toggle_task(),
            Command::InsertTable { rows, cols } => self.insert_table(*rows, *cols),
            Command::InsertImage { src, alt } => self.insert_image(src, alt),
            Command::InsertRule => self.insert_rule(),
            Command::SetLink(url) => self.set_link(url.as_deref()),
            Command::MoveBlockUp => self.move_block(false),
            Command::MoveBlockDown => self.move_block(true),
            Command::FoldHeading | Command::OpenFind => false,
            Command::CheckAll => self.check_all(true),
            Command::UncheckAll => self.check_all(false),
            Command::MoveCheckedToBottom => self.move_checked_to_bottom(),
            Command::Undo => self.undo(),
            Command::Redo => self.redo(),
            Command::SelectAll => self.select_all(),
            Command::InsertText(text) => {
                if text.contains(['\n', '\r']) {
                    self.insert_plain_text(text)
                } else {
                    self.insert_text(text, now)
                }
            }
            Command::InsertMarkdown(markdown) => self.insert_markdown(markdown),
        }
    }

    /// The ids of the text blocks (not cells) touched by the selection, in
    /// document order.
    pub(crate) fn selected_blocks(&self) -> Vec<BlockId> {
        let Some((start, end)) = self.ordered() else {
            return Vec::new();
        };
        let (Some(from), Some(to)) = (self.locate(start.block), self.locate(end.block)) else {
            return Vec::new();
        };
        order::stops_between(&self.doc, &from, &to)
            .into_iter()
            .filter(|stop| stop.text && !stop.loc.is_cell())
            .map(|stop| stop.id)
            .collect()
    }

    /// The selected blocks without those inside another selected block.
    pub(crate) fn selected_roots(&self) -> Vec<BlockId> {
        let ids = self.selected_blocks();
        let paths: Vec<(BlockId, order::Path)> = ids
            .iter()
            .filter_map(|id| match self.locate(*id)? {
                Loc::Block(path) => Some((*id, path)),
                Loc::Cell { .. } => None,
            })
            .collect();
        paths
            .iter()
            .filter(|(_, path)| {
                !paths
                    .iter()
                    .any(|(_, other)| other.len() < path.len() && path.starts_with(other))
            })
            .map(|(id, _)| *id)
            .collect()
    }

    /// The block the caret is in, or the selected node.
    pub(crate) fn current_block(&self) -> Option<BlockId> {
        match &self.selection {
            Selection::Node(id) => Some(*id),
            Selection::All => None,
            Selection::Text { head, .. } => match self.locate(head.block)? {
                Loc::Block(_) => Some(head.block),
                Loc::Cell { table, .. } => order::block_at(&self.doc.blocks, &table).map(|b| b.id),
            },
        }
    }
}

pub(crate) fn is_word_segment(segment: &str) -> bool {
    segment.chars().any(char::is_alphanumeric)
}

/// The boundary before `offset`, or `None` at the start.
pub(crate) fn prev_boundary(text: &str, offset: usize, unit: Unit) -> Option<usize> {
    if offset == 0 {
        return None;
    }
    let before = text.get(..offset)?;
    match unit {
        Unit::Grapheme => before.grapheme_indices(true).next_back().map(|(i, _)| i),
        Unit::Word => {
            let mut seen_word = false;
            let mut at = 0;
            for (index, segment) in before.split_word_bound_indices().rev() {
                let word = is_word_segment(segment);
                if seen_word && !word {
                    at = index + segment.len();
                    break;
                }
                seen_word |= word;
            }
            Some(at)
        }
    }
}

/// The boundary after `offset`, or `None` at the end.
pub(crate) fn next_boundary(text: &str, offset: usize, unit: Unit) -> Option<usize> {
    if offset >= text.len() {
        return None;
    }
    let after = text.get(offset..)?;
    match unit {
        Unit::Grapheme => after.graphemes(true).next().map(|g| offset + g.len()),
        Unit::Word => {
            let mut seen_word = false;
            let mut at = text.len();
            for (index, segment) in after.split_word_bound_indices() {
                let word = is_word_segment(segment);
                if seen_word && !word {
                    at = offset + index;
                    break;
                }
                seen_word |= word;
            }
            Some(at)
        }
    }
}
