//! Benchmarks with the view: a keystroke followed by a frame, opening a
//! large document, find. Runs in GPUI's headless test window with the
//! platform's real text system, so shaping and layout are included; GPU
//! painting is not (the test window records the scene and draws nothing).
//!
//! `cargo bench --bench editor`

mod common;

use std::hint::black_box;
use std::time::{Duration, Instant};

use criterion::{Criterion, criterion_group, criterion_main};
use gpui::{Entity, Focusable as _, TestAppContext, TestDispatcher, VisualTestContext, px, size};
use gpui_markdown_editor::extensions::{Highlight, Tag, WikiLink};
use gpui_markdown_editor::{EditorOptions, MarkdownEditor, TextPos};

fn app() -> TestAppContext {
    let text_system = gpui_platform::current_platform(true).text_system();
    TestAppContext::build_with_text_system(TestDispatcher::new(0), None, text_system)
}

/// An editor with `md` in a focused 800x600 window.
fn open<'a>(
    app: &'a mut TestAppContext,
    md: &str,
) -> (Entity<MarkdownEditor>, &'a mut VisualTestContext) {
    app.update(|cx| {
        gpui_component::init(cx);
        gpui_markdown_editor::init(cx);
    });
    let md = md.to_string();
    let (editor, cx) = app.add_window_view(move |window, cx| {
        let options = EditorOptions {
            extensions: vec![Box::new(WikiLink), Box::new(Tag), Box::new(Highlight)],
            ..Default::default()
        };
        let mut editor = MarkdownEditor::new(window, cx, options);
        editor.set_markdown(&md, cx);
        editor
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.update(|window, cx| editor.focus_handle(cx).focus(window, cx));
    cx.run_until_parked();
    draw(cx);
    (editor, cx)
}

/// Lays out and records one frame.
fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

/// A typed character in the middle of a 10,000-block document, then a
/// frame: input handler, editing core, layout of the changed block and
/// the visible ones, scene recording. Budget: 4 ms.
fn keystroke_and_frame(c: &mut Criterion) {
    let mut app = app();
    let (editor, cx) = open(&mut app, &common::document_of_blocks(10_000));
    let (block, end) = editor.read_with(cx, |e, _| {
        let blocks = e.document().blocks();
        let b = &blocks[blocks.len() / 2 + 1];
        (b.id(), b.text().len())
    });
    let pos = TextPos { block, offset: end };
    editor.update(cx, |e, cx| e.__set_selection(pos, pos, cx));
    cx.run_until_parked();
    draw(cx);

    // The character is deleted again outside the timing, so the paragraph
    // keeps its length.
    c.bench_function("editor/keystroke_and_frame/10k_blocks", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let start = Instant::now();
                cx.simulate_input("x");
                draw(cx);
                total += start.elapsed();
                cx.simulate_keystrokes("backspace");
                draw(cx);
            }
            total
        })
    });
}

/// `set_markdown` with 1 MB and the first frame. Budget: 50 ms.
fn open_large(c: &mut Criterion) {
    let md = common::document_of_size(1024 * 1024);
    let mut app = app();
    let (editor, cx) = open(&mut app, "");
    let mut group = c.benchmark_group("editor");
    group.sample_size(20);
    group.bench_function("open/1MB", |b| {
        b.iter(|| {
            editor.update(cx, |e, cx| e.set_markdown(&md, cx));
            draw(cx);
        })
    });
    group.bench_function("first_layout/1MB", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                editor.update(cx, |e, cx| e.set_markdown(&md, cx));
                let start = Instant::now();
                draw(cx);
                total += start.elapsed();
            }
            total
        })
    });
    group.bench_function("find/1MB", |b| {
        b.iter(|| editor.read_with(cx, |e, _| e.find(black_box("wrap")).len()))
    });
    group.finish();
}

criterion_group!(benches, keystroke_and_frame, open_large);
criterion_main!(benches);
