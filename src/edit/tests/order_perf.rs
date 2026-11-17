//! Timing of the core on a large document. Run with
//! `cargo test --release --lib edit::tests::order_perf -- --ignored --nocapture`.

use std::time::{Duration, Instant};

use super::*;

fn large_document(blocks: usize) -> String {
    let mut out = String::new();
    for index in 0..blocks {
        match index % 5 {
            0 => out.push_str(&format!("## Heading {index}\n\n")),
            1 => out.push_str(&format!("- item **{index}** with [a link](/u)\n")),
            2 => out.push_str(&format!("- [ ] task {index}\n\n")),
            3 => out.push_str(&format!("1. ordered {index}\n\n")),
            _ => out.push_str(&format!(
                "A paragraph with *some* text, number {index}.\n\n"
            )),
        }
    }
    out
}

#[test]
#[ignore = "timing, run in release"]
fn typing_in_a_large_document() {
    let source = large_document(10_000);
    let mut state = EditorState::new(Document::parse_with(&source, extensions()));
    assert!(state.doc.blocks.len() >= 10_000);
    // Near the end, where a search from the start is slowest.
    let target = state.doc.blocks[9_990].id;
    let end = state.end_of(target);
    state.set_selection(Selection::caret(end));

    let mut now = Instant::now();
    let text: Vec<char> = "the quick brown fox jumps over the lazy dog "
        .chars()
        .cycle()
        .take(1000)
        .collect();
    let mut worst = Duration::ZERO;
    let started = Instant::now();
    for ch in &text {
        now += Duration::from_millis(30);
        let before = Instant::now();
        state.insert_text(ch.encode_utf8(&mut [0; 4]), now);
        let _ = state.take_changes();
        worst = worst.max(before.elapsed());
    }
    let total = started.elapsed();
    println!(
        "typing: {:.1} µs per keystroke on average, {:.1} µs worst ({} blocks)",
        total.as_secs_f64() * 1e6 / text.len() as f64,
        worst.as_secs_f64() * 1e6,
        state.doc.blocks.len()
    );

    let started = Instant::now();
    for _ in 0..100 {
        now += Duration::from_millis(30);
        state.enter(now);
        state.insert_text("x", now);
    }
    println!(
        "enter + keystroke: {:.1} µs on average",
        started.elapsed().as_secs_f64() * 1e6 / 100.0
    );

    let started = Instant::now();
    for _ in 0..100 {
        state.undo();
    }
    println!(
        "undo: {:.1} µs on average",
        started.elapsed().as_secs_f64() * 1e6 / 100.0
    );
    check_invariants(&state).unwrap();
}
