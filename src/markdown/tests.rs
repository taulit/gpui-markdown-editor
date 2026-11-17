//! Tests of the serializer that edit the model directly: inserting, removing
//! and retyping blocks, which the public API cannot do yet.

use std::ops::Range;

use crate::extension::Extensions;
use crate::extensions::{Highlight, Tag, WikiLink};
use crate::model::{Block, BlockKind, Document, Inline, Mark, MarkSpan};

fn extensions() -> Extensions {
    Extensions::new(vec![Box::new(WikiLink), Box::new(Tag), Box::new(Highlight)])
}

fn parse(source: &str) -> Document {
    Document::parse_with(source, extensions())
}

fn new_block(document: &mut Document, kind: BlockKind, text: &str) -> Block {
    Block {
        id: document.alloc_id(),
        kind,
        content: Inline::plain(text),
        children: Vec::new(),
        origin: None,
        dirty: true,
    }
}

fn insert(document: &mut Document, index: usize, kind: BlockKind, text: &str) {
    let block = new_block(document, kind, text);
    document.blocks.insert(index, block);
}

/// Changes the kind of a block the way an editor command would.
fn retype(document: &mut Document, index: usize, kind: BlockKind) {
    let block = &mut document.blocks[index];
    block.kind = kind;
    block.dirty = true;
}

fn span(range: Range<usize>, mark: Mark) -> MarkSpan {
    MarkSpan { range, mark }
}

/// Writes a paragraph with the given marks and returns the Markdown.
fn written(text: &str, marks: Vec<MarkSpan>) -> String {
    let mut document = parse("");
    let mut block = new_block(&mut document, BlockKind::Paragraph, text);
    block.content.marks = marks;
    document.blocks.push(block);
    let markdown = document.to_markdown();
    markdown.strip_suffix('\n').unwrap_or(&markdown).to_string()
}

/// Parses `markdown` and returns the text and marks of its first block.
fn read(markdown: &str) -> (String, Vec<MarkSpan>) {
    let document = parse(markdown);
    let content = &document.blocks[0].content;
    (content.text.clone(), content.marks.clone())
}

#[test]
fn unedited_document_is_not_modified() {
    let document = parse("# a\n\n- b\n- c\n");
    assert!(!document.is_modified());
    assert_eq!(document.to_markdown(), "# a\n\n- b\n- c\n");
}

#[test]
fn all_dirty_matches_normalized() {
    let source = "Title\n=====\n* a\n* b\n\n1) x\n\n> q\n";
    let mut document = parse(source);
    document.mark_all_dirty();
    // Edited blocks of an existing file keep the blank lines of the source
    // and the markers of their lists.
    assert_eq!(document.to_markdown(), "# Title\n* a\n* b\n\n1) x\n\n> q\n");
    assert_eq!(
        document.normalized(),
        "# Title\n\n- a\n- b\n\n1. x\n\n> q\n"
    );
}

#[test]
fn new_item_takes_the_marker_of_the_list() {
    let mut document = parse("* a\n* b\n");
    insert(&mut document, 1, BlockKind::BulletItem, "new");
    assert_eq!(document.to_markdown(), "* a\n* new\n* b\n");

    let mut document = parse("text\n\n+ a\n+ b\n");
    insert(
        &mut document,
        1,
        BlockKind::TaskItem { checked: false },
        "first",
    );
    assert_eq!(document.to_markdown(), "text\n\n+ [ ] first\n+ a\n+ b\n");

    let mut document = parse("1) a\n2) b\n");
    insert(&mut document, 2, BlockKind::OrderedItem { number: 3 }, "c");
    assert_eq!(document.to_markdown(), "1) a\n2) b\n3) c\n");
}

#[test]
fn appended_blocks_follow_the_style_of_the_file() {
    let mut document = parse("a\r\n\r\nb\r\n");
    insert(&mut document, 2, BlockKind::Paragraph, "c\nd");
    assert_eq!(document.to_markdown(), "a\r\n\r\nb\r\n\r\nc\\\r\nd\r\n");

    let mut document = parse("a");
    insert(&mut document, 1, BlockKind::Heading(2), "b");
    assert_eq!(document.to_markdown(), "a\n\n## b");

    let mut document = parse("");
    insert(&mut document, 0, BlockKind::Paragraph, "first");
    assert_eq!(document.to_markdown(), "first\n");
}

