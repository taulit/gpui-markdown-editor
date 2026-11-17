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
    /// A rectangle of table cells between two cells of the same table,
    /// made by dragging across cells. `head` is the cell that moves.
    Cells {
        /// The cell the drag started in.
        anchor: BlockId,
        /// The cell under the pointer.
        head: BlockId,
    },
    /// The whole document, including image, rule and table blocks at its
    /// edges (the second Ctrl+A, or `Command::SelectAll`).
    All,
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

    /// The caret position, if this is a text selection. For a cell
    /// rectangle the start of its head cell, so that row and column
    /// commands act on that cell.
    pub(crate) fn head(&self) -> Option<TextPos> {
        match self {
            Self::Text { head, .. } => Some(*head),
            Self::Cells { head, .. } => Some(TextPos {
                block: *head,
                offset: 0,
            }),
            Self::Node(_) | Self::All => None,
        }
    }
}
