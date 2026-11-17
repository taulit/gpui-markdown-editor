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
//!     gpui_markdown_editor::init(cx);
//! }
//! ```
//!
//! [`init`] registers the key bindings in the key context `MarkdownEditor`.
//! [`MarkdownEditor::new`] calls it if the host did not, but calling it at
//! startup, before binding your own keys, lets your bindings take
//! precedence.
//!
//! Complete programs are in `examples/basic.rs` and `examples/notes.rs`.
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
//! Unreleased, phase E3: the document is edited in place (WYSIWYG).
//! What works:
//!
//! - **Typing** with the Markdown input rules (`# `, `- `, `1. `, `[] `,
//!   `> `, ` ``` `, `---`, `**x**`, `*x*`, `~~x~~`, `` `x` ``, `==x==`;
//!   Backspace right after a rule undoes it), Enter and Shift+Enter, Tab and
//!   Shift+Tab in lists, code blocks and table cells.
//! - **Shortcuts** (Cmd instead of Ctrl on macOS): Ctrl+B/I/U, Ctrl+Shift+X
//!   strike, Ctrl+E code, Ctrl+Shift+H highlight, Ctrl+K link (links to the
//!   URL on the clipboard or removes the link), Ctrl+Shift+L checklist,
//!   Ctrl+Alt+1..3 headings, Ctrl+Alt+0 text, Ctrl+Enter checkbox, Alt+Up/Down
//!   move block, Ctrl+Z, Ctrl+Shift+Z and Ctrl+Y, Ctrl+A (block, then
//!   document), Ctrl+C/X/V.
//! - **Caret movement** with arrows, Ctrl+arrows (Alt on macOS) by words,
//!   Home/End by visual lines, Ctrl+Home/End, PageUp/PageDown, all with Shift
//!   to select, across blocks. Up and Down keep the column. Backspace and
//!   Delete, with Ctrl by words.
//! - **Mouse**: click, Shift+click, drag (scrolling at the edges), double
//!   click selects a word, triple click a block; a click on a checkbox
//!   toggles it, on a rule or image selects it. Ctrl+click (Cmd+click) on a
//!   link, wiki link or tag emits its event; a plain click places the caret.
//! - **IME** through GPUI's input handler: marked text is underlined, the
//!   candidate window follows the caret, stale ranges from the input method
//!   are clamped.
//! - **Clipboard**: copy and cut write the selection as Markdown text. Paste
//!   inserts Markdown as blocks and other text as plain text; a URL pasted
//!   over selected text links it.
//! - **History**: undo and redo, typing grouped by word. Undoing every edit
//!   gives back the loaded text byte for byte; [`is_dirty`](MarkdownEditor::is_dirty)
//!   follows the history, so undoing to the saved state is clean again.
//! - **Tables**: caret, typing, marks and selection in cells; arrows, Tab,
//!   Shift+Tab and Enter between cells. Dragging from one cell into another
//!   selects a rectangle of cells: copy writes it as a Markdown table,
//!   Delete empties its cells, typing replaces the text of the first cell.
//!   Near a table or with the caret in it, "+" bars below and right of it
//!   add a row or column, and handles at the focused row and column open a
//!   menu to insert, move, delete and align. Columns are as wide as their
//!   content (48 to 320 px), a table wider than the column is scaled down
//!   and wraps. Dragging a column border sets its width, stored in
//!   [`DocMeta::column_widths`] with [`EditorEvent::MetaChanged`].
//! - **Images**: loaded asynchronously through
//!   [`EditorOptions::resolve_src`] (absolute paths without a resolver,
//!   http(s) URLs through GPUI), decoded and scaled down off the UI thread,
//!   only near the viewport. Shown with radius 12 at their size (or the
//!   width set for them) within the content width; the placeholder with the
//!   alt text stays while loading and on failure. A selected image has
//!   resize handles that store the width as `![alt|400](src)`. Pasted image
//!   data goes through [`EditorOptions::on_image`].
//! - **UI parts** ([`UiParts`]): the format bar at the bottom adapts to the
//!   selection and opens the style popover; the slash menu (`/`) inserts
//!   block types, extension and host entries; the link popover edits links
//!   and a bubble at a link offers open, edit, remove; the find bar (Ctrl+F)
//!   counts and steps through matches; `[[` and `#` open the host's
//!   [completions](EditorOptions::completions). All work with the keyboard.
//! - **Folding**: the chevron left of a heading (or Ctrl+.) folds its
//!   section; folded blocks are not laid out and the caret skips them. The
//!   state is [`DocMeta::folded_headings`].
//! - **Drag and drop**: a handle left of a block moves it with its
//!   children; dropped image files go through [`EditorOptions::on_image`].
//!   [`markdown::html_to_markdown`] converts rich text for hosts that read
//!   HTML from the clipboard (GPUI has no HTML clipboard format).
//! - **Extensions** style their marks, add input rules, click events and
//!   slash entries, see [`Extension`].
//! - **Accessibility**: the document and its visible blocks are AccessKit
//!   nodes with roles (heading levels, checkbox states); popups have roles
//!   and labels. The caret and selected text are not reported (GPUI's
//!   synthetic nodes are leaves).
//! - All [`Command`]s. [`highlight`](MarkdownEditor::highlight),
//!   [`scroll_to`](MarkdownEditor::scroll_to), [`EditorStyle`],
//!   `first_line_is_title` and `max_width` are applied.
//!
//! Not yet: attachments ([`EditorEvent::AttachmentClicked`] is never
//! emitted).

pub mod command;
mod edit;
pub mod editor;
pub mod event;
pub mod extension;
pub mod extensions;
pub mod markdown;
pub mod meta;
pub mod model;
pub mod options;
pub mod style;
mod view;

pub use command::Command;
pub use editor::{MarkdownEditor, init};
pub use event::EditorEvent;
pub use extension::{Extension, Extensions, InlineMatch, MarkColor, MarkStyle};
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
