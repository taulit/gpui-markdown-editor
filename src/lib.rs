//! WYSIWYG Markdown editor component for GPUI.

pub mod extension;
pub mod extensions;
pub mod markdown;
pub mod model;

pub use extension::{Extension, Extensions, InlineMatch};
pub use model::{
    Align, Block, BlockId, BlockKind, Cell, CustomMarkId, CustomNodeId, Document, ImageData,
    Inline, Mark, MarkSpan, TableData, TextPos, TextRange,
};
