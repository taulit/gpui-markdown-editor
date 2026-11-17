//! Markdown parsing and lossless serialization for [`Document`].

use std::sync::Arc;

use crate::extension::Extensions;
use crate::model::{Block, BlockId, BlockKind, Document, Inline};

impl Document {
    /// Parses Markdown (CommonMark with GFM tables, task lists and
    /// strikethrough) into a document.
    ///
    /// Parsing never fails. Constructs the editor does not model are kept as
    /// [`BlockKind::Raw`] blocks.
    pub fn parse(source: &str) -> Self {
        Self::parse_with(source, Extensions::default())
    }

    /// Like [`parse`](Self::parse), with custom inline syntax from
    /// `extensions`. The document keeps the extensions for serialization.
    pub fn parse_with(source: &str, extensions: Extensions) -> Self {
        // Placeholder until the real parser lands: one raw block spanning the
        // whole source keeps the round-trip lossless.
        let mut blocks = Vec::new();
        if !source.is_empty() {
            blocks.push(Block {
                id: BlockId(0),
                kind: BlockKind::Raw,
                content: Inline::plain(source),
                children: Vec::new(),
                origin: Some(0..source.len()),
                dirty: false,
            });
        }
        Self {
            blocks,
            source: Arc::from(source),
            next_id: 1,
            extensions,
        }
    }

    /// Serializes the document to Markdown.
    ///
    /// Blocks that were not edited are copied from the original source, so an
    /// unchanged document serializes byte-identically. Edited blocks are
    /// written in a fixed style: `-` for bullets, `**` for bold, `*` for
    /// italic, `- [ ]` for tasks and aligned table columns.
    pub fn to_markdown(&self) -> String {
        if !self.is_modified() {
            return self.source.to_string();
        }
        self.blocks
            .iter()
            .map(|b| b.content.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}