#[test]
fn removed_blocks_leave_a_normal_gap() {
    let mut document = parse("a\n\n\n\nb\n\n\n\nc\n");
    document.blocks.remove(1);
    assert!(document.is_modified());
    assert_eq!(document.to_markdown(), "a\n\nc\n");

    let mut document = parse("\u{feff}a\n\nb\n");
    document.blocks.clear();
    assert_eq!(document.to_markdown(), "\u{feff}");
}

#[test]
fn moved_blocks_are_copied() {
    let mut document = parse("* a\n\n__b__\n\n1) c\n");
    document.blocks.swap(0, 2);
    assert_eq!(document.to_markdown(), "1) c\n\n__b__\n\n* a\n");
}

#[test]
fn retyped_block_gets_the_gap_it_needs() {
    // An item that becomes a paragraph must not continue the item above.
    let mut document = parse("- a\n- b\n- c\n");
    retype(&mut document, 1, BlockKind::Paragraph);
    // A list may directly follow a paragraph.
    assert_eq!(document.to_markdown(), "- a\n\nb\n- c\n");

    // A heading that becomes a paragraph must not merge with the text below.
    let mut document = parse("# a\ntext\n");
    retype(&mut document, 0, BlockKind::Paragraph);
    assert_eq!(document.to_markdown(), "a\n\ntext\n");

    // A setext heading below an edited paragraph would swallow it. It was
    // read in the context of the line above, so it is written again too.
    let mut document = parse("# a\nb\n===\n");
    retype(&mut document, 0, BlockKind::Paragraph);
    assert_eq!(document.to_markdown(), "a\n\n# b\n");

    // Text below a link reference definition can look like a block.
    let mut document = parse("[a]: /u\n2) text\n");
    document.blocks[0].dirty = true;
    assert_eq!(document.to_markdown(), "[a]: /u\n\n2\\) text\n");
}

#[test]
fn indented_block_behind_a_new_item_is_rewritten() {
    // `  text` is a paragraph of its own here, but would be part of a list
    // item written in front of it.
    let mut document = parse("a\n\n  text\n\n  - item\n");
    retype(&mut document, 0, BlockKind::BulletItem);
    let markdown = document.to_markdown();
    assert_eq!(markdown, "- a\n\ntext\n\n  - item\n");
    let reparsed = parse(&markdown);
    assert_eq!(reparsed.blocks.len(), 3);
    assert!(reparsed.blocks[0].children.is_empty());
}

#[test]
fn unclosed_fence_is_closed_when_something_follows() {
    let mut document = parse("```\ncode\n");
    insert(&mut document, 1, BlockKind::Paragraph, "after");
    assert_eq!(document.to_markdown(), "```\ncode\n```\n\nafter\n");
}

#[test]
fn first_rule_does_not_become_front_matter() {
    let mut document = parse("***\ntitle\n***\n");
    retype(&mut document, 0, BlockKind::Rule);
    let markdown = document.to_markdown();
    assert_eq!(markdown, "---\n\ntitle\n***\n");
    assert!(matches!(parse(&markdown).blocks[0].kind, BlockKind::Rule));
}

#[test]
fn ordered_numbers_survive() {
    let mut document = parse("");
    for number in [1, 2, 1, 7, 8] {
        let index = document.blocks.len();
        insert(&mut document, index, BlockKind::OrderedItem { number }, "x");
    }
    let markdown = document.to_markdown();
    assert_eq!(markdown, "1. x\n2. x\n\n1) x\n\n7. x\n8. x\n");
    let numbers: Vec<BlockKind> = parse(&markdown)
        .blocks
        .iter()
        .map(|b| b.kind.clone())
        .collect();
    let expected: Vec<BlockKind> = [1, 2, 1, 7, 8]
        .into_iter()
        .map(|number| BlockKind::OrderedItem { number })
        .collect();
    assert_eq!(numbers, expected);
}

#[test]
fn children_are_indented_by_the_marker_width() {
    let mut document = parse("");
    let mut item = new_block(&mut document, BlockKind::OrderedItem { number: 10 }, "ten");
    let child = new_block(&mut document, BlockKind::BulletItem, "child");
    let para = new_block(&mut document, BlockKind::Paragraph, "para\nsecond line");
    let code = new_block(&mut document, BlockKind::CodeBlock { lang: None }, "a\n\nb");
    item.children = vec![child, para, code];
    document.blocks.push(item);
    assert_eq!(
        document.to_markdown(),
        "10. ten\n    - child\n\n    para\\\n    second line\n\n    ```\n    a\n\n    b\n    ```\n"
    );
}

