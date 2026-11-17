//! Events emitted by the editor.

/// An event emitted by [`MarkdownEditor`](crate::MarkdownEditor).
///
/// Subscribe with `cx.subscribe(&editor, ...)`. Click events only report what
/// was clicked. Opening a link, note or file is up to the host.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum EditorEvent {
    /// The document was edited by the user or by a
    /// [`Command`](crate::Command). Not emitted by
    /// [`set_markdown`](crate::MarkdownEditor::set_markdown). Hosts usually
    /// debounce this and save [`markdown`](crate::MarkdownEditor::markdown).
    Changed,
    /// The cursor moved or the selection changed.
    SelectionChanged,
    /// The user changed presentation state in the editor: resized a table
    /// column or folded a heading. Read it with
    /// [`meta`](crate::MarkdownEditor::meta). Not emitted by
    /// [`set_meta`](crate::MarkdownEditor::set_meta).
    MetaChanged,
    /// A link was activated. Holds the link target as written in the Markdown.
    LinkClicked(String),
    /// A wiki link (`[[Target]]`) was activated. Holds the target, not the
    /// alias.
    WikiLinkClicked(String),
    /// A tag was activated. Holds the tag without the leading `#`.
    TagClicked(String),
    /// An attachment was activated: a paragraph that is only a link to a
    /// local file that is not an image, clicked on its row (or Ctrl/Cmd+
    /// clicked on its name). Holds the link target as written in the
    /// Markdown.
    AttachmentClicked(String),
}
