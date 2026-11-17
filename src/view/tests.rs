//! Layout, geometry and cache tests with the real platform text system.

use std::sync::{Arc, OnceLock};
use std::time::Instant;

use gpui::{
    Bounds, HeadlessAppContext, Pixels, PlatformTextSystem, WindowTextSystem, point, px, size,
};

use super::{DocumentView, LayoutInput, ViewInput};
use crate::edit::Selection;
use crate::extensions::{Highlight, Tag, WikiLink};
use crate::model::{BlockId, Document, Inline, TextPos, TextRange};
use crate::style::EditorStyle;
use crate::{BlockKind, Extensions};

struct Env {
    ts: WindowTextSystem,
    style: EditorStyle,
}

fn env() -> Env {
    static TEXT_SYSTEM: OnceLock<Arc<dyn PlatformTextSystem>> = OnceLock::new();
    let platform = TEXT_SYSTEM
        .get_or_init(|| gpui_platform::current_platform(true).text_system())
        .clone();
    let mut cx = HeadlessAppContext::with_platform(
        platform,
        Arc::new(()),
        gpui_platform::current_headless_renderer,
    );
    cx.update(|cx| {
        gpui_component::init(cx);
        let mut style = EditorStyle::from_theme(cx);
        style.font_family = "Noto Sans".into();
        style.code_font_family = "Noto Sans Mono".into();
        Env {
            ts: WindowTextSystem::new(cx.text_system().clone()),
            style,
        }
    })
}

fn bounds() -> Bounds<Pixels> {
    Bounds::new(point(px(0.), px(0.)), size(px(900.), px(700.)))
}

fn doc(md: &str) -> Document {
    Document::parse_with(
        md,
        Extensions::new(vec![Box::new(WikiLink), Box::new(Tag), Box::new(Highlight)]),
    )
}

fn layout_input<'a>(doc: &'a Document, style: &'a EditorStyle) -> LayoutInput<'a> {
    LayoutInput {
        document: doc,
        style,
        first_line_is_title: true,
        max_width: Some(px(700.)),
        column_widths: &NO_WIDTHS,
        image_preview: None,
    }
}

static NO_WIDTHS: std::sync::LazyLock<super::ColumnWidths> =
    std::sync::LazyLock::new(Default::default);

fn input<'a>(
    doc: &'a Document,
    style: &'a EditorStyle,
    selection: Option<&'a Selection>,
) -> ViewInput<'a> {
    ViewInput {
        layout: layout_input(doc, style),
        selection,
        highlights: &[],
        caret_visible: true,
        marked: None,
        hover: None,
        open_handle: None,
        resolve_src: None,
    }
}

/// Lays out and builds a frame like the element does.
fn frame(view: &DocumentView, doc: &Document, env: &Env) {
    let _ = view.prepare_frame(&input(doc, &env.style, None), bounds(), &env.ts);
}

fn pos(block: BlockId, offset: usize) -> TextPos {
    TextPos { block, offset }
}

const LONG: &str = "The quick brown fox jumps over the lazy dog and keeps running \
through the forest until the evening light fades behind the hills, where it \
finally rests next to a quiet river that reflects the first stars of the night.";

#[test]
fn caret_bounds_and_position_at_round_trip() {
    let env = env();
    let doc = doc(&format!(
        "Title\n\n{LONG}\n\n- item **bold** and *italic*\n"
    ));
    let view = DocumentView::new();
    frame(&view, &doc, &env);
    let mut q = view.query(layout_input(&doc, &env.style), &env.ts);
    for block in &doc.blocks()[1..] {
        let text = block.text();
        for (offset, _) in text.char_indices().chain([(text.len(), ' ')]) {
            let p = pos(block.id(), offset);
            let caret = q.caret_bounds(p).unwrap();
            let hit = q
                .position_at(point(caret.origin.x, caret.center().y))
                .unwrap();
            // Inside a ligature ("fi") two offsets share one place.
            if hit != p {
                assert_eq!(
                    q.caret_bounds(hit).unwrap(),
                    caret,
                    "offset {offset} in {text:?}"
                );
                continue;
            }
        }
    }
}

