//! Configuration of a [`MarkdownEditor`], fixed at creation.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use gpui::{Context, Pixels, SharedString, Window, px};

use crate::editor::MarkdownEditor;
use crate::extension::Extension;

/// The callback type of [`EditorOptions::resolve_src`]: maps a source as
/// written in the Markdown to a file on disk.
pub type SrcResolver = Arc<dyn Fn(&str) -> Option<PathBuf>>;

/// The callback type of [`EditorOptions::on_image`]: receives image bytes and
/// their MIME type and returns the source to write into the Markdown.
pub type ImageHandler = Arc<dyn Fn(Vec<u8>, &str) -> Option<String>>;

/// The callback type of [`EditorOptions::completions`]: receives what is being
/// completed and the text typed so far and returns the suggestions.
pub type CompletionProvider = Arc<dyn Fn(CompletionKind, &str) -> Vec<Completion>>;

/// The callback type of [`SlashItem::on_select`].
pub type SlashHandler = Arc<dyn Fn(&mut MarkdownEditor, &mut Window, &mut Context<MarkdownEditor>)>;

/// Configuration passed to [`MarkdownEditor::new`].
///
/// Start from [`EditorOptions::default`] and override what you need:
///
/// ```
/// use gpui_markdown_editor::extensions::{Tag, WikiLink};
/// use gpui_markdown_editor::EditorOptions;
///
/// let options = EditorOptions {
///     first_line_is_title: true,
///     extensions: vec![Box::new(WikiLink), Box::new(Tag)],
///     ..Default::default()
/// };
/// assert!(options.ui.format_bar);
/// ```
pub struct EditorOptions {
    /// Shows the first block as the title (like a level 1 heading) even when
    /// it is a plain paragraph, as Apple Notes does. Display only: the
    /// Markdown is not changed. Default `false`.
    pub first_line_is_title: bool,
    /// The maximum width of the content column. The column is centered in the
    /// editor. `None` uses the full width. Default 700 px.
    pub max_width: Option<Pixels>,
    /// Custom inline syntax. Earlier extensions win when matches overlap.
    /// The crate ships [`WikiLink`](crate::extensions::WikiLink),
    /// [`Tag`](crate::extensions::Tag) and
    /// [`Highlight`](crate::extensions::Highlight). Default: none.
    pub extensions: Vec<Box<dyn Extension>>,
    /// Maps an image or attachment source as written in the Markdown (usually
    /// a relative path) to a file on disk. Return `None` if the file is
    /// missing. Without a resolver only absolute paths and URLs are shown.
    pub resolve_src: Option<SrcResolver>,
    /// Stores image data that was pasted or dropped into the editor. Receives
    /// the bytes and their MIME type (for example `image/png`) and returns the
    /// source to write into the Markdown, or `None` to reject the image.
    /// Without a handler pasted images are ignored.
    pub on_image: Option<ImageHandler>,
    /// Supplies suggestions while the user types a wiki link (`[[`) or a tag
    /// (`#`). Receives what is being completed and the text typed so far.
    /// Called on every keystroke, so it should return quickly.
    pub completions: Option<CompletionProvider>,
    /// Entries added to the slash menu after the built-in block types, for
    /// example "Insert template".
    pub slash_items: Vec<SlashItem>,
    /// Which built-in UI parts are shown. Default: all.
    pub ui: UiParts,
}

impl Default for EditorOptions {
    fn default() -> Self {
        Self {
            first_line_is_title: false,
            max_width: Some(px(700.)),
            extensions: Vec::new(),
            resolve_src: None,
            on_image: None,
            completions: None,
            slash_items: Vec::new(),
            ui: UiParts::default(),
        }
    }
}

impl fmt::Debug for EditorOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EditorOptions")
            .field("first_line_is_title", &self.first_line_is_title)
            .field("max_width", &self.max_width)
            .field(
                "extensions",
                &self.extensions.iter().map(|e| e.name()).collect::<Vec<_>>(),
            )
            .field("resolve_src", &self.resolve_src.is_some())
            .field("on_image", &self.on_image.is_some())
            .field("completions", &self.completions.is_some())
            .field("slash_items", &self.slash_items)
            .field("ui", &self.ui)
            .finish()
    }
}

