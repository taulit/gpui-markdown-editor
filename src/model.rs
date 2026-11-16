//! The document model: a flat-ish tree of [`Block`]s with [`Inline`] content.
//!
//! The model is independent of GPUI and can be used headless, for example to
//! count open tasks in a file or to rewrite Markdown in a script:
//!
//! ```
//! use gpui_markdown_editor::Document;
//!
//! let source = "# Groceries\n\n- [ ] Milk\n- [x] Bread\n";
//! let doc = Document::parse(source);
//! // Unchanged documents serialize byte-identically.
//! assert_eq!(doc.to_markdown(), source);
//! ```
//!
//! # Lossless round-trip
//!
//! Every top-level block remembers the byte range it was parsed from
//! ([`Block::origin`]). Serializing copies the original text of every block
//! that was not edited, including the whitespace between blocks. Only blocks
//! marked dirty are written again, in the normalized style documented in
//! [`Document::to_markdown`].

use std::ops::Range;
use std::sync::Arc;

use gpui::SharedString;

use crate::extension::Extensions;

/// Identifies a [`Block`] or table [`Cell`] within one [`Document`].
///
/// Ids are unique per document and stay valid while the node exists. They are
/// not stable across [`Document::parse`] calls and must not be persisted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(pub(crate) u64);

/// Identifies a custom block node registered by an [`Extension`](crate::Extension).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CustomNodeId(pub &'static str);

/// Identifies a custom mark registered by an [`Extension`](crate::Extension).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CustomMarkId(pub &'static str);

/// A parsed Markdown document.
#[derive(Clone, Debug)]
pub struct Document {
    pub(crate) blocks: Vec<Block>,
    /// The text the document was parsed from. Clean blocks are copied from it.
    pub(crate) source: Arc<str>,
    #[allow(dead_code)] // read by the parser, which replaces the placeholder next
    pub(crate) next_id: u64,
    /// The extensions the document was parsed with, used again when writing.
    #[allow(dead_code)]
    pub(crate) extensions: Extensions,
}

impl Default for Document {
    fn default() -> Self {
        Self::parse("")
    }
}

impl Document {
    /// The top-level blocks in document order.
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// The Markdown source this document was parsed from.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Finds a block or nested block by id (depth-first).
    pub fn block(&self, id: BlockId) -> Option<&Block> {
        fn find(blocks: &[Block], id: BlockId) -> Option<&Block> {
            blocks.iter().find_map(|b| {
                if b.id == id {
                    Some(b)
                } else {
                    find(&b.children, id)
                }
            })
        }
        find(&self.blocks, id)
    }

    /// Returns `true` if any block was changed since parsing.
    pub fn is_modified(&self) -> bool {
        fn any_dirty(blocks: &[Block]) -> bool {
            blocks.iter().any(|b| b.dirty || any_dirty(&b.children))
        }
        any_dirty(&self.blocks) || self.structure_changed()
    }

    /// Allocates a fresh id for a new block or cell.
    #[allow(dead_code)]
    pub(crate) fn alloc_id(&mut self) -> BlockId {
        let id = BlockId(self.next_id);
        self.next_id += 1;
        id
    }

    /// `true` when top-level blocks were inserted, removed or reordered, so
    /// the origins no longer tile the source in order.
    fn structure_changed(&self) -> bool {
        let mut cursor = 0;
        for block in &self.blocks {
            match &block.origin {
                Some(origin) if origin.start >= cursor => cursor = origin.end,
                _ => return true,
            }
        }
        // Trailing text after the last block (only whitespace) is kept as is.
        self.source[cursor..].chars().any(|c| !c.is_whitespace())
    }
}

/// One block of a document: a paragraph, heading, list item, table and so on.
///
/// Lists are not containers. Each list item is its own block, and consecutive
/// items of the same kind form a list. Nested items and further paragraphs of
/// an item are its [`children`](Block::children).
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub(crate) id: BlockId,
    pub(crate) kind: BlockKind,
    pub(crate) content: Inline,
    pub(crate) children: Vec<Block>,
    /// Byte range in [`Document::source`]. Set for top-level blocks that came
    /// from the parser, `None` for blocks created by editing and for children.
    pub(crate) origin: Option<Range<usize>>,
    pub(crate) dirty: bool,
}

impl Block {
    /// The id of this block.
    pub fn id(&self) -> BlockId {
        self.id
    }

    /// What kind of block this is.
    pub fn kind(&self) -> &BlockKind {
        &self.kind
    }

    /// The inline content. Empty for blocks without text
    /// ([`BlockKind::Rule`], [`BlockKind::Image`], [`BlockKind::Table`]).
    pub fn content(&self) -> &Inline {
        &self.content
    }

    /// The plain text of this block without marks.
    pub fn text(&self) -> &str {
        &self.content.text
    }

    /// Nested blocks: sub-items of a list item, the content of a quote.
    pub fn children(&self) -> &[Block] {
        &self.children
    }

    /// The byte range this block was parsed from, if it is unchanged since.
    pub fn origin(&self) -> Option<Range<usize>> {
        if self.dirty {
            None
        } else {
            self.origin.clone()
        }
    }

