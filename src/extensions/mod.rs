//! Extensions shipped with the crate. All are optional.
//!
//! ```
//! use gpui_markdown_editor::extensions::{Highlight, Tag, WikiLink};
//! use gpui_markdown_editor::{Document, Extensions, Mark};
//!
//! let extensions = Extensions::new(vec![Box::new(WikiLink), Box::new(Tag), Box::new(Highlight)]);
//! let doc = Document::parse_with("See [[Plan|the plan]] for #work, ==soon==.", extensions);
//! let content = doc.blocks()[0].content();
//! assert_eq!(content.text(), "See the plan for #work, soon.");
//! assert_eq!(content.marks().len(), 3);
//! assert_eq!(content.marks()[2].mark, Mark::Highlight);
//! ```

use gpui::SharedString;

use crate::extension::{Extension, InlineMatch};
use crate::model::{CustomMarkId, Mark};

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

    fn parse_inline(&self, text: &str) -> Vec<InlineMatch> {
        let mut matches = Vec::new();
        let mut from = 0;
        while let Some(found) = text[from..].find("[[") {
            let open = from + found;
            let inner_start = open + 2;
            let rest = &text[inner_start..];
            // The link ends at the first `]]`. A bracket or line break before
            // that means this is not a wiki link.
            let stop = rest.find(['[', ']', '\n']).unwrap_or(rest.len());
            let inner = &rest[..stop];
            if inner.is_empty() || !rest[stop..].starts_with("]]") {
                from = open + 1;
                continue;
            }
            let (target, shown) = match inner.split_once('|') {
                Some((target, alias)) if !alias.is_empty() => (target, alias),
                Some((target, _)) => (target, target),
                None => (inner, inner),
            };
            let end = inner_start + stop + 2;
            if !shown.is_empty() {
                matches.push(InlineMatch {
                    range: open..end,
                    text: shown.to_string(),
                    mark: Mark::Custom {
                        id: Self::MARK,
                        attr: (target != shown).then(|| SharedString::from(target.to_string())),
                    },
                });
            }
            from = end;
        }
        matches
    }

    fn serialize_mark(&self, mark: &Mark, text: &str) -> Option<String> {
        let Mark::Custom { id, attr } = mark else {
            return None;
        };
        if *id != Self::MARK {
            return None;
        }
        let breaks_syntax = |part: &str| part.contains(['[', ']', '\n']);
        match attr {
            Some(target)
                if !breaks_syntax(target) && !target.contains('|') && !breaks_syntax(text) =>
            {
                Some(format!("[[{target}|{text}]]"))
            }
            None if !breaks_syntax(text) && !text.contains('|') && !text.is_empty() => {
                Some(format!("[[{text}]]"))
            }
            _ => None,
        }
    }
}

/// Tags: `#tag`, `#project/sub`.
///
/// A tag starts at a word boundary and consists of letters, digits, `_`, `-`
/// and `/`, with at least one letter. The mark covers the tag including the
/// `#`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Tag;

impl Tag {
    /// The mark id used for tags.
    pub const MARK: CustomMarkId = CustomMarkId("tag");

    /// Returns the length of the tag that `text` starts with.
    fn tag_len(text: &str) -> Option<usize> {
        let body = text.strip_prefix('#')?;
        let len = body
            .char_indices()
            .find(|(_, c)| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '/')))
            .map_or(body.len(), |(index, _)| index);
        let body = body[..len].trim_end_matches(['/', '-']);
        body.chars()
            .any(char::is_alphabetic)
            .then_some(1 + body.len())
    }
}

impl Extension for Tag {
    fn name(&self) -> &'static str {
        "tag"
    }

    fn parse_inline(&self, text: &str) -> Vec<InlineMatch> {
        let mut matches = Vec::new();
        let mut from = 0;
        while let Some(found) = text[from..].find('#') {
            let start = from + found;
            // `a#b`, `##` and the fragment of a URL are not tags.
            let boundary = text[..start]
                .chars()
                .next_back()
                .is_none_or(|c| !(c.is_alphanumeric() || matches!(c, '#' | '/' | '_' | '-' | '&')));
            match Self::tag_len(&text[start..]).filter(|_| boundary) {
                Some(len) => {
                    matches.push(InlineMatch {
                        range: start..start + len,
                        text: text[start..start + len].to_string(),
                        mark: Mark::Custom {
                            id: Self::MARK,
                            attr: None,
                        },
                    });
                    from = start + len;
                }
                None => from = start + 1,
            }
        }
        matches
    }

    fn serialize_mark(&self, mark: &Mark, text: &str) -> Option<String> {
        match mark {
            Mark::Custom { id, .. } if *id == Self::MARK => {
                (Self::tag_len(text) == Some(text.len())).then(|| text.to_string())
            }
            _ => None,
        }
    }
}

/// Highlighted text: `==text==`, stored as [`Mark::Highlight`].
///
/// The highlighted text must not start or end with whitespace and cannot
/// contain `==` or other formatting. Writing `==text==` back is built into the
/// serializer and works without this extension.
#[derive(Clone, Copy, Debug, Default)]
pub struct Highlight;

