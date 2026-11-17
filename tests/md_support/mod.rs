//! Helpers shared by the Markdown tests.
#![allow(dead_code)]

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use gpui_markdown_editor::extensions::{Highlight, Tag, WikiLink};
use gpui_markdown_editor::{Block, BlockKind, Document, Extensions, Inline};

/// The extensions shipped with the crate.
pub fn extensions() -> Extensions {
    Extensions::new(vec![Box::new(WikiLink), Box::new(Tag), Box::new(Highlight)])
}

pub fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus")
}

/// One example of a specification.
pub struct Example {
    pub name: String,
    pub markdown: String,
}

/// Loads the examples of one file in `tests/corpus/spec`.
pub fn spec_examples(file: &str) -> Vec<Example> {
    let path = corpus_dir().join("spec").join(file);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let json: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
    let stem = file.trim_end_matches(".json");
    json.as_array()
        .expect("an array of examples")
        .iter()
        .map(|example| Example {
            name: format!("{stem}#{}", example["example"]),
            markdown: example["markdown"].as_str().expect("markdown").to_string(),
        })
        .collect()
}

/// Loads the notes in `tests/corpus/notes`, sorted by file name.
pub fn notes() -> Vec<Example> {
    let dir = corpus_dir().join("notes");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| Example {
            name: path.file_name().unwrap().to_string_lossy().into_owned(),
            markdown: std::fs::read_to_string(&path).expect("UTF-8 note"),
        })
        .collect()
}

fn dump_inline(inline: &Inline, out: &mut String) {
    write!(out, "{:?}", inline.text()).unwrap();
    for span in inline.marks() {
        write!(
            out,
            " [{}..{} {:?}]",
            span.range.start, span.range.end, span.mark
        )
        .unwrap();
    }
}

fn dump_block(block: &Block, depth: usize, out: &mut String) {
    out.push_str(&"  ".repeat(depth));
    match block.kind() {
        BlockKind::Table(table) => {
            writeln!(out, "Table {:?}", table.align).unwrap();
            for row in &table.rows {
                out.push_str(&"  ".repeat(depth + 1));
                out.push_str("Row");
                for cell in row {
                    out.push_str(" | ");
                    dump_inline(cell.content(), out);
                }
                out.push('\n');
            }
        }
        kind => {
            write!(out, "{kind:?} ").unwrap();
            dump_inline(block.content(), out);
            out.push('\n');
        }
    }
    for child in block.children() {
        dump_block(child, depth + 1, out);
    }
}

/// Describes the blocks of a document without ids, source ranges and dirty
/// flags, so that two documents can be compared by content.
pub fn dump(document: &Document) -> String {
    let mut out = String::new();
    for block in document.blocks() {
        dump_block(block, 0, &mut out);
    }
    out
}

/// Marks every block of the document as edited.
pub fn mark_all_dirty(document: &mut Document) {
    fn ids(blocks: &[Block], out: &mut Vec<gpui_markdown_editor::BlockId>) {
        for block in blocks {
            out.push(block.id());
            ids(block.children(), out);
        }
    }
    let mut all = Vec::new();
    ids(document.blocks(), &mut all);
    for id in all {
        assert!(document.mark_dirty(id));
    }
}
