//! HTML to Markdown, for pasting rich text (feature `paste-html`).
//!
//! Covers what rich text on the clipboard usually holds: paragraphs,
//! headings, lists (with task checkboxes), quotes, preformatted code,
//! tables, rules, links, images, bold, italic, underline, strikethrough and
//! inline code. Unknown elements contribute their text. Scripts, styles and
//! the document head are dropped.

use std::cell::RefCell;

use html5ever::tendril::StrTendril;

use super::write_inline::{line_start_escape, write_code, write_destination};
use html5ever::tokenizer::{
    BufferQueue, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
};

/// Converts an HTML fragment or document to Markdown in the editor's
/// normalized style. Returns an empty string if there is no content.
///
/// Hosts that read HTML from the clipboard themselves (GPUI does not offer
/// an HTML clipboard format) can insert the result with
/// [`Command::InsertMarkdown`](crate::Command::InsertMarkdown):
///
/// ```
/// use gpui_markdown_editor::markdown::html_to_markdown;
///
/// let md = html_to_markdown("<h2>Plan</h2><p>Buy <b>milk</b> and <a href=\"https://x.org\">more</a></p>");
/// assert_eq!(md, "## Plan\n\nBuy **milk** and [more](https://x.org)\n");
/// ```
///
/// Input beyond 8 MiB is ignored, and elements nested deeper than 64
/// levels are flattened into their ancestor, so hostile input cannot
/// exhaust memory or the stack.
pub fn html_to_markdown(html: &str) -> String {
    let mut end = html.len().min(MAX_INPUT);
    while !html.is_char_boundary(end) {
        end -= 1;
    }
    let root = parse(&html[..end]);
    let blocks = render_blocks(&root.children, &Ctx::default());
    let mut out = blocks.join("\n\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// A minimal DOM: elements with their attributes, and text.
#[derive(Debug, Default)]
struct Element {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<Node>,
}

#[derive(Debug)]
enum Node {
    Element(Element),
    Text(String),
}

impl Element {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

const VOID: &[&str] = &[
    "br", "img", "hr", "input", "meta", "link", "col", "area", "base", "wbr", "source",
];
const SKIP: &[&str] = &["script", "style", "head", "title", "template", "noscript"];
const BLOCK: &[&str] = &[
    "p",
    "div",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "ul",
    "ol",
    "li",
    "blockquote",
    "pre",
    "table",
    "hr",
    "section",
    "article",
    "header",
    "footer",
    "main",
    "figure",
    "body",
    "html",
    "nav",
    "aside",
    "dl",
    "dd",
    "dt",
    "figcaption",
    "address",
    "details",
    "summary",
    "option",
    "select",
];

/// The longest input converted, in bytes.
const MAX_INPUT: usize = 8 << 20;
/// The deepest element nesting kept; deeper elements are flattened into
/// their ancestor (their content stays).
const MAX_DEPTH: usize = 64;

/// HTML's implied end tags: a start tag `name` closes the innermost open
/// element named in the first list, unless one of the second list (a
/// scope boundary) is open inside it.
fn implied_end(name: &str) -> Option<(&'static [&'static str], &'static [&'static str])> {
    const SCOPE: &[&str] = &["table", "td", "th", "caption", "html", "template", "button"];
    Some(match name {
        "li" => (
            &["li"],
            &[
                "ul", "ol", "table", "td", "th", "caption", "html", "template",
            ],
        ),
        "dt" | "dd" => (
            &["dt", "dd"],
            &["dl", "table", "td", "th", "html", "template"],
        ),
        "tr" => (&["tr"], &["table", "html", "template"]),
        "td" | "th" => (&["td", "th"], &["tr", "table", "html", "template"]),
        "thead" | "tbody" | "tfoot" => {
            (&["thead", "tbody", "tfoot"], &["table", "html", "template"])
        }
        "option" => (&["option"], &["select", "datalist", "html", "template"]),
        "optgroup" => (&["option", "optgroup"], &["select", "html", "template"]),
        _ if BLOCK.contains(&name) => (&["p"], SCOPE),
        _ => return None,
    })
}

/// Collects tokens into a tree. Unmatched end tags are ignored, unclosed
/// elements end with their parent, and start tags close the elements HTML
/// implicitly ends (`<li>a<li>b`, `<td>1<td>2`, `<p>a<p>b`).
#[derive(Default)]
struct TreeSink {
    stack: RefCell<Vec<Element>>,
    /// Start tags dropped beyond [`MAX_DEPTH`], whose end tags are ignored.
    dropped: RefCell<Vec<String>>,
}

impl TreeSink {
    /// Closes the elements implicitly ended by a start tag `name`.
    fn close_implied(stack: &mut Vec<Element>, name: &str) {
        let Some((closes, boundary)) = implied_end(name) else {
            return;
        };
        for depth in (1..stack.len()).rev() {
            let open = stack[depth].name.as_str();
            if closes.contains(&open) {
                while stack.len() > depth {
                    Self::close(stack);
                }
                return;
            }
            if boundary.contains(&open) {
                return;
            }
        }
    }

    fn close(stack: &mut Vec<Element>) {
        if stack.len() > 1
            && let Some(done) = stack.pop()
            && let Some(parent) = stack.last_mut()
        {
            parent.children.push(Node::Element(done));
        }
    }
}

impl TokenSink for TreeSink {
    type Handle = ();

    fn process_token(&self, token: Token, _line: u64) -> TokenSinkResult<()> {
        let mut stack = self.stack.borrow_mut();
        match token {
            Token::TagToken(tag) => {
                let name = tag.name.to_string();
                match tag.kind {
                    TagKind::StartTag => {
                        let element = Element {
                            attrs: tag
                                .attrs
                                .iter()
                                .map(|a| (a.name.local.to_string(), a.value.to_string()))
                                .collect(),
                            name: name.clone(),
                            children: Vec::new(),
                        };
                        Self::close_implied(&mut stack, &name);
                        let raw =
                            matches!(name.as_str(), "script" | "style" | "textarea" | "title");
                        if VOID.contains(&name.as_str()) || tag.self_closing {
                            if let Some(parent) = stack.last_mut() {
                                parent.children.push(Node::Element(element));
                            }
                        } else if stack.len() > MAX_DEPTH && !raw {
                            self.dropped.borrow_mut().push(name);
                        } else {
                            stack.push(element);
                            // The tokenizer must not parse markup inside these.
                            if raw {
                                return TokenSinkResult::RawData(
                                    html5ever::tokenizer::states::RawKind::Rcdata,
                                );
                            }
                        }
                    }
                    TagKind::EndTag => {
                        let mut dropped = self.dropped.borrow_mut();
                        if let Some(i) = dropped.iter().rposition(|d| *d == name) {
                            dropped.remove(i);
                        } else if let Some(depth) = stack.iter().rposition(|e| e.name == name)
                            && depth > 0
                        {
                            while stack.len() > depth {
                                Self::close(&mut stack);
                            }
                        }
                    }
                }
            }
            Token::CharacterTokens(text) => {
                if let Some(parent) = stack.last_mut() {
                    match parent.children.last_mut() {
                        Some(Node::Text(t)) => t.push_str(&text),
                        _ => parent.children.push(Node::Text(text.to_string())),
                    }
                }
            }
            _ => {}
        }
        TokenSinkResult::Continue
    }
}

fn parse(html: &str) -> Element {
    let sink = TreeSink::default();
    sink.stack.borrow_mut().push(Element::default());
    let tokenizer = Tokenizer::new(sink, TokenizerOpts::default());
    let queue = BufferQueue::default();
    queue.push_back(StrTendril::from(html));
    let _ = tokenizer.feed(&queue);
    tokenizer.end();
    let sink = tokenizer.sink;
    let mut stack = sink.stack.into_inner();
    while stack.len() > 1 {
        TreeSink::close(&mut stack);
    }
    stack.pop().unwrap_or_default()
}

/// Where inline content is being written.
#[derive(Clone, Default)]
struct Ctx {
    /// Inside a link: nested links are written as text.
    in_link: bool,
}

fn is_block(node: &Node) -> bool {
    matches!(node, Node::Element(e) if BLOCK.contains(&e.name.as_str()))
}

/// The blocks of a container: runs of inline content become paragraphs.
fn render_blocks(children: &[Node], ctx: &Ctx) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut inline = String::new();
    let flush = |inline: &mut String, blocks: &mut Vec<String>| {
        let text = finish_inline(inline);
        if !text.is_empty() {
            blocks.push(text);
        }
        inline.clear();
    };
    for child in children {
        match child {
            Node::Element(e) if SKIP.contains(&e.name.as_str()) => {}
            Node::Element(e) if is_block(child) => {
                flush(&mut inline, &mut blocks);
                blocks.extend(render_block(e, ctx));
            }
            _ => render_inline(child, ctx, &mut inline),
        }
    }
    flush(&mut inline, &mut blocks);
    blocks
}

