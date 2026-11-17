//! Property tests: random text built from Markdown syntax fragments must
//! survive parsing and writing.

mod md_support;

use gpui_markdown_editor::Document;
use md_support::{dump, extensions};
use proptest::prelude::*;

/// Pieces of syntax and text that are glued together at random.
#[rustfmt::skip]
const FRAGMENTS: &[&str] = &[
    "# ", "## ", "###### ", "- ", "* ", "+ ", "1. ", "2) ", "10. ", "> ", "- [ ] ", "- [x] ",
    "    ", "  ", " ", "\t", "\n", "\n", "\n\n", "\r\n", "```\n", "```rust\n", "~~~\n", "`", "``",
    "*", "**", "_", "__", "~", "~~", "==", "[", "]", "(", ")", "![", "[[", "]]", "|", " | ",
    "---", "===", "***", "|---|---|\n", ":-:", "<u>", "</u>", "<div>", "</div>", "<!--", "-->",
    "<b>", "&amp;", "&#35;", "&nbsp;", "\\", "\\\n", "  \n", " #tag", "#", "[^1]", "[^1]: ",
    "[a]: /u\n", "[a]", "(/url)", "(/url \"t\")", "$", "$$", "!", ":", ": ", "http://x.y",
    "<http://x.y>", "<a@b.c>", "\u{feff}", "word", "two words", "a", "b", "1", "ä", "日本", "🎉",
    "> [!note]\n", "---\ntitle: x\n---\n",
];

fn markdown() -> impl Strategy<Value = String> {
    prop::collection::vec(0..FRAGMENTS.len(), 0..48)
        .prop_map(|picks| picks.into_iter().map(|index| FRAGMENTS[index]).collect())
}

/// Fragments for the properties that compare models.
///
/// Emphasis, code and tags come as complete spans with a space in front. A
/// soup of single delimiters (`***_**~**x***`) and spans that touch
/// (`_a_***b*****c**`, `` `a```b`` ``, `_a_#tag`) parse into marks that
/// overlap or meet in ways the writer, using only `*` and `**`, cannot always
/// reproduce.
///
/// Footnote references, `:` at the start of a line (definition lists) and
/// HTML comments are left out as well. They are kept as raw source, and what
/// they mean depends on their surroundings in ways the writer does not
/// track: a comment below a link reference definition is text, on its own it
/// is an HTML block.
#[rustfmt::skip]
const WELL_FORMED: &[&str] = &[
    "# ", "## ", "###### ", "- ", "* ", "+ ", "1. ", "2) ", "10. ", "> ", "- [ ] ", "- [x] ",
    "    ", "  ", " ", "\t", "\n", "\n", "\n\n", "\r\n", "```\n", "```rust\n", "~~~\n", " `code`",
    " ``a`b``", " **bold**", " *italic*", " __bold__", " _italic_", " ~~strike~~", " **a *b* c**",
    " *a **b** c*", " ***both***", "==mark==", "<u>under</u>", "[", "]", "(", ")", "[text](/url)",
    "[text](/url \"t\")", "![alt](img.png)", "![alt|200](img.png)", "[[Note]]", "[[Note|alias]]",
    "![[embed]]", "|", " | ", "---", "===", "* * *", "|---|---|\n", "|:-:|--:|\n", "<div>",
    "</div>", "<b>", "&amp;", "&#35;", "\\", "\\\n", "  \n", " #tag", " #", "[^1]: ", "[a]: /u\n",
    "[a]", "(/url)", "$x$", "$", "!", "http://x.y", "<http://x.y>", "<a@b.c>", "\u{feff}", "word",
    "two words", "a", "b", "1", "ä", "日本", "🎉", "> [!note]\n", "---\ntitle: x\n---\n", "\\*",
    "\\_", "\\#", "\\[", "2 * 3", "a_b",
];

fn well_formed_markdown() -> impl Strategy<Value = String> {
    prop::collection::vec(0..WELL_FORMED.len(), 0..48)
        .prop_map(|picks| picks.into_iter().map(|index| WELL_FORMED[index]).collect())
}

/// More cases than the default of 256, unless `PROPTEST_CASES` says otherwise.
fn config() -> ProptestConfig {
    let mut config = ProptestConfig::default();
    if std::env::var_os("PROPTEST_CASES").is_none() {
        config.cases = 2000;
    }
    config
}

proptest! {
    #![proptest_config(config())]

    /// Parsing and writing without an edit gives back the input.
    #[test]
    fn unedited_text_is_written_byte_identically(source in markdown()) {
        for exts in [Default::default(), extensions()] {
            let document = Document::parse_with(&source, exts);
            prop_assert_eq!(document.to_markdown(), source.as_str());
            prop_assert!(!document.is_modified());
            let mut cursor = 0;
            for block in document.blocks() {
                let origin = block.origin().expect("parsed blocks have an origin");
                prop_assert!(origin.start >= cursor && origin.end <= source.len());
                prop_assert!(source.is_char_boundary(origin.start) && source.is_char_boundary(origin.end));
                cursor = origin.end;
            }
        }
    }

    /// Any text can be parsed and written in the normalized style.
    #[test]
    fn any_text_can_be_normalized(source in prop_oneof!["\\PC{0,64}", markdown()]) {
        let document = Document::parse_with(&source, extensions());
        prop_assert_eq!(document.to_markdown(), source.as_str());
        let normalized = document.normalized();
        Document::parse_with(&normalized, extensions()).normalized();
    }

    /// Writing in the normalized style keeps the meaning and is stable.
    #[test]
    fn normalized_text_keeps_its_model(source in well_formed_markdown()) {
        let original = Document::parse_with(&source, extensions());
        let first = original.normalized();
        let reparsed = Document::parse_with(&first, extensions());
        prop_assert_eq!(dump(&reparsed), dump(&original), "normalized: {:?}", first);
        prop_assert_eq!(reparsed.normalized(), first);
    }

    /// Rewriting one block does not change what the other blocks mean.
    #[test]
    fn one_edited_block_keeps_the_model(source in well_formed_markdown(), pick in any::<prop::sample::Index>()) {
        let mut document = Document::parse_with(&source, extensions());
        if document.blocks().is_empty() {
            return Ok(());
        }
        let before = dump(&document);
        let id = document.blocks()[pick.index(document.blocks().len())].id();
        prop_assert!(document.mark_dirty(id));
        let written = document.to_markdown();
        let reparsed = Document::parse_with(&written, extensions());
        prop_assert_eq!(dump(&reparsed), before, "written: {:?}", written);
    }
}
