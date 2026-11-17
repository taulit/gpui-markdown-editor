//! The lossless round-trip: parsing a file and writing it back without an
//! edit yields the same bytes, for the CommonMark and GFM specification
//! examples and for a corpus of notes.
//!
//! The second half checks the normalized style that edited blocks are written
//! in: writing it is stable, and it does not change what the text means.

mod md_support;

use gpui_markdown_editor::{Document, Extensions};
use md_support::{Example, dump, extensions, mark_all_dirty, notes, spec_examples};

const SPEC_FILES: [&str; 3] = [
    "commonmark-0.31.2.json",
    "gfm-0.29.json",
    "gfm-extensions.json",
];

fn all_examples() -> Vec<Example> {
    let mut examples: Vec<Example> = SPEC_FILES
        .iter()
        .flat_map(|file| spec_examples(file))
        .collect();
    examples.extend(notes());
    examples
}

/// Checks that the source ranges of the top-level blocks cover the source in
/// order, with nothing but whitespace between them.
fn check_tiling(document: &Document) -> Result<(), String> {
    let source = document.source();
    let blank = |text: &str| text.bytes().all(|b| b.is_ascii_whitespace());
    let mut cursor = if source.starts_with('\u{feff}') { 3 } else { 0 };
    let mut rebuilt = source[..cursor].to_string();
    for block in document.blocks() {
        let origin = block.origin().ok_or("a parsed block without origin")?;
        if origin.start < cursor || origin.end > source.len() || origin.start > origin.end {
            return Err(format!(
                "origin {origin:?} is out of order (cursor {cursor})"
            ));
        }
        let gap = source
            .get(cursor..origin.start)
            .ok_or("gap not on a char boundary")?;
        if !blank(gap) {
            return Err(format!("text between blocks is not blank: {gap:?}"));
        }
        let text = source
            .get(origin.clone())
            .ok_or("origin not on a char boundary")?;
        if text.is_empty() || text.ends_with(|c: char| c.is_ascii_whitespace()) {
            return Err(format!(
                "origin {origin:?} is empty or ends with whitespace: {text:?}"
            ));
        }
        let line_start =
            origin.start == cursor && cursor <= 3 || source[..origin.start].ends_with(['\n', '\r']);
        if !line_start {
            return Err(format!(
                "origin {origin:?} does not start at the start of a line"
            ));
        }
        for child in block.children() {
            if child.origin().is_some() {
                return Err("a nested block has an origin".into());
            }
        }
        rebuilt.push_str(gap);
        rebuilt.push_str(text);
        cursor = origin.end;
    }
    let rest = &source[cursor..];
    if !blank(rest) {
        return Err(format!("text behind the last block is not blank: {rest:?}"));
    }
    rebuilt.push_str(rest);
    if rebuilt != source {
        return Err("blocks and gaps do not add up to the source".into());
    }
    Ok(())
}

fn check_ids(document: &Document) -> Result<(), String> {
    fn collect(
        blocks: &[gpui_markdown_editor::Block],
        ids: &mut Vec<gpui_markdown_editor::BlockId>,
    ) {
        for block in blocks {
            ids.push(block.id());
            if let gpui_markdown_editor::BlockKind::Table(table) = block.kind() {
                ids.extend(table.rows.iter().flatten().map(|cell| cell.id()));
            }
            collect(block.children(), ids);
        }
    }
    let mut ids = Vec::new();
    collect(document.blocks(), &mut ids);
    let count = ids.len();
    ids.sort();
    ids.dedup();
    if ids.len() == count {
        Ok(())
    } else {
        Err("ids are not unique".into())
    }
}

