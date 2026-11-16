//! WYSIWYG Markdown editor component for [GPUI](https://www.gpui.rs), built to
//! sit next to [gpui-component](https://crates.io/crates/gpui-component).
//!
//! The editor reads and writes Markdown (CommonMark with GFM tables, task
//! lists and strikethrough) and keeps files it did not change byte-identical.
//! It consists of two layers:
//!
//! - [`MarkdownEditor`], the view. It is configured with [`EditorOptions`],
//!   driven with [`Command`]s, styled with [`EditorStyle`] and reports back
//!   with [`EditorEvent`]s.
//! - [`Document`], the model. It does not need a window and can be used on
//!   its own to parse, inspect and rewrite Markdown, see [`model`].
//!
//! # Quickstart
//!
//! ```no_run
//! use gpui::{App, AppContext as _, Context, Entity, IntoElement, Render, Window, div, prelude::*};
//! use gpui_markdown_editor::extensions::{Tag, WikiLink};
//! use gpui_markdown_editor::{EditorEvent, EditorOptions, MarkdownEditor};
//!
//! struct Note {
//!     editor: Entity<MarkdownEditor>,
//! }
//!
//! impl Note {
//!     fn new(markdown: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
//!         let options = EditorOptions {
//!             first_line_is_title: true,
//!             extensions: vec![Box::new(WikiLink), Box::new(Tag)],
//!             ..Default::default()
//!         };
//!         let editor = cx.new(|cx| MarkdownEditor::new(window, cx, options));
//!         editor.update(cx, |editor, cx| editor.set_markdown(markdown, cx));
//!
//!         cx.subscribe(&editor, |_, editor, event: &EditorEvent, cx| match event {
//!             EditorEvent::Changed => {
//!                 let markdown = editor.read(cx).markdown();
//!                 // Save `markdown`, then call `mark_saved` on the editor.
//!                 # let _ = markdown;
//!             }
//!             EditorEvent::LinkClicked(url) => cx.open_url(url),
//!             _ => {}
//!         })
//!         .detach();
//!
//!         Self { editor }
//!     }
//! }
//!
//! impl Render for Note {
//!     fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
//!         div().size_full().child(self.editor.clone())
//!     }
//! }
//!
//! fn init(cx: &mut App) {
//!     // Once, before the first editor is created.
//!     gpui_component::init(cx);
//! }
//! ```
//!
//! A complete program is in `examples/basic.rs`.
//!
//! # Feature flags
//!
//! | Feature | Default | Effect |
//! |---|---|---|
//! | `ui` | yes | The built-in UI parts selected with [`UiParts`]: format bar, slash menu, find bar, link popover. |
//! | `paste-html` | yes | Converts HTML from the clipboard into document blocks on paste. |
//! | `serde` | no | `Serialize` and `Deserialize` for [`DocMeta`]. |
//!
//! # Status
//!
//! This is version 0.0.1, the API skeleton. The public API is complete and is
//! what later versions implement, but the editor behind it is deliberately
//! simple for now:
//!
//! - The view edits the **Markdown source as plain text** in a multi-line
//!   input. Formatting is not rendered yet. Loading, editing, undo, saving,
//!   [`find`](MarkdownEditor::find) and
//!   [`word_count`](MarkdownEditor::word_count) work.
//! - Of the [`Command`]s only `Undo`, `Redo`, `SelectAll`, `InsertText`,
//!   `InsertMarkdown` and `OpenFind` have an effect. The others are accepted
//!   and do nothing.
//! - [`highlight`](MarkdownEditor::highlight),
//!   [`scroll_to`](MarkdownEditor::scroll_to) and
//!   [`set_meta`](MarkdownEditor::set_meta) store their state without a
//!   visible effect. Of [`EditorStyle`] only the font, text size, line height
//!   and the top and side padding are applied. Colors follow the
//!   gpui-component theme.
//! - Of [`EditorOptions`] only `max_width`, `extensions` and `ui.find_bar`
//!   are used. The callbacks are never called.
//! - Only [`EditorEvent::Changed`] and [`EditorEvent::SelectionChanged`] are
//!   emitted.

pub mod command;
pub mod editor;
pub mod event;
pub mod extension;
pub mod extensions;
pub mod markdown;
pub mod meta;
pub mod model;
pub mod options;
pub mod style;

pub use command::Command;
pub use editor::MarkdownEditor;
pub use event::EditorEvent;
pub use extension::{Extension, Extensions, InlineMatch};
pub use meta::DocMeta;
pub use model::{
    Align, Block, BlockId, BlockKind, Cell, CustomMarkId, CustomNodeId, Document, ImageData,
    Inline, Mark, MarkSpan, TableData, TextPos, TextRange,
};
pub use options::{
    Completion, CompletionKind, CompletionProvider, EditorOptions, ImageHandler, SlashHandler,
    SlashItem, SrcResolver, UiParts,
};
pub use style::EditorStyle;