#[test]
fn empty_blocks() {
    let mut document = parse("");
    insert(&mut document, 0, BlockKind::BulletItem, "");
    insert(&mut document, 1, BlockKind::TaskItem { checked: true }, "");
    insert(&mut document, 2, BlockKind::Heading(3), "");
    insert(&mut document, 3, BlockKind::Quote, "");
    insert(&mut document, 4, BlockKind::CodeBlock { lang: None }, "");
    // The space behind the box of an empty task is needed at the end of a file.
    assert_eq!(
        document.to_markdown(),
        "-\n- [x] \n\n###\n\n>\n\n```\n```\n"
    );
}

#[test]
fn overlapping_marks_are_split() {
    let markdown = written(
        "one two three",
        vec![span(0..7, Mark::Bold), span(4..13, Mark::Italic)],
    );
    assert_eq!(markdown, "**one *two*** *three*");
    let (text, marks) = read(&markdown);
    assert_eq!(text, "one two three");
    // The space where bold ends cannot be italic: `* three*` is no emphasis.
    assert_eq!(
        marks,
        vec![
            span(0..7, Mark::Bold),
            span(4..7, Mark::Italic),
            span(8..13, Mark::Italic)
        ]
    );
}

#[test]
fn whitespace_leaves_emphasis() {
    assert_eq!(written("a b c", vec![span(1..4, Mark::Bold)]), "a **b** c");
    assert_eq!(written("a   c", vec![span(1..4, Mark::Italic)]), "a   c");
    assert_eq!(
        written(
            "a b c",
            vec![span(1..4, Mark::Bold), span(1..4, Mark::Strike)]
        ),
        "a **~~b~~** c"
    );
    assert_eq!(written("a\nb", vec![span(0..2, Mark::Bold)]), "**a**\\\nb");
    // Links and underline may hold whitespace at their edges.
    assert_eq!(
        written(" a ", vec![span(0..3, Mark::Underline)]),
        "<u> a </u>"
    );
}

#[test]
fn marks_around_atoms() {
    let link = Mark::Link {
        url: "u".into(),
        title: None,
    };
    assert_eq!(
        written(
            "see code here",
            vec![span(0..13, link), span(4..8, Mark::Code)]
        ),
        "[see `code` here](u)"
    );
    // A mark cannot end inside a code span.
    assert_eq!(
        written(
            "ab code",
            vec![span(0..5, Mark::Bold), span(3..7, Mark::Code)]
        ),
        "**ab** `code`"
    );
    assert_eq!(written("a|b", vec![span(0..3, Mark::Raw)]), "a|b");
}

#[test]
fn highlight_is_written_per_run() {
    assert_eq!(
        written("a b c", vec![span(2..3, Mark::Highlight)]),
        "a ==b== c"
    );
    let markdown = written(
        "a b c",
        vec![span(0..5, Mark::Highlight), span(2..3, Mark::Bold)],
    );
    assert_eq!(markdown, "==a== **==b==** ==c==");
    let (text, marks) = read(&markdown);
    assert_eq!(text, "a b c");
    assert_eq!(
        marks.iter().filter(|m| m.mark == Mark::Highlight).count(),
        3
    );
    // Text that would end the highlight early is written plain.
    assert_eq!(written("a==b", vec![span(0..4, Mark::Highlight)]), "a==b");
}

#[test]
fn custom_marks_use_their_extension() {
    let wiki = Mark::Custom {
        id: WikiLink::MARK,
        attr: Some("Target".into()),
    };
    let tag = Mark::Custom {
        id: Tag::MARK,
        attr: None,
    };
    assert_eq!(
        written(
            "see alias and #tag",
            vec![span(4..9, wiki), span(14..18, tag.clone())]
        ),
        "see [[Target|alias]] and #tag"
    );
    // A tag that is not valid as a tag falls back to plain text.
    assert_eq!(written("#a b", vec![span(0..4, tag)]), "\\#a b");
}

#[test]
fn literal_text_that_looks_like_extension_syntax() {
    assert_eq!(
        written("#tag [[x]] ==y==", vec![]),
        "\\#tag \\[[x]] \\==y=="
    );
    let (text, marks) = read("\\#tag \\[[x]] \\==y==");
    assert_eq!(text, "#tag [[x]] ==y==");
    assert!(marks.is_empty());
}

#[test]
#[allow(clippy::reversed_empty_ranges)] // a span an editor bug could produce
fn broken_spans_do_not_panic() {
    let text = "aä€b";
    let marks = vec![
        span(2..4, Mark::Bold),
        span(0..100, Mark::Italic),
        span(50..60, Mark::Code),
        span(3..2, Mark::Strike),
        span(1..3, Mark::Code),
        span(2..6, Mark::Raw),
    ];
    let markdown = written(text, marks);
    assert!(markdown.contains('€'));
}

