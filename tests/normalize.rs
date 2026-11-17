//! Snapshots of the normalized style that edited blocks are written in, and
//! of how a single edited block sits between untouched neighbours.

mod md_support;

use gpui_markdown_editor::{Document, Extensions};
use indoc::indoc;
use md_support::extensions;

/// Shows control characters that would be invisible in a snapshot.
fn visible(text: &str) -> String {
    text.replace('\r', "␍")
        .replace('\t', "→")
        .replace('\u{feff}', "[BOM]")
}

fn normalized_with(source: &str, extensions: Extensions) -> String {
    let document = Document::parse_with(source, extensions.clone());
    let normalized = document.normalized();
    let again = Document::parse_with(&normalized, extensions).normalized();
    assert_eq!(normalized, again, "normalizing is not stable");
    visible(&normalized)
}

fn normalized(source: &str) -> String {
    normalized_with(source, Extensions::default())
}

/// Marks the top-level blocks with the given indexes as edited and returns
/// the source and the result side by side.
fn edited(source: &str, indexes: &[usize]) -> String {
    let mut document = Document::parse_with(source, extensions());
    for index in indexes {
        let id = document.blocks()[*index].id();
        assert!(document.mark_dirty(id));
    }
    assert!(document.is_modified());
    let output = document.to_markdown();
    format!(
        "{}⏎\n======== edited {indexes:?}\n{}⏎\n",
        visible(source),
        visible(&output)
    )
}

