//! Undo and redo.
//!
//! Typing is grouped into one entry while keystrokes are less than
//! [`GROUP_INTERVAL`] apart, stay in one text node and do not start a new
//! word. Everything else is an entry of its own.

use std::time::{Duration, Instant};

use super::transaction::Transaction;
use crate::model::BlockId;

/// Keystrokes closer together than this are undone together.
pub(crate) const GROUP_INTERVAL: Duration = Duration::from_millis(500);

/// What kind of edit an entry is, for grouping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditKind {
    /// Text typed into a node.
    Insert { node: BlockId },
    /// Backspace inside a node.
    DeleteBackward { node: BlockId },
    /// Delete inside a node.
    DeleteForward { node: BlockId },
    /// Anything else, never merged.
    Other,
}

#[derive(Clone, Debug)]
pub(crate) struct Entry {
    pub(crate) forward: Transaction,
    pub(crate) inverse: Transaction,
    kind: EditKind,
    time: Option<Instant>,
    /// The typed text ended with whitespace: the next word starts a new entry.
    word_end: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct History {
    undo: Vec<Entry>,
    redo: Vec<Entry>,
    /// Set when the selection moved: the next edit starts a new entry.
    sealed: bool,
}

impl History {
    pub(crate) fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.sealed = false;
    }

    /// The next edit does not merge with the last entry.
    pub(crate) fn seal(&mut self) {
        self.sealed = true;
    }

    pub(crate) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub(crate) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Records an applied transaction. `text` is the typed text for
    /// [`EditKind::Insert`].
    pub(crate) fn push(
        &mut self,
        forward: Transaction,
        inverse: Transaction,
        kind: EditKind,
        time: Option<Instant>,
        text: &str,
    ) {
        self.redo.clear();
        let starts_word = !text.starts_with(char::is_whitespace);
        let word_end = text.ends_with(char::is_whitespace);
        let sealed = std::mem::take(&mut self.sealed);
        if let Some(last) = self.undo.last_mut() {
            let close = match (last.time, time) {
                (Some(last), Some(now)) => now.saturating_duration_since(last) < GROUP_INTERVAL,
                _ => false,
            };
            let new_word = matches!(kind, EditKind::Insert { .. }) && last.word_end && starts_word;
            if !sealed && kind != EditKind::Other && last.kind == kind && close && !new_word {
                last.forward.steps.extend(forward.steps);
                last.forward.selection_after = forward.selection_after;
                let mut steps = inverse.steps;
                steps.append(&mut last.inverse.steps);
                last.inverse.steps = steps;
                last.inverse.selection_before = inverse.selection_before;
                for (id, dirty) in inverse.restore_dirty {
                    if !last.inverse.restore_dirty.iter().any(|(known, _)| *known == id) {
                        last.inverse.restore_dirty.push((id, dirty));
                    }
                }
                last.time = time;
                last.word_end = word_end;
                return;
            }
        }
        self.undo.push(Entry {
            forward,
            inverse,
            kind,
            time,
            word_end,
        });
    }

    pub(crate) fn pop_undo(&mut self) -> Option<Entry> {
        self.sealed = true;
        self.undo.pop()
    }

    pub(crate) fn pop_redo(&mut self) -> Option<Entry> {
        self.sealed = true;
        self.redo.pop()
    }

    pub(crate) fn push_redo(&mut self, entry: Entry) {
        self.redo.push(entry);
    }

    /// Puts an entry back after redoing it, without clearing the redo stack.
    pub(crate) fn push_undone(&mut self, entry: Entry) {
        self.undo.push(entry);
    }
}