/// Switches for the UI parts the editor can show on its own.
///
/// Turn a part off to replace it with your own UI, driven by
/// [`MarkdownEditor::command`]. The parts need the cargo feature `ui`
/// (enabled by default). Without it these switches have no effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiParts {
    /// The floating toolbar at the bottom that adapts to the selection.
    pub format_bar: bool,
    /// The block menu that opens when `/` is typed at the start of a line or
    /// after a space.
    pub slash_menu: bool,
    /// The search bar, opened with Ctrl+F or [`Command::OpenFind`](crate::Command::OpenFind).
    pub find_bar: bool,
    /// The popover to edit, open or remove the link at the cursor.
    pub link_popover: bool,
}

impl UiParts {
    /// All parts enabled. Same as [`UiParts::default`].
    pub const ALL: Self = Self {
        format_bar: true,
        slash_menu: true,
        find_bar: true,
        link_popover: true,
    };

    /// All parts disabled, for hosts that bring their own UI.
    pub const NONE: Self = Self {
        format_bar: false,
        slash_menu: false,
        find_bar: false,
        link_popover: false,
    };
}

impl Default for UiParts {
    fn default() -> Self {
        Self::ALL
    }
}

/// A host-defined entry of the slash menu.
///
/// ```
/// use gpui_markdown_editor::{Command, SlashItem};
///
/// let item = SlashItem::new("meeting", "Meeting notes", |editor, window, cx| {
///     let template = "## Attendees\n\n## Decisions\n".to_string();
///     editor.command(Command::InsertMarkdown(template), window, cx);
/// })
/// .description("Insert the meeting template")
/// .keywords(["template", "protocol"]);
/// assert_eq!(item.label, "Meeting notes");
/// ```
#[derive(Clone)]
pub struct SlashItem {
    /// A unique, stable identifier.
    pub id: SharedString,
    /// The text shown in the menu.
    pub label: SharedString,
    /// A second, quieter line below the label.
    pub description: Option<SharedString>,
    /// Further words the entry is found by when the user filters the menu.
    pub keywords: Vec<SharedString>,
    /// Called when the entry is chosen. The typed `/query` is already removed
    /// and the cursor is where it was.
    pub on_select: SlashHandler,
}

impl SlashItem {
    /// Creates an entry without description and keywords.
    pub fn new(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        on_select: impl Fn(&mut MarkdownEditor, &mut Window, &mut Context<MarkdownEditor>) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: None,
            keywords: Vec::new(),
            on_select: Arc::new(on_select),
        }
    }

    /// Sets the description.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the keywords.
    pub fn keywords<I>(mut self, keywords: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<SharedString>,
    {
        self.keywords = keywords.into_iter().map(Into::into).collect();
        self
    }
}

impl fmt::Debug for SlashItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SlashItem")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("description", &self.description)
            .field("keywords", &self.keywords)
            .finish_non_exhaustive()
    }
}

/// What the user is completing, passed to
/// [`EditorOptions::completions`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CompletionKind {
    /// The target of a wiki link, after `[[`.
    WikiLink,
    /// A tag, after `#`.
    Tag,
}

/// One suggestion returned by [`EditorOptions::completions`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    /// The text shown in the list.
    pub label: SharedString,
    /// Quieter text next to the label, for example a folder or a count.
    pub detail: Option<SharedString>,
    /// What is inserted: the link target or the tag name, without `[[`, `]]`
    /// or `#`.
    pub insert: SharedString,
}

impl Completion {
    /// Creates a suggestion that inserts its label.
    pub fn new(label: impl Into<SharedString>) -> Self {
        let label = label.into();
        Self {
            insert: label.clone(),
            detail: None,
            label,
        }
    }
}
