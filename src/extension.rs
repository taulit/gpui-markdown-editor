//! Extension points for custom inline syntax.
//!
//! An [`Extension`] teaches the parser and serializer about inline syntax that
//! is not part of CommonMark or GFM. The crate ships [`WikiLink`], [`Tag`] and
//! [`Highlight`](crate::extensions::Highlight).
//!
//! The trait is still growing (rendering and input rule hooks land before
//! 0.1). All methods have defaults, so new hooks do not break implementors.
//!
//! [`WikiLink`]: crate::extensions::WikiLink
//! [`Tag`]: crate::extensions::Tag

use std::fmt;
use std::ops::Range;
use std::sync::Arc;

use crate::model::Mark;

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