#[test]
fn position_at_margins_and_below_the_end() {
    let env = env();
    let doc = doc("Title\n\nFirst paragraph\n\nLast paragraph\n");
    let view = DocumentView::new();
    frame(&view, &doc, &env);
    let mut q = view.query(layout_input(&doc, &env.style), &env.ts);
    let last = doc.blocks()[2].id();
    let line = q.caret_bounds(pos(last, 0)).unwrap();
    // Left margin: start of the line. Right margin: end of the line.
    assert_eq!(
        q.position_at(point(px(5.), line.center().y)),
        Some(pos(last, 0))
    );
    assert_eq!(
        q.position_at(point(px(895.), line.center().y)),
        Some(pos(last, "Last paragraph".len()))
    );
    // Below the last block: on the last line.
    assert_eq!(
        q.position_at(point(line.origin.x, px(690.))),
        Some(pos(last, 0))
    );
}

#[test]
fn vertical_movement_across_wrapped_lines_and_blocks() {
    let env = env();
    let doc = doc(&format!("Title\n\n{LONG}\n\nShort one\n"));
    let view = DocumentView::new();
    frame(&view, &doc, &env);
    let mut q = view.query(layout_input(&doc, &env.style), &env.ts);
    let long = doc.blocks()[1].id();
    let short = doc.blocks()[2].id();

    let start = pos(long, 4);
    let goal = q.caret_bounds(start).unwrap().origin.x;
    let mut lines = vec![start];
    let mut cur = start;
    while let Some(next) = q.position_below(cur, goal) {
        lines.push(next);
        cur = next;
    }
    // The long paragraph wraps to at least two lines, then the short block.
    assert!(lines.len() >= 3, "{lines:?}");
    assert_eq!(lines.last().unwrap().block, short);
    let wrapped = lines.iter().filter(|p| p.block == long).count();
    assert!(wrapped >= 2);
    // The goal column is kept on the wrapped lines.
    for p in &lines[..wrapped] {
        let x = q.caret_bounds(*p).unwrap().origin.x;
        assert!((x - goal).abs() < px(12.), "{p:?} at {x:?}, goal {goal:?}");
    }
    // And back up again, to the start, then into the title.
    let mut cur = *lines.last().unwrap();
    for expected in lines.iter().rev().skip(1) {
        cur = q.position_above(cur, goal).unwrap();
        assert_eq!(cur.block, expected.block);
    }
    let title = q.position_above(cur, goal).unwrap();
    assert_eq!(title.block, doc.blocks()[0].id());
    assert_eq!(q.position_above(title, goal), None);
    let end = pos(short, 3);
    assert_eq!(q.position_below(end, goal), None);
}

#[test]
fn line_start_and_end_on_wrapped_lines() {
    let env = env();
    let doc = doc(&format!("Title\n\n{LONG}\n"));
    let view = DocumentView::new();
    frame(&view, &doc, &env);
    let mut q = view.query(layout_input(&doc, &env.style), &env.ts);
    let long = doc.blocks()[1].id();
    let first_end = q.line_end(pos(long, 0)).unwrap();
    assert!(first_end.offset > 0 && first_end.offset < LONG.len());
    // The line ends before the space it wraps at.
    assert_eq!(&LONG[first_end.offset..first_end.offset + 1], " ");
    let second = pos(long, first_end.offset + 1);
    assert_eq!(q.line_start(second).unwrap(), second);
    assert_eq!(q.line_start(pos(long, 3)).unwrap(), pos(long, 0));
    let last_end = q.line_end(pos(long, LONG.len() - 2)).unwrap();
    assert_eq!(last_end.offset, LONG.len());
    // Both ends of the first line are on the same visual line.
    let a = q.caret_bounds(pos(long, 0)).unwrap();
    let b = q.caret_bounds(first_end).unwrap();
    assert_eq!(a.origin.y, b.origin.y);
}

#[test]
fn word_ranges() {
    let env = env();
    let doc = doc("hello world, foo-bar  baz\n");
    let view = DocumentView::new();
    let mut q = view.query(layout_input(&doc, &env.style), &env.ts);
    let id = doc.blocks()[0].id();
    let word = |q: &mut super::ViewQuery<'_>, offset| {
        let r = q.word_range(pos(id, offset)).unwrap();
        r.start.offset..r.end.offset
    };
    assert_eq!(word(&mut q, 2), 0..5);
    assert_eq!(word(&mut q, 5), 0..5);
    assert_eq!(word(&mut q, 6), 6..11);
    assert_eq!(word(&mut q, 13), 13..16);
    assert_eq!(word(&mut q, 22), 22..25);
    assert_eq!(word(&mut q, 25), 22..25);
}

