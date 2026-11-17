//! Generated documents shared by the benchmarks.

#![allow(dead_code)]

/// A note-like document of at least `bytes` bytes: headings, paragraphs
/// with marks and links, lists, tasks, quotes, code blocks and tables.
pub fn document_of_size(bytes: usize) -> String {
    let mut md = String::from("# Benchmark notes\n\n");
    let mut section = 1;
    while md.len() < bytes {
        push_section(&mut md, section);
        section += 1;
    }
    md
}

/// A document of exactly `blocks` top-level blocks: sections of a heading,
/// paragraphs and lists, as the 10,000-line timing test uses.
pub fn document_of_blocks(blocks: usize) -> String {
    let mut md = String::new();
    for i in 0..blocks {
        match i % 4 {
            0 => md.push_str(&format!("## Section {i}\n\n")),
            1 | 2 => md.push_str(&format!(
                "Paragraph {i}: some **bold** words, a [link](https://example.com) and enough \
                 text to wrap onto a second line in the content column.\n\n"
            )),
            _ => md.push_str(&format!(
                "- Item of {i}\n- [ ] Task of {i}\n- [x] Done in {i}\n\n"
            )),
        }
    }
    md
}

fn push_section(md: &mut String, n: usize) {
    md.push_str(&format!("## Section {n}\n\n"));
    md.push_str(&format!(
        "Paragraph {n} with **bold**, *italic*, `code`, ~~struck~~ and ==highlighted== text, \
         a [link](https://example.com/{n}), a [[Wiki Link {n}]] and a #tag{n}. It goes on \
         long enough to wrap onto a second line in a normal content column.\n\n"
    ));
    md.push_str(&format!(
        "- First item {n}\n  - Nested item\n- Second item\n\n- [ ] Open task {n}\n- [x] Done task\n\n"
    ));
    md.push_str(&format!("> A quote in section {n}, with *emphasis*.\n\n"));
    md.push_str(&format!(
        "```rust\nfn section_{n}() -> usize {{\n    {n}\n}}\n```\n\n"
    ));
    md.push_str(&format!(
        "| Name | Count | Note |\n|---|--:|---|\n| a{n} | {n} | first |\n| b{n} | 42 | second |\n\n"
    ));
}

/// Rich-text HTML of at least `bytes` bytes, as a browser puts it on the
/// clipboard.
pub fn html_of_size(bytes: usize) -> String {
    let mut html = String::from("<html><body>");
    let mut n = 1;
    while html.len() < bytes {
        html.push_str(&format!(
            "<h2>Heading {n}</h2><p>Some <b>bold</b>, <i>italic</i> and <code>code</code> with a \
             <a href=\"https://example.com/{n}\">link</a>.</p><ul><li>One</li><li>Two \
             <strong>strong</strong></li></ul><blockquote><p>Quoted {n}</p></blockquote>\
             <table><tr><th>A</th><th>B</th></tr><tr><td>{n}</td><td>x</td></tr></table>\
             <pre><code>let x = {n};</code></pre>"
        ));
        n += 1;
    }
    html.push_str("</body></html>");
    html
}
