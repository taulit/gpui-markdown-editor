//! Editing commands.

use crate::model::{BlockKind, Mark};

/// An editing command, run with
/// [`MarkdownEditor::command`](crate::MarkdownEditor::command).
///
/// Commands are the programmatic counterpart of what the user does with the
/// keyboard, the format bar and the slash menu. Hosts bind them to their own
/// shortcuts, menus and toolbars. Unless stated otherwise a command applies to
/// the selection, or to the block that holds the cursor when nothing is
/// selected. A command that does not apply in the current context does
/// nothing.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Command {
    /// Adds the mark to the selection, or removes it if the whole selection
    /// already has it. Without a selection it applies to the text typed next.
    ToggleMark(Mark),
    /// Turns the selected blocks into the given kind, for example a heading or
    /// a checklist item. The text is kept.
    SetBlock(BlockKind),
    /// Nests the selected list items one level deeper.
    Indent,
    /// Moves the selected list items one level up.
    Outdent,
    /// Ticks or unticks the selected checklist items.
    ToggleTask,
    /// Inserts an empty table below the current block. The first row is the
    /// header.
    InsertTable {
        /// Number of rows including the header row.
        rows: usize,
        /// Number of columns.
        cols: usize,
    },
    /// Inserts an image block below the current block.
    InsertImage {
        /// The image source: a path or URL, written to the Markdown as is.
        src: String,
        /// Alternative text.
        alt: String,
    },
    /// Inserts a thematic break below the current block.
    InsertRule,
    /// Links the selection to the URL. `None` removes the link at the cursor.
    SetLink(Option<String>),
    /// Moves the selected blocks above the block before them.
    MoveBlockUp,
    /// Moves the selected blocks below the block after them.
    MoveBlockDown,
    /// Folds or unfolds the heading at the cursor. The state is part of
    /// [`DocMeta`](crate::DocMeta).
    FoldHeading,
    /// Ticks every checklist item of the document.
    CheckAll,
    /// Unticks every checklist item of the document.
    UncheckAll,
    /// Moves ticked items below the open ones in every checklist, keeping
    /// their relative order.
    MoveCheckedToBottom,
    /// Reverts the last change.
    Undo,
    /// Applies the last reverted change again.
    Redo,
    /// Selects the whole document.
    SelectAll,
    /// Replaces the selection with plain text, as if it was typed.
    InsertText(String),
    /// Replaces the selection with Markdown, parsed into blocks. Used by hosts
    /// to insert templates and snippets.
    InsertMarkdown(String),
    /// Opens the find bar, if [`UiParts::find_bar`](crate::UiParts::find_bar)
    /// is enabled.
    OpenFind,
}
