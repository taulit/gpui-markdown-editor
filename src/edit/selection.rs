//! What is selected in a document.

use crate::model::{BlockId, TextPos};

/// The selection of an editor.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Selection {
    /// A caret (`anchor == head`) or a text range, possibly across blocks.
    /// `head` is the end that moves; it may come before `anchor`.
    Text {
        /// Where the selection started.
        anchor: TextPos,
        /// Where the caret is.
        head: TextPos,
    },
    /// A block selected as a whole: an image, rule or table.
    Node(BlockId),
}

impl Selection {
    /// A caret at `pos`.
    pub(crate) fn caret(pos: TextPos) -> Self {
        Self::Text {
            anchor: pos,
            head: pos,
        }
    }

    /// `true` for a text selection without extent.
    pub(crate) fn is_caret(&self) -> bool {
        matches!(self, Self::Text { anchor, head } if anchor == head)
    }

    /// The caret position, if this is a text selection.
    pub(crate) fn head(&self) -> Option<TextPos> {
        match self {
            Self::Text { head, .. } => Some(*head),
            Self::Node(_) => None,
        }
    }
}
