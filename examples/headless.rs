//! The document model without a window: outline, open tasks and tags of a
//! note, a change, and the Markdown written back.
//!
//! Run with `cargo run --example headless`.

use gpui_markdown_editor::extensions::{Tag, WikiLink};
use gpui_markdown_editor::{Block, BlockKind, Document, Extensions, Mark};

const NOTE: &str = "\
# Weekend trip

Leaving *Friday* at 8. See [[Packing list]] and #travel.

## To do

- [x] Book the cabin
- [ ] Check the tires
  - [ ] Spare wheel too
- [ ] Buy groceries #shopping

## Notes

Nothing yet.
";

/// Calls `f` with every block, nested ones included, in document order.
fn walk<'a>(blocks: &'a [Block], f: &mut impl FnMut(&'a Block)) {
    for block in blocks {
        f(block);
        walk(block.children(), f);
    }
}

fn main() {
    let extensions = Extensions::new(vec![Box::new(WikiLink), Box::new(Tag)]);
    let doc = Document::parse_with(NOTE, extensions);

    println!("Outline:");
    walk(doc.blocks(), &mut |block| {
        if let BlockKind::Heading(level) = block.kind() {
            println!("{}{}", "  ".repeat(usize::from(*level) - 1), block.text());
        }
    });

    println!("\nOpen tasks:");
    walk(doc.blocks(), &mut |block| {
        if let BlockKind::TaskItem { checked: false } = block.kind() {
            println!("- {}", block.text());
        }
    });

    // Tags are marks of the `Tag` extension; the text includes the `#`.
    let mut tags = Vec::new();
    walk(doc.blocks(), &mut |block| {
        let content = block.content();
        for span in content.marks() {
            if span.mark
                == (Mark::Custom {
                    id: Tag::MARK,
                    attr: None,
                })
            {
                tags.push(content.text()[span.range.clone()].to_string());
            }
        }
    });
    println!("\nTags: {}", tags.join(", "));

    // Unchanged, the document writes back byte for byte.
    assert_eq!(doc.to_markdown(), NOTE);
    // `normalized` writes every block in the editor's fixed style.
    println!("\nNormalized:\n{}", doc.normalized());
}