fn render_block(e: &Element, ctx: &Ctx) -> Vec<String> {
    match e.name.as_str() {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            let level = usize::from(e.name.as_bytes()[1] - b'0');
            let text = inline_of(&e.children, ctx).replace('\n', " ");
            if text.is_empty() {
                Vec::new()
            } else {
                vec![format!("{} {text}", "#".repeat(level))]
            }
        }
        "hr" => vec!["---".to_string()],
        "pre" => vec![code_block(e)],
        "ul" | "ol" => {
            let list = render_list(e, ctx);
            if list.is_empty() {
                Vec::new()
            } else {
                vec![list]
            }
        }
        "blockquote" => {
            let inner = render_blocks(&e.children, ctx).join("\n\n");
            if inner.is_empty() {
                return Vec::new();
            }
            vec![
                inner
                    .lines()
                    .map(|l| {
                        if l.is_empty() {
                            ">".to_string()
                        } else {
                            format!("> {l}")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            ]
        }
        "table" => render_table(e, ctx).into_iter().collect(),
        // A stray item outside a list.
        "li" => {
            let text = render_blocks(&e.children, ctx).join("\n\n");
            if text.is_empty() {
                Vec::new()
            } else {
                vec![format!("- {text}")]
            }
        }
        _ => render_blocks(&e.children, ctx),
    }
}

fn render_list(e: &Element, ctx: &Ctx) -> String {
    let ordered = e.name == "ol";
    let start: usize = e.attr("start").and_then(|s| s.parse().ok()).unwrap_or(1);
    let mut items = Vec::new();
    let mut number = start;
    for child in &e.children {
        let Node::Element(li) = child else {
            continue;
        };
        if li.name != "li" {
            if li.name == "ul" || li.name == "ol" {
                // A list directly in a list belongs to the item before.
                let nested = render_list(li, ctx);
                if let Some(last) = items.last_mut() {
                    let last: &mut String = last;
                    for line in nested.lines() {
                        last.push('\n');
                        last.push_str("  ");
                        last.push_str(line);
                    }
                }
            }
            continue;
        }
        let checkbox = find_checkbox(li);
        let mut marker = if ordered {
            format!("{number}. ")
        } else {
            "- ".to_string()
        };
        number += 1;
        if let Some(checked) = checkbox {
            marker.push_str(if checked { "[x] " } else { "[ ] " });
        }
        let blocks = render_blocks(&li.children, ctx);
        let indent = " ".repeat(if ordered { marker.len().min(4) } else { 2 });
        let mut item = String::new();
        for (i, block) in blocks.iter().enumerate() {
            let nested_list =
                block.starts_with("- ") || block.starts_with(|c: char| c.is_ascii_digit());
            if i > 0 {
                // Nested lists stay tight, other blocks get a blank line.
                item.push_str(if nested_list { "\n" } else { "\n\n" });
            }
            for (j, line) in block.lines().enumerate() {
                if j > 0 {
                    item.push('\n');
                }
                if i == 0 && j == 0 {
                    item.push_str(&marker);
                } else if !line.is_empty() {
                    item.push_str(&indent);
                }
                item.push_str(line);
            }
        }
        if blocks.is_empty() {
            item.push_str(marker.trim_end());
        }
        items.push(item);
    }
    items.join("\n")
}

/// `Some(checked)` if the item starts with a checkbox input.
fn find_checkbox(li: &Element) -> Option<bool> {
    fn walk(nodes: &[Node]) -> Option<bool> {
        for node in nodes {
            match node {
                Node::Element(e) if e.name == "input" => {
                    return (e.attr("type") == Some("checkbox"))
                        .then(|| e.attr("checked").is_some());
                }
                Node::Element(e) if !BLOCK.contains(&e.name.as_str()) || e.name == "p" => {
                    if let Some(found) = walk(&e.children) {
                        return Some(found);
                    }
                }
                Node::Text(t) if !t.trim().is_empty() => return None,
                _ => {}
            }
        }
        None
    }
    walk(&li.children)
}

fn code_block(pre: &Element) -> String {
    let mut text = String::new();
    text_content(&pre.children, &mut text);
    let lang = pre
        .children
        .iter()
        .find_map(|n| match n {
            Node::Element(e) if e.name == "code" => e.attr("class"),
            _ => None,
        })
        .or_else(|| pre.attr("class"))
        .and_then(|class| {
            class.split_whitespace().find_map(|c| {
                c.strip_prefix("language-")
                    .or_else(|| c.strip_prefix("lang-"))
            })
        })
        .unwrap_or("");
    let text = text.strip_suffix('\n').unwrap_or(&text);
    let longest = longest_run(text, '`');
    let fence = "`".repeat((longest + 1).max(3));
    format!("{fence}{lang}\n{text}\n{fence}")
}

fn render_table(table: &Element, ctx: &Ctx) -> Option<String> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    fn collect<'a>(nodes: &'a [Node], out: &mut Vec<&'a Element>) {
        for node in nodes {
            if let Node::Element(e) = node {
                match e.name.as_str() {
                    "tr" => out.push(e),
                    "table" => {}
                    _ => collect(&e.children, out),
                }
            }
        }
    }
    let mut trs = Vec::new();
    collect(&table.children, &mut trs);
    for tr in trs {
        let cells: Vec<String> = tr
            .children
            .iter()
            .filter_map(|n| match n {
                Node::Element(c) if c.name == "td" || c.name == "th" => Some(
                    inline_of(&c.children, ctx)
                        .replace('\n', " ")
                        .replace('|', "\\|"),
                ),
                _ => None,
            })
            .collect();
        if !cells.is_empty() {
            rows.push(cells);
        }
    }
    let cols = rows.iter().map(Vec::len).max()?;
    let line = |cells: &[String]| {
        let mut s = String::from("|");
        for i in 0..cols {
            s.push(' ');
            s.push_str(cells.get(i).map_or("", String::as_str));
            s.push_str(" |");
        }
        s
    };
    let mut out = vec![line(&rows[0]), format!("|{}", " --- |".repeat(cols))];
    out.extend(rows[1..].iter().map(|r| line(r)));
    Some(out.join("\n"))
}

