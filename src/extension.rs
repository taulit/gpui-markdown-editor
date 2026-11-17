//! Extension points for custom inline syntax.
//!
//! An [`Extension`] teaches the editor about inline syntax that is not part
//! of CommonMark or GFM: how to find it in text and write it back, how its
//! marks look, what typing completes it, what a click on it reports, and
//! which entries it adds to the slash menu. The crate ships [`WikiLink`],
//! [`Tag`] and [`Highlight`](crate::extensions::Highlight).
//!
//! All methods have defaults, so new hooks do not break implementors.
//!
//! [`WikiLink`]: crate::extensions::WikiLink
//! [`Tag`]: crate::extensions::Tag

use std::fmt;
use std::ops::Range;
use std::sync::Arc;

use gpui::{FontWeight, Hsla};

use crate::event::EditorEvent;
use crate::model::{CustomMarkId, Mark};

/// Custom inline syntax: how to find it in text and how to write it back.
pub trait Extension: Send + Sync + 'static {
    /// A unique, stable name, for example `"wikilink"`.
    fn name(&self) -> &'static str;

    /// Finds occurrences of this extension's syntax in a run of plain text.
    ///
    /// `text` is literal text of one inline run: it never contains code spans,
    /// links, raw HTML or other Markdown syntax. Matches must be sorted, must
    /// not overlap and must lie on `char` boundaries.
    fn parse_inline(&self, text: &str) -> Vec<InlineMatch> {
        let _ = text;
        Vec::new()
    }

    /// Writes a mark created by this extension back to Markdown.
    ///
    /// `text` is the text covered by the mark. Return `None` for marks that
    /// belong to other extensions.
    fn serialize_mark(&self, mark: &Mark, text: &str) -> Option<String> {
        let _ = (mark, text);
        None
    }

    /// How text with this extension's custom mark `id` is drawn. `None`
    /// (the default) draws it like the surrounding text.
    fn mark_style(&self, id: CustomMarkId) -> Option<MarkStyle> {
        let _ = id;
        None
    }

    /// An input rule: called after the user typed text, with the text of
    /// the block before the caret (`before`, outside code). Return a match
    /// within `before` to turn that range into marked text, for example
    /// when the closing `]]` of a wiki link was typed. The match must end
    /// at most at `before.len()` and lie on `char` boundaries. The default
    /// has no rule.
    fn input_rule(&self, before: &str) -> Option<InlineMatch> {
        let _ = before;
        None
    }

    /// The event emitted when text with this extension's mark is clicked
    /// with the secondary modifier (Ctrl, Cmd on macOS). `text` is the
    /// marked text. Return `None` for marks of other extensions.
    fn click(&self, mark: &Mark, text: &str) -> Option<EditorEvent> {
        let _ = (mark, text);
        None
    }

    /// Entries this extension adds to the slash menu, after the built-in
    /// block types and before the host's
    /// [`slash_items`](crate::EditorOptions::slash_items).
    fn slash_items(&self) -> Vec<SlashEntry> {
        Vec::new()
    }
}

/// The look of a custom mark, returned by [`Extension::mark_style`].
///
/// ```
/// use gpui::FontWeight;
/// use gpui_markdown_editor::extension::{MarkColor, MarkStyle};
///
/// let style = MarkStyle::new()
///     .color(MarkColor::Tag)
///     .weight(FontWeight::SEMIBOLD);
/// assert!(!style.underline);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct MarkStyle {
    /// Text color.
    pub color: Option<MarkColor>,
    /// Background behind the text.
    pub background: Option<MarkColor>,
    /// Font weight, at least that of the surrounding text.
    pub weight: Option<FontWeight>,
    /// Italic text.
    pub italic: bool,
    /// Underlined text, in a light version of the text color.
    pub underline: bool,
}

impl MarkStyle {
    /// A style that changes nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the text color.
    pub fn color(mut self, color: MarkColor) -> Self {
        self.color = Some(color);
        self
    }

    /// Sets the background.
    pub fn background(mut self, color: MarkColor) -> Self {
        self.background = Some(color);
        self
    }

    /// Sets the font weight.
    pub fn weight(mut self, weight: FontWeight) -> Self {
        self.weight = Some(weight);
        self
    }

    /// Makes the text italic.
    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    /// Underlines the text.
    pub fn underline(mut self) -> Self {
        self.underline = true;
        self
    }
}

/// A color of a [`MarkStyle`]: one of the [`EditorStyle`](crate::EditorStyle)
/// colors, so that it follows the theme, or a fixed one.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum MarkColor {
    /// [`EditorStyle::link_color`](crate::EditorStyle::link_color).
    Link,
    /// [`EditorStyle::tag_color`](crate::EditorStyle::tag_color).
    Tag,
    /// [`EditorStyle::highlight_color`](crate::EditorStyle::highlight_color).
    Highlight,
    /// [`EditorStyle::muted_color`](crate::EditorStyle::muted_color).
    Muted,
    /// A fixed color.
    Fixed(Hsla),
}

/// A slash menu entry of an extension, see [`Extension::slash_items`].
/// Choosing it types `insert` at the caret (input rules apply).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlashEntry {
    /// A unique, stable identifier.
    pub id: &'static str,
    /// The text shown in the menu.
    pub label: &'static str,
    /// Further words the entry is found by.
    pub keywords: &'static [&'static str],
    /// The text typed when the entry is chosen.
    pub insert: &'static str,
}

/// One occurrence of custom inline syntax found by [`Extension::parse_inline`].
#[derive(Clone, Debug, PartialEq)]
pub struct InlineMatch {
    /// The byte range of the whole syntax in the scanned text, including
    /// delimiters. For `[[Note]]` this covers all eight bytes.
    pub range: Range<usize>,
    /// The text shown in the editor in place of the syntax, without
    /// delimiters. For `[[Note|Alias]]` this is `Alias`.
    pub text: String,
    /// The mark applied to `text`.
    pub mark: Mark,
}

/// A shared, immutable set of extensions.
#[derive(Clone, Default)]
pub struct Extensions(Arc<[Box<dyn Extension>]>);

impl Extensions {
    /// Creates a set from a list of extensions. Earlier extensions win when
    /// matches overlap.
    pub fn new(extensions: Vec<Box<dyn Extension>>) -> Self {
        Self(extensions.into())
    }

    /// Iterates over the extensions in registration order.
    pub fn iter(&self) -> impl Iterator<Item = &dyn Extension> {
        self.0.iter().map(|e| e.as_ref())
    }

    /// Returns `true` if no extension is registered.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Extensions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.iter().map(|e| e.name()))
            .finish()
    }
}

impl From<Vec<Box<dyn Extension>>> for Extensions {
    fn from(extensions: Vec<Box<dyn Extension>>) -> Self {
        Self::new(extensions)
    }
}