#[test]
fn checkbox_link_and_node_hits() {
    let env = env();
    let doc =
        doc("Title\n\n- [ ] Milk\n- [x] Bread\n\nSee [the map](https://x.y) and #tag\n\n---\n");
    let view = DocumentView::new();
    frame(&view, &doc, &env);
    let mut q = view.query(layout_input(&doc, &env.style), &env.ts);
    let milk = doc.blocks()[1].id();
    let bread = doc.blocks()[2].id();
    let row = q.block_bounds(milk).unwrap();
    let text = q.caret_bounds(pos(milk, 0)).unwrap();
    let checkbox = point(row.origin.x + px(10.), text.center().y);
    assert_eq!(q.checkbox_at(checkbox), Some(milk));
    assert_eq!(
        q.checkbox_at(point(text.origin.x + px(20.), text.center().y)),
        None
    );
    let row = q.block_bounds(bread).unwrap();
    assert_eq!(
        q.checkbox_at(point(row.origin.x + px(10.), row.center().y)),
        Some(bread)
    );

    let para = doc.blocks()[3].id();
    let link = q.caret_bounds(pos(para, 6)).unwrap();
    let (block, span) = q
        .link_at(point(link.origin.x + px(2.), link.center().y))
        .unwrap();
    assert_eq!(block, para);
    assert_eq!(span.range, 4..11);
    let plain = q.caret_bounds(pos(para, 1)).unwrap();
    assert!(
        q.link_at(point(plain.origin.x + px(2.), plain.center().y))
            .is_none()
    );
    let tag = q.caret_bounds(pos(para, 17)).unwrap();
    assert!(
        q.link_at(point(tag.origin.x + px(2.), tag.center().y))
            .is_some()
    );

    let rule = doc.blocks()[4].id();
    let r = q.block_bounds(rule).unwrap();
    assert_eq!(q.node_at(point(r.center().x, r.center().y)), Some(rule));
    assert_eq!(q.node_at(point(text.origin.x, text.center().y)), None);
}

#[test]
fn bounds_for_range_gives_the_first_line() {
    let env = env();
    let doc = doc(&format!("Title\n\n{LONG}\n"));
    let view = DocumentView::new();
    frame(&view, &doc, &env);
    let mut q = view.query(layout_input(&doc, &env.style), &env.ts);
    let id = doc.blocks()[1].id();
    let r = q.bounds_for_range(id, 4..LONG.len()).unwrap();
    let a = q.caret_bounds(pos(id, 4)).unwrap();
    assert_eq!(r.origin, a.origin);
    assert_eq!(r.size.height, a.size.height);
    assert!(r.size.width > px(100.));
}

#[test]
fn only_changed_blocks_are_shaped_again() {
    let env = env();
    let mut doc = doc("Title\n\nOne\n\nTwo\n\nThree\n");
    let view = DocumentView::new();
    frame(&view, &doc, &env);
    let before = view.stats();
    assert_eq!(before.shaped_texts, 4);

    // Nothing changed: nothing shaped.
    frame(&view, &doc, &env);
    assert_eq!(view.stats().shaped_texts, before.shaped_texts);

    // A changed block misses the cache even without invalidation.
    doc.blocks[2].content = Inline::plain("Two, changed");
    frame(&view, &doc, &env);
    let after = view.stats();
    assert_eq!(after.shaped_texts, before.shaped_texts + 1);
    assert_eq!(after.block_layouts, before.block_layouts + 1);

    // Explicit invalidation drops exactly that block.
    view.invalidate([doc.blocks()[3].id()]);
    frame(&view, &doc, &env);
    assert_eq!(view.stats().shaped_texts, after.shaped_texts + 1);

    view.invalidate_all();
    frame(&view, &doc, &env);
    assert_eq!(view.stats().shaped_texts, after.shaped_texts + 5);
}

fn long_doc(blocks: usize) -> Document {
    let mut md = String::from("A long note\n\n");
    for i in 1..blocks {
        if i % 10 == 0 {
            md.push_str(&format!("## Section {i}\n\n"));
        } else {
            md.push_str(&format!(
                "Paragraph {i} with some text that is long enough to wrap onto a second line \
                 in the content column of the editor.\n\n"
            ));
        }
    }
    doc(&md)
}

