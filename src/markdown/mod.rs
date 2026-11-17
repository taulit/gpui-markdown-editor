//! Markdown parsing and lossless serialization for [`Document`].
//!
//! Parsing maps CommonMark with GFM tables, task lists and strikethrough to
//! the document model. Whatever the model has no node for (HTML, footnotes,
//! link reference definitions, front matter, math, callouts) is kept as raw
//! source, so nothing is lost.
//!
//! Serializing copies the source of every block that was not edited and
//! writes the others in a fixed style, see [`Document::to_markdown`].

mod inline;
mod parse;
mod prefix;
mod serialize;
mod write_inline;

#[cfg(test)]
mod tests;

use crate::extension::Extensions;
use crate::model::{Block, BlockId, Document};

impl Document {
    /// Parses Markdown (CommonMark with GFM tables, task lists and
    /// strikethrough) into a document.
    ///
    /// Parsing never fails. Constructs the editor does not model are kept as
    /// [`BlockKind::Raw`](crate::BlockKind::Raw) blocks and
    /// [`Mark::Raw`](crate::Mark::Raw) spans.
    pub fn parse(source: &str) -> Self {
        Self::parse_with(source, Extensions::default())
    }

    /// Like [`parse`](Self::parse), with custom inline syntax from
    /// `extensions`. The document keeps the extensions for serialization.
    pub fn parse_with(source: &str, extensions: Extensions) -> Self {
        parse::parse(source, extensions)
    }

    /// Serializes the document to Markdown.
    ///
    /// Blocks that were not edited are copied from the original source
    /// together with the blank lines between them, so an unchanged document
    /// serializes byte-identically. Edited and new blocks are written in a
    /// fixed style:
    ///
    /// - ATX headings (`## Title`), `---` for rules, `> ` for quotes
    /// - `-` for bullets, `1.` for numbered items, `- [ ]` and `- [x]` for
    ///   tasks. An edited item of an existing list keeps the marker of that
    ///   list.
    /// - `**bold**`, `*italic*`, `~~strike~~`, `` `code` ``, `==highlight==`,
    ///   `<u>underline</u>`, `[text](url "title")`
    /// - fenced code blocks, with a fence longer than any backtick run inside
    /// - tables with padded, aligned columns
    /// - one blank line between blocks, none between items of one list
    ///
    /// Characters are escaped only where they would be read as syntax. New
    /// text uses the line ending of the source (`\n` or `\r\n`).
    pub fn to_markdown(&self) -> String {
        serialize::serialize(self, serialize::Mode::Mixed)
    }

    /// Serializes every block in the normalized style, ignoring the source.
    #[doc(hidden)]
    pub fn normalized(&self) -> String {
        serialize::serialize(self, serialize::Mode::Normalized)
    }

    /// Marks the block with `id` as edited. Returns `false` if there is no
    /// such block. Meant for tests of the serializer.
    #[doc(hidden)]
    pub fn mark_dirty(&mut self, id: BlockId) -> bool {
        fn find(blocks: &mut [Block], id: BlockId) -> Option<&mut Block> {
            blocks.iter_mut().find_map(|block| {
                if block.id == id {
                    Some(block)
                } else {
                    find(&mut block.children, id)
                }
            })
        }
        match find(&mut self.blocks, id) {
            Some(block) => {
                block.dirty = true;
                true
            }
            None => false,
        }
    }

    /// Marks every block as edited.
    #[allow(dead_code)] // used by tests until the editor applies transactions
    pub(crate) fn mark_all_dirty(&mut self) {
        fn mark(blocks: &mut [Block]) {
            for block in blocks {
                block.dirty = true;
                mark(&mut block.children);
            }
        }
        mark(&mut self.blocks);
    }
}