#[test]
fn lists() {
    insta::assert_snapshot!(normalized(indoc! {"
        * star
        * star two
        + plus

        1) one
        2) two

        5. five
        7. six

        - loose

        - list

        -
        - after empty
    "}));
}

#[test]
fn nested_lists() {
    insta::assert_snapshot!(normalized(indoc! {"
        * one
            * nested deeply indented
                * third level
            * back

              paragraph in nested item
        * two
            1. ordered child
            2. second
        9. nine
        10. ten
            continuation line

            second paragraph of ten

                code in ten
        11. eleven
            > quote in eleven
    "}));
}

#[test]
fn tasks() {
    insta::assert_snapshot!(normalized(indoc! {"
        * [ ] open
        * [X] done with capital X
        + [x] done
            - [ ] nested open
        1. [ ] numbered task
        - [ ]
        - \\[ ] not a task
    "}));
}

#[test]
fn marks() {
    insta::assert_snapshot!(normalized(indoc! {"
        __bold__ _italic_ ___both___ ~strike~ ~~strike~~ `code` <u>under</u>

        **bold _nested italic_ bold** *italic **nested bold** italic*

        **bold***italic*~~strike~~`code` and *a***b***c*

        **bold `code` [link](url)** and [**bold link** *italic*](url 'title')

        <u>under **bold** *italic*</u> and **<u>both</u>**

        *italic
        over two lines* and <http://auto.link> and <me@example.com>
    "}));
}

#[test]
fn tables() {
    insta::assert_snapshot!(normalized(indoc! {"
        |left|center|right|none|
        |:-|:-:|-:|-|
        |a|b|c|d|
        |longer cell|x|1234567|`a\\|b`|
        |short||**bold**|

        Name | Wert
        --- | ---
        Größe | 日本語
        emoji | 🎉🎉
    "}));
}

#[test]
fn code() {
    insta::assert_snapshot!(normalized(indoc! {"
        `a` ``b`c`` `` ` `` ` d ` ``` `` ```

        ~~~rust
        fn main() {}
        ~~~

        ````
        ```
        inner fence
        ```
        ````

            indented

            code

        ```
        ```

        ~~~ a`b
        info with backtick
        ~~~
    "}));
}

#[test]
fn escaping() {
    insta::assert_snapshot!(normalized(indoc! {r"
        \# heading \- list 1\. ordered: only escaped at the start of a line

        \# not a heading

        \- not a list

        \+ not a list

        1\. not ordered

        \> not a quote

        \--- and \=== and \___

        2 * 3 * 4, a*b, snake_case, _private, 5 < 6, AT&T, a | b, [x], x[0], f(a)[1]

        \*literal stars\* \_literal underscores\_ \~\~tildes\~\~ \`backtick\`

        \[text](url) \![alt](src) \<tag> \<!-- c --> \&amp; \\\* C:\dir

        &#35; &amp;amp; &lt;b&gt; &#42;x&#42; &#32;lead and trail&#32;

        line\
        \# after a hard break\
        \---
    "}));
}

#[test]
fn escaping_with_definitions() {
    insta::assert_snapshot!(normalized(indoc! {r"
        \[text] is escaped because [text] is defined.

        [text]: /url
    "}));
}

#[test]
fn soft_wrapped_paragraph() {
    insta::assert_snapshot!(normalized(indoc! {"
        A paragraph that was
        wrapped by hand
          with indentation
        and a hard break  
        at the end.
    "}));
}

#[test]
fn quotes() {
    insta::assert_snapshot!(normalized(indoc! {"
        > first paragraph
        continues lazily
        >
        > second paragraph
        > - item
        >   - nested
        >
        > > nested quote
        >
        >     code

        >

        > # heading first
        > text
    "}));
}

#[test]
fn headings_rules_images() {
    insta::assert_snapshot!(normalized(indoc! {"
        Setext one
        ===

        Setext
        two
        ---

        ### closed ###

        # trailing hash \\#

        #

        * * *
        ___

        ![alt text](img.png 'Title')

        ![photo_1|300](<my photos/photo 1.png>)
    "}));
}

#[test]
fn extension_syntax() {
    insta::assert_snapshot!(normalized_with(
        indoc! {r"
            [[Note]] [[Target|Alias]] ![[embed.png]] #tag #a/b ==highlight==

            \[[not a link]] \#nottag \==not highlight== a#b ##c

            | [[A\|B]] | #x |
            |---|---|
        "},
        extensions()
    ));
}

#[test]
fn extension_syntax_without_extensions() {
    insta::assert_snapshot!(normalized("[[Note]] #tag ==highlight==\n"));
}

#[test]
fn raw_is_kept() {
    insta::assert_snapshot!(normalized(indoc! {r"
        ---
        title: Front matter
        ---
        <div align=center>
          *not markdown*
        </div>

        > [!tip] Callout
        > stays

        Term
        : definition

        Text with <span class='x'>html</span>, a footnote[^1], $math_{1}$ and ![inline](img.png).

        [^1]: Footnote
            text

        - item

          [ref]: /in-item
    "}));
}

#[test]
fn single_edited_block_leaves_neighbours_untouched() {
    let source = indoc! {"
        Setext heading
        ==============
        * item *one*
        * item __two__
        * item three

        Paragraph with   odd    spacing
        and a soft break.



        | a|b |
        |-|-|
        |1|2|
    "};
    // The heading, the middle item and the paragraph.
    insta::assert_snapshot!(edited(source, &[0, 2, 4]));
}

#[test]
fn edited_item_keeps_the_marker_of_its_list() {
    let source = "+ one\n+ __two__\n+ three\n\n3) a\n4) __b__\n5) c\n";
    insta::assert_snapshot!(edited(source, &[1, 4]));
}

#[test]
fn edited_table() {
    let source = "before\n\n|a|b|\n|-|:-:|\n|1|2|\nafter\n";
    insta::assert_snapshot!(edited(source, &[1]));
}

#[test]
fn edited_block_next_to_tight_neighbours() {
    // A paragraph directly above a list and a heading directly above text.
    let source = "Shopping:\n- milk\n- eggs\n## Next\ntext\n> quote\n";
    insta::assert_snapshot!("tight_paragraph", edited(source, &[0]));
    insta::assert_snapshot!("tight_item", edited(source, &[1]));
    insta::assert_snapshot!("tight_heading", edited(source, &[3]));
    insta::assert_snapshot!("tight_text", edited(source, &[4]));
}

#[test]
fn edited_blocks_use_the_line_ending_of_the_file() {
    let source = "# Title\r\n\r\n* a\r\n* b\r\n  * c\r\n\r\nhard  \r\nbreak\r\n";
    insta::assert_snapshot!("crlf", edited(source, &[1, 3]));
    let source = "\u{feff}a\n\n__last__";
    insta::assert_snapshot!("bom_no_final_newline", edited(source, &[1]));
}