/// The Markdown of inline content.
fn inline_of(nodes: &[Node], ctx: &Ctx) -> String {
    let mut out = String::new();
    for node in nodes {
        render_inline(node, ctx, &mut out);
    }
    finish_inline(&out)
}

/// Trims a paragraph and drops whitespace around hard breaks.
fn finish_inline(text: &str) -> String {
    let text = text.trim_matches(|c: char| c == ' ' || c == '\n');
    text.split("\\\n")
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\\\n")
        .trim_end_matches("\\\n")
        .to_string()
}

fn render_inline(node: &Node, ctx: &Ctx, out: &mut String) {
    match node {
        Node::Text(text) => {
            // Whitespace collapses as in HTML.
            let mut last_space = out.ends_with(' ') || out.is_empty() || out.ends_with('\n');
            // Text at the start of a line must not read as block syntax
            // (`- x`, `1. x`, `: x`, ...).
            let line_start = out.rsplit('\n').next().is_some_and(|l| l.trim().is_empty());
            let lead = text.len() - text.trim_start().len();
            let escape_at = if line_start {
                line_start_escape(text.trim_start(), true, false).map(|at| lead + at)
            } else {
                None
            };
            let mut chars = text.char_indices().peekable();
            while let Some((i, ch)) = chars.next() {
                if ch.is_whitespace() {
                    if !last_space {
                        out.push(' ');
                        last_space = true;
                    }
                } else {
                    let next = chars.peek().map(|(_, c)| *c);
                    // `==` would open a highlight.
                    if (escape_at == Some(i) || (ch == '=' && next == Some('=')))
                        && !needs_escape(ch)
                    {
                        out.push('\\');
                    }
                    escape_char(ch, out);
                    last_space = false;
                }
            }
        }
        Node::Element(e) => {
            let name = e.name.as_str();
            if SKIP.contains(&name) {
                return;
            }
            match name {
                "br" => out.push_str("\\\n"),
                "img" => {
                    let src = e.attr("src").unwrap_or("");
                    if !src.is_empty() {
                        let alt = escape(e.attr("alt").unwrap_or(""));
                        out.push_str(&format!("![{alt}]({})", write_destination(src)));
                    }
                }
                "input" => {}
                "a" if !ctx.in_link => {
                    let inner_ctx = Ctx { in_link: true };
                    let text = inline_of(&e.children, &inner_ctx);
                    match e
                        .attr("href")
                        .filter(|h| !h.is_empty() && !h.starts_with("javascript:"))
                    {
                        Some(href) if !text.is_empty() => {
                            out.push_str(&format!("[{text}]({})", write_destination(href)));
                        }
                        _ => out.push_str(&text),
                    }
                }
                "code" | "kbd" | "samp" | "tt" => {
                    let mut text = String::new();
                    text_content(&e.children, &mut text);
                    if !text.trim().is_empty() {
                        out.push_str(&write_code(&text, false));
                    }
                }
                _ => {
                    let delimiter = match name {
                        "b" | "strong" => Some(("**", "**")),
                        "i" | "em" | "cite" | "dfn" => Some(("*", "*")),
                        "s" | "del" | "strike" => Some(("~~", "~~")),
                        "u" | "ins" => Some(("<u>", "</u>")),
                        "mark" => Some(("==", "==")),
                        _ => None,
                    };
                    let bold_by_style = name == "span"
                        && e.attr("style").is_some_and(|s| {
                            s.contains("font-weight:700")
                                || s.contains("font-weight: 700")
                                || s.contains("font-weight:bold")
                                || s.contains("font-weight: bold")
                        });
                    let delimiter = delimiter.or(bold_by_style.then_some(("**", "**")));
                    match delimiter {
                        Some((open, close)) => {
                            let inner = inline_of(&e.children, ctx);
                            if inner.is_empty() {
                                return;
                            }
                            // Delimiters must touch the text: move the
                            // surrounding spaces outside.
                            let lead = inner.len() - inner.trim_start().len();
                            if lead > 0 && !out.ends_with(' ') {
                                out.push(' ');
                            }
                            out.push_str(open);
                            out.push_str(inner.trim());
                            out.push_str(close);
                        }
                        None => {
                            for child in &e.children {
                                if is_block(child) {
                                    // Block content inside inline content:
                                    // keep it as text on its own line.
                                    let blocks = match child {
                                        Node::Element(b) => render_block(b, ctx),
                                        Node::Text(_) => Vec::new(),
                                    };
                                    for b in blocks {
                                        out.push_str("\\\n");
                                        out.push_str(&b);
                                    }
                                } else {
                                    render_inline(child, ctx, out);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn text_content(nodes: &[Node], out: &mut String) {
    for node in nodes {
        match node {
            Node::Text(t) => out.push_str(t),
            Node::Element(e) if e.name == "br" => out.push('\n'),
            Node::Element(e) => text_content(&e.children, out),
        }
    }
}

fn longest_run(text: &str, ch: char) -> usize {
    let mut longest = 0;
    let mut run = 0;
    for c in text.chars() {
        if c == ch {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    longest
}

/// Characters that are escaped wherever they appear in text.
fn needs_escape(ch: char) -> bool {
    matches!(
        ch,
        '\\' | '*' | '_' | '`' | '[' | ']' | '<' | '>' | '~' | '#'
    )
}

fn escape_char(ch: char, out: &mut String) {
    if needs_escape(ch) {
        out.push('\\');
    }
    out.push(ch);
}

fn escape(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        escape_char(ch, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::html_to_markdown;

    #[test]
    fn converts_the_common_subset() {
        let cases: &[(&str, &str)] = &[
            ("", ""),
            (
                "<p>Hello <b>bold</b> and <em>it</em></p>",
                "Hello **bold** and *it*\n",
            ),
            ("<p>a</p><p>b</p>", "a\n\nb\n"),
            ("<h1>T</h1><h3>S</h3>", "# T\n\n### S\n"),
            (
                "<p><s>x</s> <del>y</del> <u>z</u> <code>c</code></p>",
                "~~x~~ ~~y~~ <u>z</u> `c`\n",
            ),
            (
                "<p><a href=\"https://a.b/c\">link</a></p>",
                "[link](https://a.b/c)\n",
            ),
            (
                "<p><a href=\"https://a.b/x y\">sp</a></p>",
                "[sp](<https://a.b/x y>)\n",
            ),
            ("<img src=\"p.png\" alt=\"Pic\">", "![Pic](p.png)\n"),
            (
                "<ul><li>one</li><li>two<ul><li>deep</li></ul></li></ul>",
                "- one\n- two\n  - deep\n",
            ),
            ("<ol start=\"3\"><li>c</li><li>d</li></ol>", "3. c\n4. d\n"),
            (
                "<ul><li><input type=\"checkbox\" checked> done</li><li><input type=\"checkbox\"> open</li></ul>",
                "- [x] done\n- [ ] open\n",
            ),
            (
                "<blockquote><p>q1</p><p>q2</p></blockquote>",
                "> q1\n>\n> q2\n",
            ),
            (
                "<pre><code class=\"language-rust\">fn x() {}\n</code></pre>",
                "```rust\nfn x() {}\n```\n",
            ),
            (
                "<table><tr><th>A</th><th>B</th></tr><tr><td>1</td><td>2|3</td></tr></table>",
                "| A | B |\n| --- | --- |\n| 1 | 2\\|3 |\n",
            ),
            ("<p>a<br>b</p>", "a\\\nb\n"),
            ("<hr>", "---\n"),
            ("<p>1 * 2 _x_ [y]</p>", "1 \\* 2 \\_x\\_ \\[y\\]\n"),
            (
                "<script>alert(1)</script><style>p{}</style><p>ok</p>",
                "ok\n",
            ),
            ("<div>  spaced \n  out  </div>", "spaced out\n"),
            ("<b>unclosed <i>tags", "**unclosed *tags***\n"),
            ("plain text", "plain text\n"),
            ("<p>a</b>b</p>", "ab\n"),
            (
                "<span style=\"font-weight:700\">gdocs</span>",
                "**gdocs**\n",
            ),
            ("<p>x <b> y </b>z</p>", "x **y**z\n"),
        ];
        for (html, md) in cases {
            assert_eq!(html_to_markdown(html), *md, "for {html:?}");
        }
    }

    #[test]
    fn output_parses_into_the_expected_blocks() {
        use crate::model::{BlockKind, Document};
        let md = html_to_markdown(
            "<h2>Plan</h2><ul><li>a</li></ul><blockquote>q</blockquote><table><tr><td>x</td></tr></table>",
        );
        let doc = Document::parse(&md);
        let kinds: Vec<_> = doc
            .blocks()
            .iter()
            .map(|b| std::mem::discriminant(b.kind()))
            .collect();
        assert_eq!(kinds.len(), 4);
        assert_eq!(*doc.blocks()[0].kind(), BlockKind::Heading(2));
        assert_eq!(*doc.blocks()[1].kind(), BlockKind::BulletItem);
        assert_eq!(*doc.blocks()[2].kind(), BlockKind::Quote);
        assert!(matches!(doc.blocks()[3].kind(), BlockKind::Table(_)));
    }

    #[test]
    fn odd_input_does_not_panic() {
        for html in [
            "<",
            "</",
            "<<>>",
            "<a href>",
            "<table><td>",
            "<ol start=x><li>",
            "&amp;&#0;&#xFFFFFF;",
            "<pre>```</pre>",
        ] {
            let _ = html_to_markdown(html);
        }
        assert_eq!(
            html_to_markdown("<pre>a ``` b</pre>"),
            "````\na ``` b\n````\n"
        );
    }

    #[test]
    fn deep_nesting_is_flattened() {
        let html = "<div>".repeat(5000) + "deep" + &"</div>".repeat(5000);
        assert_eq!(html_to_markdown(&html), "deep\n");
        let html = "<blockquote>".repeat(5000) + "q";
        let md = html_to_markdown(&html);
        assert!(md.ends_with("q\n"));
        let html = "<ul><li>".repeat(3000) + "x";
        assert!(html_to_markdown(&html).trim_end().ends_with("- x"));
        // Unclosed inline elements nest as deep.
        let html = "<b><i>".repeat(5000) + "t";
        assert!(html_to_markdown(&html).contains('t'));
        // Content after a dropped element continues in its ancestor.
        let html = "<div>".repeat(100) + "a" + &"</div>".repeat(100) + "<p>b</p>";
        assert_eq!(html_to_markdown(&html), "a\n\nb\n");
    }

    #[test]
    fn huge_input_is_cut() {
        let html = "<p>x</p>".repeat((super::MAX_INPUT / 8) + 10);
        let md = html_to_markdown(&html);
        assert_eq!(md.matches('x').count(), super::MAX_INPUT / 8);
    }

    #[test]
    fn implied_end_tags_make_siblings() {
        let cases: &[(&str, &str)] = &[
            ("<ul><li>a<li>b</ul>", "- a\n- b\n"),
            (
                "<ol><li>a<li>b<ul><li>c<li>d</ul><li>e</ol>",
                "1. a\n2. b\n   - c\n   - d\n3. e\n",
            ),
            ("<p>a<p>b", "a\n\nb\n"),
            ("<p>a<div>b</div>", "a\n\nb\n"),
            (
                "<table><tr><td>1<td>2<tr><td>3<td>4</table>",
                "| 1 | 2 |\n| --- | --- |\n| 3 | 4 |\n",
            ),
            (
                "<table><thead><tr><th>A<th>B<tbody><tr><td>1<td>2</table>",
                "| A | B |\n| --- | --- |\n| 1 | 2 |\n",
            ),
            ("<dl><dt>t<dd>d<dt>u</dl>", "t\n\nd\n\nu\n"),
            ("<select><option>a<option>b</select>", "a\n\nb\n"),
        ];
        for (html, md) in cases {
            assert_eq!(html_to_markdown(html), *md, "for {html:?}");
        }
    }

    #[test]
    fn text_stays_literal() {
        use crate::extension::Extensions;
        use crate::extensions::{Highlight, Tag, WikiLink};
        use crate::model::{BlockKind, Document};
        for text in [
            "- x",
            "+ x",
            "* x",
            "1. x",
            "2) x",
            "# x",
            "> x",
            "==x==",
            "a ==b== c",
            "[[x]]",
            "#tag",
            ": x",
            "---",
            "-",
            "~~~",
            "[ ] x",
            "a\\b",
        ] {
            let md = html_to_markdown(&format!("<p>{text}</p>"));
            let doc = Document::parse_with(
                &md,
                Extensions::new(vec![Box::new(WikiLink), Box::new(Tag), Box::new(Highlight)]),
            );
            assert_eq!(doc.blocks().len(), 1, "{text:?} gave {md:?}");
            let block = &doc.blocks()[0];
            assert_eq!(*block.kind(), BlockKind::Paragraph, "{text:?} gave {md:?}");
            assert_eq!(block.text(), text, "{text:?} gave {md:?}");
            assert!(block.content().marks.is_empty(), "{text:?} gave {md:?}");
        }
        // After a line break as well.
        assert_eq!(html_to_markdown("<p>a<br>- b</p>"), "a\\\n\\- b\n");
    }
}