fn report(failures: Vec<String>, total: usize) {
    assert!(
        failures.is_empty(),
        "{} of {total} inputs failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

fn check_identity(extensions: impl Fn() -> Extensions) {
    let examples = all_examples();
    let mut failures = Vec::new();
    for example in &examples {
        let document = Document::parse_with(&example.markdown, extensions());
        if document.to_markdown() != example.markdown {
            failures.push(format!("{}: output differs from input", example.name));
        }
        if document.is_modified() {
            failures.push(format!(
                "{}: parsed document counts as modified",
                example.name
            ));
        }
        if let Err(error) = check_tiling(&document).and_then(|_| check_ids(&document)) {
            failures.push(format!("{}: {error}", example.name));
        }
    }
    report(failures, examples.len());
}

#[test]
fn unedited_documents_are_written_byte_identically() {
    check_identity(Extensions::default);
}

#[test]
fn unedited_documents_are_written_byte_identically_with_extensions() {
    check_identity(extensions);
}

#[test]
fn spec_files_are_complete() {
    assert_eq!(spec_examples("commonmark-0.31.2.json").len(), 652);
    assert_eq!(spec_examples("gfm-0.29.json").len(), 672);
    assert_eq!(spec_examples("gfm-extensions.json").len(), 30);
    assert!(notes().len() >= 20);
}

/// Spec examples whose meaning the normalized style cannot keep, with the
/// reason. Everything else must parse to the same model after normalizing.
const MODEL_CHANGES: &[(&str, &str)] = &[];

fn check_normalized(examples: &[Example], extensions: impl Fn() -> Extensions) {
    let mut failures = Vec::new();
    let mut unused: Vec<&str> = MODEL_CHANGES.iter().map(|(name, _)| *name).collect();
    for example in examples {
        let original = Document::parse_with(&example.markdown, extensions());
        let first = original.normalized();
        let reparsed = Document::parse_with(&first, extensions());
        let second = reparsed.normalized();
        if first != second {
            failures.push(format!(
                "{}: normalizing is not stable\n  input  {:?}\n  first  {first:?}\n  second {second:?}",
                example.name, example.markdown
            ));
            continue;
        }
        // Marking every block dirty must give the same text as `normalized`,
        // apart from what the source contributes around the blocks.
        let mut dirty = Document::parse_with(&example.markdown, extensions());
        mark_all_dirty(&mut dirty);
        let rewritten = Document::parse_with(&dirty.to_markdown(), extensions());
        if dump(&rewritten) != dump(&reparsed) {
            failures.push(format!(
                "{}: all blocks dirty differs from normalized",
                example.name
            ));
        }
        let allowed = MODEL_CHANGES.iter().any(|(name, _)| *name == example.name);
        unused.retain(|name| *name != example.name);
        let same = dump(&original) == dump(&reparsed);
        if same == allowed {
            let what = if allowed {
                "is allowlisted but keeps its model"
            } else {
                "changes the model"
            };
            failures.push(format!(
                "{}: normalizing {what}\n  input  {:?}\n  output {first:?}\n--- before\n{}--- after\n{}",
                example.name,
                example.markdown,
                dump(&original),
                dump(&reparsed)
            ));
        }
    }
    let _ = unused;
    report(failures, examples.len());
}

#[test]
fn normalized_spec_examples_are_stable_and_keep_their_model() {
    let examples: Vec<Example> = SPEC_FILES
        .iter()
        .flat_map(|file| spec_examples(file))
        .collect();
    check_normalized(&examples, Extensions::default);
}

#[test]
fn normalized_notes_are_stable_and_keep_their_model() {
    check_normalized(&notes(), Extensions::default);
    check_normalized(&notes(), extensions);
}

/// Nesting deeper than the parser follows is kept as raw source instead of
/// overflowing the stack.
#[test]
fn deep_nesting_is_kept_as_raw_source() {
    let quotes = format!("{}deep\n", "> ".repeat(3000));
    let lists: String = (0..300)
        .map(|depth| format!("{}- item\n", "  ".repeat(depth)))
        .collect();
    let mixed = format!("{}text\n", "> - ".repeat(500));
    for source in [quotes, lists, mixed] {
        let document = Document::parse(&source);
        assert_eq!(document.to_markdown(), source);
        check_tiling(&document).unwrap();
        let normalized = document.normalized();
        let reparsed = Document::parse(&normalized);
        assert_eq!(dump(&reparsed), dump(&document));
        assert_eq!(reparsed.normalized(), normalized);
    }
}