#[test]
fn edge_whitespace_and_breaks() {
    assert_eq!(written(" a ", vec![]), "&#32;a&#32;");
    // A hard break can start a paragraph, but not end it.
    assert_eq!(written("\na\n", vec![]), "\\\na");
    assert_eq!(written("a\n b", vec![]), "a\\\n&#32;b");
    let (text, _) = read("&#32;a&#32;");
    assert_eq!(text, " a ");
}

#[test]
fn escaped_text_is_not_extension_syntax() {
    let (text, marks) = read("\\#tag and &#35;tag and #real");
    assert_eq!(text, "#tag and #tag and #real");
    assert_eq!(marks.len(), 1);
    assert_eq!(marks[0].range, 18..23);
}

#[test]
fn underline_tags() {
    let (text, marks) = read("a <u>b *c*</u> d <u>open and </u></u>");
    assert_eq!(text, "a b c d open and </u>");
    assert!(marks.contains(&span(2..5, Mark::Underline)));
    assert!(marks.contains(&span(4..5, Mark::Italic)));
    assert!(marks.contains(&span(8..17, Mark::Underline)));
    // A closing tag without an opening one stays as written.
    assert!(marks.contains(&span(17..21, Mark::Raw)));
    let (text, marks) = read("<u>never closed");
    assert_eq!(text, "<u>never closed");
    assert_eq!(marks, vec![span(0..3, Mark::Raw)]);
}

#[test]
fn ids_are_unique_and_continue() {
    let mut document = parse("- a\n  - b\n\n| x | y |\n|---|---|\n| 1 | 2 |\n");
    let mut ids = Vec::new();
    fn collect(blocks: &[Block], ids: &mut Vec<u64>) {
        for block in blocks {
            ids.push(block.id.0);
            if let BlockKind::Table(table) = &block.kind {
                ids.extend(table.rows.iter().flatten().map(|cell| cell.id.0));
            }
            collect(&block.children, ids);
        }
    }
    collect(&document.blocks, &mut ids);
    ids.sort_unstable();
    assert_eq!(ids, (0..ids.len() as u64).collect::<Vec<_>>());
    assert_eq!(document.alloc_id().0, ids.len() as u64);
}

#[test]
fn loose_task_items() {
    let document = parse("- [ ] open\n\n- [x] done\n\n  more\n");
    assert_eq!(document.blocks.len(), 2);
    assert_eq!(
        document.blocks[0].kind,
        BlockKind::TaskItem { checked: false }
    );
    assert_eq!(document.blocks[0].content.text, "open");
    assert_eq!(
        document.blocks[1].kind,
        BlockKind::TaskItem { checked: true }
    );
    assert_eq!(document.blocks[1].children.len(), 1);
}

#[test]
fn empty_paragraphs_are_not_written() {
    // Between blocks of any kind, and inside a quote.
    let mut document = parse("a\n\n# b\n");
    insert(&mut document, 1, BlockKind::Paragraph, "");
    insert(&mut document, 0, BlockKind::Paragraph, "");
    insert(&mut document, 4, BlockKind::Paragraph, "");
    assert_eq!(document.to_markdown(), "a\n\n# b\n");
    assert_eq!(document.normalized(), "a\n\n# b\n");

    // Two lists around an empty paragraph become one list.
    let mut document = parse("- a\n\nx\n\n- c\n");
    document.blocks[1].content = Inline::default();
    document.blocks[1].dirty = true;
    let markdown = document.to_markdown();
    assert_eq!(markdown, "- a\n- c\n");
    assert_eq!(parse(&markdown).blocks.len(), 2);

    let mut document = parse("> q\n");
    let child = new_block(&mut document, BlockKind::Paragraph, "");
    document.blocks[0].children.push(child);
    document.blocks[0].dirty = true;
    assert_eq!(document.to_markdown(), "> q\n");
}

#[test]
fn a_lone_space_entity_is_ordinary_text() {
    // Files written by 0.0.3 and 0.0.4 kept empty paragraphs as `&#32;`.
    let source = "a\n\n&#32;\n\nb\n";
    let document = parse(source);
    assert_eq!(document.blocks.len(), 3);
    assert_eq!(document.blocks[1].kind, BlockKind::Paragraph);
    assert_eq!(document.blocks[1].content.text, " ");
    assert_eq!(document.to_markdown(), source);
}