#[test]
fn virtualization_keeps_shaping_bounded() {
    let env = env();
    let doc = long_doc(10_000);
    assert_eq!(doc.blocks().len(), 10_000);
    let view = DocumentView::new();
    frame(&view, &doc, &env);
    let stats = view.stats();
    assert!(stats.visible_blocks < 60, "{stats:?}");
    assert!(stats.shaped_texts < 60, "{stats:?}");

    // Jump to the middle: still bounded, and estimates keep the total sane.
    let mid = doc.blocks()[5_000].id();
    view.request_reveal(TextRange {
        start: pos(mid, 0),
        end: pos(mid, 0),
    });
    frame(&view, &doc, &env);
    let stats = view.stats();
    assert!(stats.shaped_texts < 150, "{stats:?}");
    assert!(stats.cached_blocks < 150, "{stats:?}");
    let mut q = view.query(layout_input(&doc, &env.style), &env.ts);
    let caret = q.caret_bounds(pos(mid, 0)).unwrap();
    assert!(
        caret.origin.y >= px(0.) && caret.bottom() <= px(700.),
        "{caret:?}"
    );

    // Scrolling through many screens evicts old layouts.
    drop(q);
    for _ in 0..200 {
        view.query(layout_input(&doc, &env.style), &env.ts)
            .scroll_by(px(700.));
        frame(&view, &doc, &env);
    }
    assert!(view.stats().cached_blocks < 600, "{:?}", view.stats());
}

#[test]
fn correcting_estimates_does_not_move_the_viewport() {
    let env = env();
    let doc = long_doc(2_000);
    let view = DocumentView::new();
    let target = doc.blocks()[1_000].id();
    view.request_reveal(TextRange {
        start: pos(target, 0),
        end: pos(target, 0),
    });
    frame(&view, &doc, &env);
    let y = |view: &DocumentView| {
        view.query(layout_input(&doc, &env.style), &env.ts)
            .caret_bounds(pos(target, 0))
            .unwrap()
            .origin
            .y
    };
    let before = y(&view);
    // Lay out many blocks above (their real heights differ from the
    // estimates) through queries, then paint again.
    {
        let mut q = view.query(layout_input(&doc, &env.style), &env.ts);
        for block in &doc.blocks()[100..900] {
            q.caret_bounds(pos(block.id(), 0));
        }
    }
    assert_eq!(y(&view), before);
    frame(&view, &doc, &env);
    assert_eq!(y(&view), before);
}

#[test]
fn scrolling_reveals_and_pages() {
    let env = env();
    let doc = long_doc(300);
    let view = DocumentView::new();
    frame(&view, &doc, &env);
    let mut q = view.query(layout_input(&doc, &env.style), &env.ts);
    assert_eq!(q.scroll_top(), px(0.));
    let p = pos(doc.blocks()[1].id(), 0);
    let goal = q.caret_bounds(p).unwrap().origin.x;
    let down = q.page_down(p, goal).unwrap();
    assert!(q.scroll_top() > px(500.));
    let caret = q.caret_bounds(down).unwrap();
    assert!(caret.origin.y > px(0.) && caret.bottom() < px(700.));
    let up = q.page_up(down, goal).unwrap();
    assert_eq!(q.scroll_top(), px(0.));
    assert_eq!(up.block, p.block);

    let far = pos(doc.blocks()[250].id(), 3);
    q.reveal(far);
    let caret = q.caret_bounds(far).unwrap();
    assert!(
        caret.origin.y > px(0.) && caret.bottom() < px(700.),
        "{caret:?}"
    );
    // Scrolling past the end clamps.
    q.scroll_by(px(1e7));
    let max = q.content_height() - px(700.);
    assert!((q.scroll_top() - max).abs() < px(1.));
    q.scroll_by(px(-1e8));
    assert_eq!(q.scroll_top(), px(0.));
}

