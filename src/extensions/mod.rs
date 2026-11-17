//! Extensions shipped with the crate. All are optional.

use crate::extension::Extension;
use crate::model::CustomMarkId;

/// Wiki links: `[[Note]]`, `[[Note|Alias]]`.
///
/// The mark covers the displayed text. Its `attr` holds the target when it
/// differs from the displayed text.
#[derive(Clone, Copy, Debug, Default)]
pub struct WikiLink;

impl WikiLink {
    /// The mark id used for wiki links.
    pub const MARK: CustomMarkId = CustomMarkId("wikilink");
}

impl Extension for WikiLink {
    fn name(&self) -> &'static str {
        "wikilink"
    }
}

/// Tags: `#tag`, `#project/sub`.
///
/// The mark covers the tag including the `#`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Tag;

impl Tag {
    /// The mark id used for tags.
    pub const MARK: CustomMarkId = CustomMarkId("tag");
}

impl Extension for Tag {
    fn name(&self) -> &'static str {
        "tag"
    }
}

/// Highlighted text: `==text==`, stored as [`Mark::Highlight`](crate::Mark::Highlight).
#[derive(Clone, Copy, Debug, Default)]
pub struct Highlight;

impl Extension for Highlight {
    fn name(&self) -> &'static str {
        "highlight"
    }
}