    /// Whether this block was edited and will be written in normalized style.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
}

/// The kind of a [`Block`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum BlockKind {
    /// Plain text.
    Paragraph,
    /// A heading of level 1 to 6. The editor offers levels 1 to 3 and renders
    /// deeper levels like level 3.
    Heading(u8),
    /// An item of a bullet list.
    BulletItem,
    /// An item of a numbered list. `number` is the ordinal that is displayed
    /// and written.
    OrderedItem {
        /// The ordinal of this item.
        number: u32,
    },
    /// A checklist item.
    TaskItem {
        /// Whether the item is ticked.
        checked: bool,
    },
    /// A block quote. Its first paragraph is the block content, further
    /// blocks are children.
    Quote,
    /// A fenced or indented code block. The code is the block text.
    CodeBlock {
        /// The info string of the fence, for example `rust`.
        lang: Option<SharedString>,
    },
    /// A GFM table.
    Table(TableData),
    /// An image on its own line.
    Image(ImageData),
    /// A thematic break (`---`).
    Rule,
    /// Markdown or HTML the editor does not model. The block text is the
    /// source, shown and written back unchanged.
    Raw,
    /// A block provided by an [`Extension`](crate::Extension).
    Custom(CustomNodeId),
}

/// Attributes of an image block.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ImageData {
    /// The image source as written: a path or URL.
    pub src: SharedString,
    /// Alternative text.
    pub alt: SharedString,
    /// Optional title.
    pub title: Option<SharedString>,
    /// Display width in pixels, written Obsidian-style as `![alt|400](src)`.
    pub width: Option<u32>,
}

/// Horizontal alignment of a table column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Align {
    /// No alignment given (`---`).
    #[default]
    None,
    /// `:---`
    Left,
    /// `:---:`
    Center,
    /// `---:`
    Right,
}

/// The content of a table block. The first row is the header.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct TableData {
    /// One alignment per column.
    pub align: Vec<Align>,
    /// Rows of cells. Every row has `align.len()` cells.
    pub rows: Vec<Vec<Cell>>,
}

/// One table cell. Cells are addressed by id like blocks, see [`TextPos`].
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub(crate) id: BlockId,
    pub(crate) content: Inline,
}

impl Cell {
    /// The id of this cell.
    pub fn id(&self) -> BlockId {
        self.id
    }

    /// The inline content of this cell.
    pub fn content(&self) -> &Inline {
        &self.content
    }
}

/// Text with formatting: a string and non-overlapping spans per mark kind.
///
/// A hard line break is a `\n` in the text. Soft line breaks of the source
/// are stored as a single space.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Inline {
    pub(crate) text: String,
    /// Sorted by start offset. Spans of equal marks never overlap or touch.
    pub(crate) marks: Vec<MarkSpan>,
}

impl Inline {
    /// Creates unformatted inline content.
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            marks: Vec::new(),
        }
    }

    /// The text without marks.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The mark spans, sorted by start offset.
    pub fn marks(&self) -> &[MarkSpan] {
        &self.marks
    }

    /// Returns `true` if there is no text.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

/// A [`Mark`] applied to a byte range of an [`Inline`] text.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkSpan {
    /// Byte range in the text, on `char` boundaries, never empty.
    pub range: Range<usize>,
    /// The formatting applied to the range.
    pub mark: Mark,
}

/// Inline formatting.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Mark {
    /// `**bold**`
    Bold,
    /// `*italic*`
    Italic,
    /// `<u>underline</u>`
    Underline,
    /// `~~strike~~`
    Strike,
    /// `` `code` ``
    Code,
    /// `==highlight==`
    Highlight,
    /// `[text](url "title")`
    Link {
        /// The link target.
        url: SharedString,
        /// Optional title.
        title: Option<SharedString>,
    },
    /// Inline Markdown or HTML the editor does not model (inline images,
    /// footnote references, HTML tags). The covered text is the source and is
    /// written back unchanged.
    Raw,
    /// A mark provided by an [`Extension`](crate::Extension), for example a
    /// wiki link or tag.
    Custom {
        /// The registered mark.
        id: CustomMarkId,
        /// Extension-defined attribute, for example the target of a wiki link
        /// when it differs from the displayed text.
        attr: Option<SharedString>,
    },
}

/// A position in the text of a block or table cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextPos {
    /// The block or table cell that holds the text.
    pub block: BlockId,
    /// UTF-8 byte offset into the text, on a `char` boundary.
    pub offset: usize,
}

/// A range of text between two positions in document order.
///
/// Returned by [`MarkdownEditor::find`](crate::MarkdownEditor::find) and
/// accepted by [`highlight`](crate::MarkdownEditor::highlight) and
/// [`scroll_to`](crate::MarkdownEditor::scroll_to). Ranges are only valid for
/// the document they were created for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextRange {
    /// Start of the range (inclusive).
    pub start: TextPos,
    /// End of the range (exclusive).
    pub end: TextPos,
}
