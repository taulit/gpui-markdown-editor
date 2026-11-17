//! Headless benchmarks: parser, serializer, HTML conversion and the
//! editing core without layout.
//!
//! `cargo bench --bench markdown`

mod common;

use std::hint::black_box;
use std::time::{Duration, Instant};

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use gpui_markdown_editor::bench::EditCore;
use gpui_markdown_editor::extensions::{Highlight, Tag, WikiLink};
use gpui_markdown_editor::{Document, Extensions, TextPos};

fn extensions() -> Extensions {
    Extensions::from(vec![
        Box::new(WikiLink) as Box<dyn gpui_markdown_editor::Extension>,
        Box::new(Tag),
        Box::new(Highlight),
    ])
}

fn parse_and_serialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("markdown");
    for (name, bytes) in [("10KB", 10 * 1024), ("1MB", 1024 * 1024)] {
        let md = common::document_of_size(bytes);
        group.throughput(Throughput::Bytes(md.len() as u64));
        group.bench_function(format!("parse/{name}"), |b| {
            b.iter(|| Document::parse_with(black_box(&md), extensions()))
        });
        let doc = Document::parse_with(&md, extensions());
        group.bench_function(format!("serialize_unchanged/{name}"), |b| {
            b.iter(|| black_box(&doc).to_markdown())
        });
        group.bench_function(format!("serialize_normalized/{name}"), |b| {
            b.iter(|| black_box(&doc).normalized())
        });
    }
    group.finish();
}

#[cfg(feature = "paste-html")]
fn html(c: &mut Criterion) {
    let html = common::html_of_size(100 * 1024);
    let mut group = c.benchmark_group("html");
    group.throughput(Throughput::Bytes(html.len() as u64));
    group.bench_function("html_to_markdown/100KB", |b| {
        b.iter(|| gpui_markdown_editor::markdown::html_to_markdown(black_box(&html)))
    });
    group.finish();
}

#[cfg(not(feature = "paste-html"))]
fn html(_: &mut Criterion) {}

/// One typed character in the middle of a 10,000-block document: the
/// transaction, input rules, history and change set, without layout.
fn keystroke_in_core(c: &mut Criterion) {
    let md = common::document_of_blocks(10_000);
    let mut core = EditCore::new(&md, extensions());
    let middle = core.document().blocks().len() / 2 + 1;
    let block = core.document().blocks()[middle].id();
    let end = core.document().blocks()[middle].text().len();
    core.put_caret(TextPos { block, offset: end });
    let mut group = c.benchmark_group("core");
    // Each typed character is deleted again outside the timing, so the
    // paragraph keeps its length.
    group.bench_function("keystroke/10k_blocks", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let start = Instant::now();
                core.type_text("x");
                total += start.elapsed();
                core.backspace();
            }
            total
        })
    });
    group.finish();
}

criterion_group!(benches, parse_and_serialize, html, keystroke_in_core);
criterion_main!(benches);