#[test]
fn odd_documents_do_not_panic() {
    let env = env();
    let long_line = "word ".repeat(5_000);
    let samples = [
        String::new(),
        "\n\n\n".to_owned(),
        "#\n\n-\n\n- [ ]\n\n>\n\n```\n```\n\n|a|\n|-|\n\n![](x)\n".to_owned(),
        long_line.clone(),
        "Line one  \nline two\\\nline three\n".to_owned(),
        "שלום עולם, مرحبا بالعالم\n\n日本語のテキストと中文，还有 한국어\n\n👩‍👩‍👧‍👦 emoji 🇩🇪 and e\u{301}\n".to_owned(),
        "<div>\nraw html\n</div>\n\n[^1]: footnote\n".to_owned(),
        "> quote\n> > nested\n>\n> - item\n".to_owned(),
        "1. a\n   1. b\n      1. c\n         - d\n".to_owned(),
    ];
    for md in &samples {
        let doc = doc(md);
        let view = DocumentView::new();
        let ids: Vec<BlockId> = {
            let mut ids = Vec::new();
            fn walk(b: &[crate::Block], ids: &mut Vec<BlockId>) {
                for b in b {
                    ids.push(b.id());
                    if let BlockKind::Table(t) = b.kind() {
                        ids.extend(t.rows.iter().flatten().map(|c| c.id()));
                    }
                    walk(b.children(), ids);
                }
            }
            walk(doc.blocks(), &mut ids);
            ids
        };
        let selection = ids.first().map(|id| Selection::Text {
            anchor: pos(*id, 0),
            head: pos(*ids.last().unwrap(), 10_000),
        });
        let _ = view.prepare_frame(
            &input(&doc, &env.style, selection.as_ref()),
            bounds(),
            &env.ts,
        );
        let mut q = view.query(layout_input(&doc, &env.style), &env.ts);
        for y in [-50., 0., 10., 300., 5_000.] {
            for x in [-20., 0., 400., 2_000.] {
                let p = point(px(x), px(y));
                if let Some(at) = q.position_at(p) {
                    q.caret_bounds(at).unwrap();
                }
                q.node_at(p);
                q.checkbox_at(p);
                q.link_at(p);
            }
        }
        for id in &ids {
            for offset in [0, 1, 2, 7, 10_000] {
                let p = pos(*id, offset);
                q.caret_bounds(p);
                q.position_above(p, px(100.));
                q.position_below(p, px(100.));
                q.line_start(p);
                q.line_end(p);
                q.word_range(p);
                q.bounds_for_range(*id, 0..offset);
                q.reveal(p);
            }
            q.block_bounds(*id);
        }
    }
}

/// Run with `cargo test --release --lib view::tests::timing -- --ignored --nocapture`.
#[test]
#[ignore = "timing, run in release mode"]
fn timing() {
    let env = env();
    let mut md = String::new();
    for i in 0..10_000 {
        md.push_str(&format!(
            "Line {i} of a long document with **some** text.\n\n"
        ));
    }
    let mut doc = doc(&md);
    let view = DocumentView::new();
    let mid = doc.blocks()[5_000].id();
    view.request_reveal(TextRange {
        start: pos(mid, 0),
        end: pos(mid, 0),
    });
    frame(&view, &doc, &env);

    let mut worst = std::time::Duration::ZERO;
    let mut total = std::time::Duration::ZERO;
    let n = 50;
    for i in 0..n {
        // A keystroke: one more character in a visible block.
        doc.blocks[5_000]
            .content
            .text
            .push(if i % 2 == 0 { 'x' } else { 'y' });
        let caret = pos(mid, doc.blocks[5_000].content.text.len());
        let selection = Selection::caret(caret);
        let start = Instant::now();
        view.invalidate([mid]);
        let _ = view.prepare_frame(
            &input(&doc, &env.style, Some(&selection)),
            bounds(),
            &env.ts,
        );
        let mut q = view.query(layout_input(&doc, &env.style), &env.ts);
        q.reveal(caret);
        let elapsed = start.elapsed();
        worst = worst.max(elapsed);
        total += elapsed;
    }
    println!(
        "keystroke in 10,000 lines: layout + paint list {:?} avg, {:?} worst",
        total / n,
        worst
    );

    let mut big = String::new();
    let mut i = 0;
    while big.len() < 1_000_000 {
        big.push_str(&format!(
            "Paragraph {i} with a [link](https://example.com) and enough words to wrap onto \
             another line of the column.\n\n- item {i}\n- [ ] task {i}\n\n"
        ));
        i += 1;
    }
    let start = Instant::now();
    let doc = self::doc(&big);
    let parsed = start.elapsed();
    let view = DocumentView::new();
    let start = Instant::now();
    frame(&view, &doc, &env);
    println!(
        "open 1 MB ({} blocks): parse {:?}, first layout + paint list {:?}, {} texts shaped",
        doc.blocks().len(),
        parsed,
        start.elapsed(),
        view.stats().shaped_texts
    );
}
