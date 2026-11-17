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
//! | `paste-html` | yes | Converts HTML from the clipboard into document blocks on paste, and [`markdown::html_to_markdown`]. |
//! | `serde` | no | `Serialize` and `Deserialize` for [`DocMeta`]. |
//!
//! # Loading, saving and events
//!
//! [`set_markdown`](MarkdownEditor::set_markdown) replaces the content,
//! clears the undo history and leaves the editor clean;
//! [`markdown`](MarkdownEditor::markdown) returns the current text.
//! [`is_dirty`](MarkdownEditor::is_dirty) follows the history (undoing back
//! to the saved state is clean again); call
//! [`mark_saved`](MarkdownEditor::mark_saved) after writing the file.
//!
//! The editor reports through [`EditorEvent`]: `Changed` after every edit
//! (debounce it before saving), `SelectionChanged`, `MetaChanged` when
//! [`DocMeta`] changed, and click events for links, wiki links, tags and
//! attachments. Clicks only report; opening anything is up to the host.
//! Links fire on Ctrl+click (Cmd+click on macOS) so that a plain click can
//! place the caret.
//!
//! # Options
//!
//! [`EditorOptions`] is fixed at creation:
//!
//! - `first_line_is_title` draws the first paragraph as a title (display
//!   only), `max_width` limits the centered content column (default 700 px).
//! - `extensions` adds custom inline syntax, see below.
//! - `resolve_src` maps image and attachment sources to files, `on_image`
//!   stores pasted or dropped image data, see [Images and
//!   attachments](#images-and-attachments).
//! - `completions` suggests targets after `[[` and `#`; `slash_items` adds
//!   host entries to the slash menu.
//! - `ui` switches the built-in [`UiParts`] on or off, to replace them with
//!   your own UI driven by commands.
//!
//! Colors and sizes come from [`EditorStyle`]; [`EditorStyle::from_theme`]
//! derives them from the active gpui-component theme, and
//! [`set_style`](MarkdownEditor::set_style) applies a new one (call it
//! again when the theme changes).
//!
//! # Commands
//!
//! Everything the toolbar and the keys do is available as a [`Command`] for
//! host UI, menus and scripts:
//!
//! ```no_run
//! # use gpui::{Context, Entity, Window};
//! # use gpui_markdown_editor::{BlockKind, Command, Mark, MarkdownEditor};
//! # fn run(editor: &Entity<MarkdownEditor>, window: &mut Window, cx: &mut gpui::App) {
//! editor.update(cx, |editor, cx| {
//!     editor.command(Command::ToggleMark(Mark::Bold), window, cx);
//!     editor.command(Command::SetBlock(BlockKind::Heading(2)), window, cx);
//!     editor.command(Command::InsertMarkdown("- [ ] new task\n".into()), window, cx);
//! });
//! # }
//! ```
//!
//! Commands are part of the undo history and emit `Changed` like typing.
//! Besides commands, the editor offers [`find`](MarkdownEditor::find),
//! [`highlight`](MarkdownEditor::highlight),
//! [`scroll_to`](MarkdownEditor::scroll_to) and
//! [`word_count`](MarkdownEditor::word_count).
//!
//! # Extensions
//!
//! An [`Extension`] adds inline syntax: it finds it in text, writes it
//! back, styles it, completes it while typing, reports clicks and adds slash
//! entries. All hooks have defaults. The crate ships
//! [`WikiLink`](extensions::WikiLink) (`[[Note]]`, `[[Note|Alias]]`),
//! [`Tag`](extensions::Tag) (`#tag`) and
//! [`Highlight`](extensions::Highlight) (`==text==`).
//!
//! A complete extension for `@mentions`:
//!
//! ```
//! use gpui::FontWeight;
//! use gpui_markdown_editor::extension::{Extension, InlineMatch, MarkColor, MarkStyle};
//! use gpui_markdown_editor::{CustomMarkId, Document, EditorEvent, Extensions, Mark};
//!
//! struct Mention;
//!
//! impl Mention {
//!     const MARK: CustomMarkId = CustomMarkId("mention");
//!
//!     fn mark() -> Mark {
//!         Mark::Custom { id: Self::MARK, attr: None }
//!     }
//! }
//!
//! impl Extension for Mention {
//!     fn name(&self) -> &'static str {
//!         "mention"
//!     }
//!
//!     // `@` followed by letters, at the start or after a space.
//!     fn parse_inline(&self, text: &str) -> Vec<InlineMatch> {
//!         let mut found = Vec::new();
//!         for (at, _) in text.match_indices('@') {
//!             if at > 0 && !text[..at].ends_with(' ') {
//!                 continue;
//!             }
//!             let len = text[at + 1..]
//!                 .find(|c: char| !c.is_alphanumeric())
//!                 .unwrap_or(text.len() - at - 1);
//!             if len > 0 {
//!                 let range = at..at + 1 + len;
//!                 found.push(InlineMatch {
//!                     text: text[range.clone()].to_string(),
//!                     range,
//!                     mark: Self::mark(),
//!                 });
//!             }
//!         }
//!         found
//!     }
//!
//!     fn serialize_mark(&self, mark: &Mark, text: &str) -> Option<String> {
//!         (*mark == Self::mark()).then(|| text.to_string())
//!     }
//!
//!     fn mark_style(&self, id: CustomMarkId) -> Option<MarkStyle> {
//!         (id == Self::MARK).then(|| MarkStyle::new().color(MarkColor::Link).weight(FontWeight::MEDIUM))
//!     }
//!
//!     // Ctrl/Cmd+click on a mention reports it as a link.
//!     fn click(&self, mark: &Mark, text: &str) -> Option<EditorEvent> {
//!         (*mark == Self::mark()).then(|| EditorEvent::LinkClicked(format!("mention:{}", &text[1..])))
//!     }
//! }
//!
//! let source = "Ask @anna about it.\n";
//! let doc = Document::parse_with(source, Extensions::new(vec![Box::new(Mention)]));
//! let marks = doc.blocks()[0].content().marks();
//! assert_eq!(marks[0].mark, Mention::mark());
//! assert_eq!(doc.to_markdown(), source);
//! assert_eq!(doc.normalized(), source);
//! ```
//!
//! Pass extensions in [`EditorOptions::extensions`]; earlier ones win when
//! matches overlap. Extensions add marks only: they cannot add inline nodes
//! or block renderers.
//!
//! # Presentation state: `DocMeta`
//!
//! Table column widths and folded headings are not Markdown. The editor
//! keeps them in [`DocMeta`] and emits [`EditorEvent::MetaChanged`] when
//! the user changes them; the host stores them next to the file and hands
//! them back after loading:
//!
//! ```no_run
//! # use gpui::{Context, Entity};
//! # use gpui_markdown_editor::{DocMeta, EditorEvent, MarkdownEditor};
//! # struct Host { stored: DocMeta }
//! # impl Host {
//! fn open(&mut self, editor: &Entity<MarkdownEditor>, text: &str, cx: &mut Context<Self>) {
//!     let meta = self.stored.clone(); // read from a sidecar file or database
//!     editor.update(cx, |editor, cx| {
//!         editor.set_markdown(text, cx);
//!         editor.set_meta(meta, cx);
//!     });
//!     cx.subscribe(editor, |host, editor, event: &EditorEvent, cx| {
//!         if *event == EditorEvent::MetaChanged {
//!             host.stored = editor.read(cx).meta().clone(); // write it back
//!         }
//!     })
//!     .detach();
//! }
//! # }
//! ```
//!
//! Entries are keyed by the position of the table or heading in the
//! document, so they survive reloads; entries past the end are ignored.
//! With the `serde` feature `DocMeta` serializes, with missing fields
//! defaulting.
//!
//! # Images and attachments
//!
//! An image (`![alt](src)`, optionally `![alt|400](src)` for a width) loads
//! asynchronously when it comes near the viewport.
//! [`EditorOptions::resolve_src`] maps `src` as written (usually a path
//! relative to the note) to a file; without a resolver only absolute paths
//! load. `http(s)` sources load through GPUI's HTTP client. Files and
//! downloads over 64 MB and images over 16384 px a side are refused; the
//! placeholder with the alt text stays while loading and on failure.
//!
//! Pasted or dropped image data goes to [`EditorOptions::on_image`], which
//! stores the bytes and returns the source to insert:
//!
//! ```
//! use std::sync::Arc;
//! use gpui_markdown_editor::{EditorOptions, ImageHandler};
//!
//! let on_image: ImageHandler = Arc::new(|bytes: Vec<u8>, mime: &str| {
//!     let ext = mime.strip_prefix("image/").unwrap_or("png");
//!     let name = format!("attachments/pasted-{}.{ext}", bytes.len());
//!     // std::fs::write(note_dir.join(&name), bytes).ok()?;
//!     Some(name)
//! });
//! let options = EditorOptions { on_image: Some(on_image), ..Default::default() };
//! # let _ = options;
//! ```
//!
//! A paragraph that is only a link to a local file that is not an image
//! (`[Report.pdf](files/report.pdf)`) is drawn as an attachment row with
//! the file size (when `resolve_src` finds the file); clicking it emits
//! [`EditorEvent::AttachmentClicked`]. Dropped files that are not images
//! are inserted that way.
//!
//! # Keys
//!
//! Cmd instead of Ctrl on macOS. The bindings live in the key context
//! `MarkdownEditor`; table bindings in `MarkdownEditor && table` (caret in
//! a cell).
//!
//! | Keys | Action |
//! |---|---|
//! | Ctrl+B, Ctrl+I, Ctrl+U | Bold, italic, underline |
//! | Ctrl+Shift+X, Ctrl+E, Ctrl+Shift+H | Strikethrough, inline code, highlight |
//! | Ctrl+K | Link the selection to the URL on the clipboard, or remove the link |
//! | Ctrl+Alt+1 / 2 / 3, Ctrl+Alt+0 | Heading 1 to 3, plain text |
//! | Ctrl+Shift+L, Ctrl+Enter | Checklist, tick or untick |
//! | Tab, Shift+Tab | Indent, outdent (next cell in tables) |
//! | Alt+Up, Alt+Down | Move the block (the row in tables) |
//! | Ctrl+Z, Ctrl+Shift+Z or Ctrl+Y | Undo, redo |
//! | Ctrl+A | Select the block, then the document |
//! | Ctrl+C, Ctrl+X, Ctrl+V | Copy and cut as Markdown, paste |
//! | Ctrl+F, F3, Shift+F3 | Find, next and previous match |
//! | Ctrl+. | Fold or unfold the heading's section |
//! | Shift+Enter | Line break in the block |
//! | Ctrl+arrows (Alt on macOS), Home/End, Ctrl+Home/End, PageUp/PageDown | Move by words, lines, document, pages; with Shift to select |
//! | Ctrl+Alt+Up / Down / Left / Right | Insert a table row above/below, a column left/right |
//! | Ctrl+Alt+Shift+Left / Right | Move the table column |
//! | Ctrl+Alt+Backspace, Ctrl+Alt+Shift+Backspace | Delete the table row, the column |
//! | Shift+F10 | Row menu, pressed again the column menu |
//!
//! Markdown input rules: `# ` to `###### `, `- ` or `* `, `1. `, `[] ` or
//! `- [ ] `, `> `, ` ``` `, `---`, and `**x**`, `*x*`, `~~x~~`, `` `x` ``,
//! `==x==` around text. Backspace right after a rule undoes it.
//!
//! The actions are named `markdown_editor::<Name>` (for example
//! `markdown_editor::ToggleBold`). A host rebinds a key by binding an action
//! by name in the same context after [`init`]; later bindings win, and
//! [`gpui::NoAction`] removes a default:
//!
//! ```no_run
//! use std::rc::Rc;
//! use gpui::{App, KeyBinding, KeyBindingContextPredicate, NoAction};
//!
//! fn rebind(cx: &mut App) {
//!     gpui_component::init(cx);
//!     gpui_markdown_editor::init(cx);
//!     let context = KeyBindingContextPredicate::parse("MarkdownEditor").ok().map(Rc::new);
//!     let bold = cx.build_action("markdown_editor::ToggleBold", None).unwrap();
//!     let binding = KeyBinding::load("ctrl-shift-b", bold, context, false, None, cx).unwrap();
//!     cx.bind_keys([
//!         binding,
//!         KeyBinding::new("ctrl-b", NoAction, Some("MarkdownEditor")),
//!     ]);
//! }
//! ```
//!
//! # Markdown round trip
//!
//! - **Lossless:** opening and saving without edits gives the same bytes,
//!   including line endings, trailing whitespace, a BOM and front matter.
//!   This is tested on the CommonMark and GFM spec examples and a corpus of
//!   notes.
//! - **Unknown syntax is kept:** constructs the editor does not model (HTML
//!   blocks, footnotes, math, definition lists) are
//!   raw blocks or raw spans, shown as their source and written back
//!   unchanged.
//! - **Edited blocks are normalized**, unedited neighbours are not: ATX
//!   headings, `-` bullets (an edited item keeps its list's marker), `1.`
//!   numbers, `- [ ]` / `- [x]` tasks, `**bold**`, `*italic*`,
//!   `~~strike~~`, `` `code` ``, `==highlight==`, `<u>underline</u>`,
//!   fenced code, padded and aligned tables, one blank line between blocks.
//!   Characters are escaped only where they would be read as syntax. See
//!   [`Document::to_markdown`].
//! - **Undo is exact:** undoing every edit gives back the loaded text byte
//!   for byte.
//! - An empty paragraph between blocks is written as `&#32;` (Markdown has
//!   no empty paragraph), and an edited hard-wrapped paragraph is written on
//!   one line.
//!
//! # Headless use
//!
//! [`Document`] needs no window: parse a note, read its outline, open tasks
//! or tags, and write it back. See `examples/headless.rs`.
//!
//! # Motion
//!
//! Popups fade and slide in over 250 ms, and a pressed checkbox springs to
//! 85 % of its size. Both follow GPUI's reduced-motion switch: with
//! [`App::set_reduce_motion`](gpui::App::set_reduce_motion) set, they show
//! their end state at once. While nothing animates, no frames are requested.
//!
//! # Known limits (0.1)
//!
//! - GPUI exposes no HTML clipboard format, so paste never sees rich text;
//!   [`markdown::html_to_markdown`] is there for hosts that get HTML
//!   elsewhere.
//! - Accessibility: the document, blocks and popups are AccessKit nodes with
//!   roles, but the caret and the selected text are not reported.
//! - Folding and block drag work on top-level blocks only.
//! - Extensions add marks, not inline nodes or block renderers.
//! - Up and Down skip rules and images (Left and Right select them).
//! - Enter in a quote starts a new quote paragraph rather than leaving it.
//! - Input methods are tested in simulation; dead keys, Compose and ibus
//!   have not been checked on every desktop.
//! - Performance: see `docs/performance.md` for the measured budgets.

#[doc(hidden)]
pub mod bench;
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
