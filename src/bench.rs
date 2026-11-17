//! Internals reached by `benches/`. Not part of the public API and not
//! covered by semver.

use std::time::Instant;

use crate::edit::{EditorState, Selection, Unit};
use crate::extension::Extensions;
use crate::model::{Document, TextPos};

/// The editing core without a view: what a keystroke costs before layout.
pub struct EditCore(EditorState);

impl EditCore {
    /// Parses `markdown` with `extensions`, caret at the start.
    pub fn new(markdown: &str, extensions: Extensions) -> Self {
        Self(EditorState::from_markdown(markdown, extensions))
    }

    /// The document being edited.
    pub fn document(&self) -> &Document {
        &self.0.doc
    }

    /// Puts the caret at `pos`.
    pub fn put_caret(&mut self, pos: TextPos) {
        self.0.set_selection(Selection::caret(pos));
        self.0.take_changes();
    }

    /// Types `text` at the caret as a keystroke does (input rules
    /// included) and takes the change set the view would read.
    pub fn type_text(&mut self, text: &str) {
        self.0.insert_text(text, Instant::now());
        self.0.take_changes();
    }

    /// Deletes the character before the caret, as Backspace does.
    pub fn backspace(&mut self) {
        self.0.delete_backward(Unit::Grapheme, Instant::now());
        self.0.take_changes();
    }
}
