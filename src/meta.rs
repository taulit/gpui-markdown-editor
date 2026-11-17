//! Presentation state of a document that is not part of its Markdown text.

use std::collections::{BTreeMap, BTreeSet};

/// Presentation state that Markdown cannot express: table column widths and
/// folded headings.
///
/// The editor never writes this into the Markdown text. The host stores it
/// next to the file (for example in a sidecar or a database) and hands it back
/// with [`MarkdownEditor::set_meta`](crate::MarkdownEditor::set_meta) after
/// loading. [`EditorEvent::MetaChanged`](crate::EditorEvent::MetaChanged)
/// tells the host when to store it again.
///
/// Entries are keyed by ordinal, not by [`BlockId`](crate::BlockId), because
/// block ids are not stable across loads. Entries that point past the end of
/// the document are ignored.
///
/// With the cargo feature `serde` this type implements `Serialize` and
/// `Deserialize`. Missing fields deserialize to their defaults, so stored
/// metadata stays readable when fields are added.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct DocMeta {
    /// Column widths in pixels per table. The key is the position of the table
    /// among all tables of the document in document order, starting at 0. The
    /// value holds one width per column. Tables without an entry size their
    /// columns automatically.
    pub column_widths: BTreeMap<usize, Vec<f32>>,
    /// The folded headings. Each entry is the position of a heading among all
    /// headings of the document in document order, starting at 0.
    pub folded_headings: BTreeSet<usize>,
}

impl DocMeta {
    /// Returns `true` if there is nothing worth storing.
    pub fn is_empty(&self) -> bool {
        self.column_widths.is_empty() && self.folded_headings.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_empty() {
        assert!(DocMeta::default().is_empty());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_round_trip() {
        let mut meta = DocMeta::default();
        meta.column_widths.insert(1, vec![120.0, 80.5]);
        meta.folded_headings.insert(3);

        let json = serde_json::to_string(&meta).unwrap();
        assert_eq!(serde_json::from_str::<DocMeta>(&json).unwrap(), meta);
        assert_eq!(
            serde_json::from_str::<DocMeta>("{}").unwrap(),
            DocMeta::default()
        );
    }
}