impl Extension for Highlight {
    fn name(&self) -> &'static str {
        "highlight"
    }

    fn parse_inline(&self, text: &str) -> Vec<InlineMatch> {
        // Only runs of exactly two `=` are delimiters.
        let bytes = text.as_bytes();
        let mut delimiters = Vec::new();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != b'=' {
                index += 1;
                continue;
            }
            let len = bytes[index..].iter().take_while(|b| **b == b'=').count();
            if len == 2 {
                delimiters.push(index);
            }
            index += len;
        }
        let opens = |at: usize| {
            text[at + 2..]
                .chars()
                .next()
                .is_some_and(|c| !c.is_whitespace())
        };
        let closes = |at: usize| {
            text[..at]
                .chars()
                .next_back()
                .is_some_and(|c| !c.is_whitespace())
        };
        let mut matches = Vec::new();
        let mut next = 0;
        while next + 1 < delimiters.len() {
            let open = delimiters[next];
            if !opens(open) {
                next += 1;
                continue;
            }
            let close = delimiters[next + 1..]
                .iter()
                .position(|at| closes(*at) && !text[open + 2..*at].contains('\n'));
            match close {
                // A longer run of `=` inside would be ambiguous.
                Some(offset) if !text[open + 2..delimiters[next + 1 + offset]].contains("==") => {
                    let close_index = next + 1 + offset;
                    let close = delimiters[close_index];
                    matches.push(InlineMatch {
                        range: open..close + 2,
                        text: text[open + 2..close].to_string(),
                        mark: Mark::Highlight,
                    });
                    next = close_index + 1;
                }
                _ => next += 1,
            }
        }
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(extension: &dyn Extension, text: &str) -> Vec<(String, String)> {
        extension
            .parse_inline(text)
            .into_iter()
            .map(|m| (text[m.range].to_string(), m.text))
            .collect()
    }

    fn pair(a: &str, b: &str) -> (String, String) {
        (a.to_string(), b.to_string())
    }

    #[test]
    fn wiki_links() {
        assert_eq!(shown(&WikiLink, "a [[Note]] b"), [pair("[[Note]]", "Note")]);
        assert_eq!(
            shown(&WikiLink, "[[Note|Alias]][[Other#Part]]"),
            [
                pair("[[Note|Alias]]", "Alias"),
                pair("[[Other#Part]]", "Other#Part")
            ]
        );
        assert_eq!(shown(&WikiLink, "[[[x]]"), [pair("[[x]]", "x")]);
        assert_eq!(shown(&WikiLink, "[[]] [[a\nb]] [[a]"), []);
        let found = WikiLink.parse_inline("[[Target|shown]]");
        assert_eq!(
            found[0].mark,
            Mark::Custom {
                id: WikiLink::MARK,
                attr: Some("Target".into())
            }
        );
        assert_eq!(
            WikiLink.serialize_mark(&found[0].mark, "shown").as_deref(),
            Some("[[Target|shown]]")
        );
        let plain = Mark::Custom {
            id: WikiLink::MARK,
            attr: None,
        };
        assert_eq!(
            WikiLink.serialize_mark(&plain, "Note").as_deref(),
            Some("[[Note]]")
        );
        assert_eq!(WikiLink.serialize_mark(&plain, "a]b"), None);
        assert_eq!(WikiLink.serialize_mark(&Mark::Bold, "x"), None);
    }

    #[test]
    fn tags() {
        assert_eq!(
            shown(&Tag, "#tag and #project/sub."),
            [pair("#tag", "#tag"), pair("#project/sub", "#project/sub")]
        );
        assert_eq!(
            shown(&Tag, "#über #日本 (#a-b_c)"),
            [
                pair("#über", "#über"),
                pair("#日本", "#日本"),
                pair("#a-b_c", "#a-b_c")
            ]
        );
        assert_eq!(shown(&Tag, "a#b #123 # x ##y http://x/#frag"), []);
        assert_eq!(
            shown(&Tag, "#2024q1 #tag/"),
            [pair("#2024q1", "#2024q1"), pair("#tag", "#tag")]
        );
        let mark = Mark::Custom {
            id: Tag::MARK,
            attr: None,
        };
        assert_eq!(Tag.serialize_mark(&mark, "#tag").as_deref(), Some("#tag"));
        assert_eq!(Tag.serialize_mark(&mark, "#two words"), None);
    }

    #[test]
    fn highlights() {
        assert_eq!(shown(&Highlight, "a ==b c== d"), [pair("==b c==", "b c")]);
        assert_eq!(shown(&Highlight, "==a====b=="), []);
        assert_eq!(shown(&Highlight, "==a=b=="), [pair("==a=b==", "a=b")]);
        assert_eq!(
            shown(&Highlight, "==a== ==b=="),
            [pair("==a==", "a"), pair("==b==", "b")]
        );
        assert_eq!(shown(&Highlight, "== a== ==a == ===="), []);
        assert_eq!(shown(&Highlight, "x == y and ==z=="), [pair("==z==", "z")]);
    }
}
