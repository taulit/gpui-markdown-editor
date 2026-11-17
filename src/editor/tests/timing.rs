//! End-to-end keystroke timing.

use std::time::{Duration, Instant};

use gpui::{TestAppContext, TestDispatcher};

use super::*;

/// A document of `lines` lines: sections with paragraphs and lists.
fn long_document(lines: usize) -> String {
    let mut md = String::from("A very long note\n\n");
    let mut n = 2;
    let mut section = 1;
    while n < lines {
        md.push_str(&format!("## Section {section}\n\n"));
        md.push_str(&format!(
            "Paragraph {n} of section {section}: some **bold** words, a [link](https://example.com) and enough text to wrap onto a second line in the content column.\n\n"
        ));
        md.push_str(&format!(
            "- Item one of {section}\n- [ ] Task of {section}\n- [x] Done in {section}\n\n"
        ));
        n += 9;
        section += 1;
    }
    md
}

/// A keystroke in the middle of a 10,000-line document: the input handler
/// edits the state, then the window lays out and paints a frame. Uses the
/// real text system, so shaping is included. Budget: 4 ms.
///
/// `cargo test --release --lib keystroke_in_a_long_document -- --ignored --nocapture`
#[test]
#[ignore = "timing, run in release mode"]
fn keystroke_in_a_long_document() {
    let text_system = gpui_platform::current_platform(true).text_system();
    let mut app = TestAppContext::build_with_text_system(TestDispatcher::new(0), None, text_system);
    let (editor, cx) = open(&mut app, &long_document(10_000));
    let middle = editor.read_with(cx, |e, _| e.document().blocks().len() / 2);
    let id = block(&editor, cx, middle);
    let pos = TextPos {
        block: id,
        offset: 0,
    };
    select(&editor, cx, pos, pos);
    press(cx, "end");

    let draw = |cx: &mut VisualTestContext| cx.update(|window, cx| window.draw(cx).clear(cx));
    draw(cx);
    let layouts = editor.read_with(cx, |e, _| e.view.stats().block_layouts);
    let mut times = Vec::new();
    for i in 0..60 {
        let start = Instant::now();
        cx.simulate_input(if i % 8 == 7 { " " } else { "x" });
        draw(cx);
        times.push(start.elapsed());
    }
    // Every frame laid out the edited block again.
    let layouts = editor.read_with(cx, |e, _| e.view.stats().block_layouts) - layouts;
    assert!(layouts >= 60, "{layouts} block layouts");
    let text = editor.read_with(cx, |e, _| e.document().blocks()[middle].text().to_string());
    assert_eq!(text.matches('x').count(), 53, "{text:?}");
    // The first keystrokes warm up caches.
    let mut times = times.split_off(10);
    times.sort();
    let median = times[times.len() / 2];
    let max = *times.last().unwrap();
    println!("keystroke + frame in a 10,000-line document: median {median:?}, max {max:?}");
    assert!(
        median < Duration::from_millis(4),
        "median {median:?} over the 4 ms budget"
    );
}
