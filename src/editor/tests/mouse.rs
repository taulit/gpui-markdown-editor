//! Clicks, drags, checkboxes and links.

use std::time::Duration;

use gpui::{Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, TestAppContext};

use super::*;
use crate::model::BlockKind;

fn pos(block: BlockId, offset: usize) -> TextPos {
    TextPos { block, offset }
}

fn click_n(cx: &mut VisualTestContext, p: Point<Pixels>, count: usize, modifiers: Modifiers) {
    cx.simulate_event(MouseDownEvent {
        position: p,
        modifiers,
        button: MouseButton::Left,
        click_count: count,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        position: p,
        modifiers,
        button: MouseButton::Left,
        click_count: count,
    });
}

fn ctrl() -> Modifiers {
    if cfg!(target_os = "macos") {
        Modifiers::command()
    } else {
        Modifiers::control()
    }
}

/// A point on the checkbox of the task item `id`.
fn checkbox_point(
    editor: &Entity<MarkdownEditor>,
    cx: &mut VisualTestContext,
    id: BlockId,
) -> Point<Pixels> {
    let text = point_of(editor, cx, pos(id, 0));
    editor.update_in(cx, |editor, window, _| {
        let ts = window.text_system().clone();
        let view = editor.view.clone();
        let mut q = view.query(editor.layout_input(), &ts);
        (1..80)
            .map(|dx| point(text.x - px(dx as f32), text.y))
            .find(|p| q.checkbox_at(*p) == Some(id))
            .expect("a checkbox left of the text")
    })
}

#[gpui::test]
fn click_and_shift_click(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "first line\n\nsecond line\n");
    let (a, b) = (block(&editor, cx, 0), block(&editor, cx, 1));
    let events = record_events(&editor, cx);

    let p = point_of(&editor, cx, pos(b, 3));
    cx.simulate_click(p, Modifiers::none());
    assert_eq!(caret(&editor, cx), "sec‸ond line");
    assert_eq!(count(&events, &EditorEvent::SelectionChanged), 1);

    let p = point_of(&editor, cx, pos(a, 6));
    cx.simulate_click(p, Modifiers::shift());
    let selected = editor.read_with(cx, |e, _| e.state.selected_text());
    assert_eq!(selected, "line\nsec");

    // Clicks right of the text land at the end of the line.
    let p = point_of(&editor, cx, pos(a, 10));
    cx.simulate_click(point(p.x + px(200.), p.y), Modifiers::none());
    assert_eq!(caret(&editor, cx), "first line‸");
}

#[gpui::test]
fn drag_selects(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "one two three\n\nfour five\n");
    let (a, b) = (block(&editor, cx, 0), block(&editor, cx, 1));
    let start = point_of(&editor, cx, pos(a, 4));
    let end = point_of(&editor, cx, pos(b, 4));
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    let selected = editor.read_with(cx, |e, _| e.state.selected_text());
    assert_eq!(selected, "two three\nfour");

    // A move without the button pressed does not select.
    cx.simulate_mouse_move(start, None, Modifiers::none());
    let selected = editor.read_with(cx, |e, _| e.state.selected_text());
    assert_eq!(selected, "two three\nfour");
}

#[gpui::test]
fn double_click_selects_a_word_and_triple_click_a_block(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "alpha beta gamma\n\nnext\n");
    let a = block(&editor, cx, 0);
    let p = point_of(&editor, cx, pos(a, 8));
    click_n(cx, p, 2, Modifiers::none());
    assert_eq!(caret(&editor, cx), "alpha «beta‸ gamma");

    // Dragging after a double click extends by words.
    let next = block(&editor, cx, 1);
    let q = point_of(&editor, cx, pos(a, 13));
    cx.simulate_event(MouseDownEvent {
        position: p,
        modifiers: Modifiers::none(),
        button: MouseButton::Left,
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_mouse_move(q, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(q, MouseButton::Left, Modifiers::none());
    assert_eq!(caret(&editor, cx), "alpha «beta gamma‸");

    click_n(cx, p, 3, Modifiers::none());
    assert_eq!(caret(&editor, cx), "«alpha beta gamma‸");
    let _ = next;
}

#[gpui::test]
fn clicking_a_checkbox_toggles_it_in_one_undo_step(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "- [ ] milk\n- [x] bread\n");
    let events = record_events(&editor, cx);
    let milk = block(&editor, cx, 0);
    let p = checkbox_point(&editor, cx, milk);
    cx.simulate_click(p, Modifiers::none());
    assert_eq!(md(&editor, cx), "- [x] milk\n- [x] bread\n");
    assert_eq!(count(&events, &EditorEvent::Changed), 1);

    cx.simulate_click(p, Modifiers::none());
    assert_eq!(md(&editor, cx), "- [ ] milk\n- [x] bread\n");
    press(cx, "secondary-z");
    assert_eq!(md(&editor, cx), "- [x] milk\n- [x] bread\n");
    press(cx, "secondary-z");
    assert_eq!(md(&editor, cx), "- [ ] milk\n- [x] bread\n");
    assert!(!editor.read_with(cx, |e, _| e.is_dirty()));
}

#[gpui::test]
fn clicking_a_rule_selects_it(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "above\n\n---\n\nbelow\n");
    let (first, last) = (block(&editor, cx, 0), block(&editor, cx, 2));
    let above = point_of(&editor, cx, pos(first, 2));
    let below = point_of(&editor, cx, pos(last, 2));
    let middle = point(above.x, (above.y + below.y) / 2.);
    cx.simulate_click(middle, Modifiers::none());
    let selection = editor.read_with(cx, |e, _| e.state.selection.clone());
    assert_eq!(selection, Selection::Node(block(&editor, cx, 1)));
    // Typing with the rule selected starts a paragraph below it.
    cx.simulate_input("x");
    let kinds = editor.read_with(cx, |e, _| {
        e.document()
            .blocks()
            .iter()
            .map(|b| b.kind().clone())
            .collect::<Vec<_>>()
    });
    assert_eq!(kinds[1], BlockKind::Rule);
    assert_eq!(caret(&editor, cx), "x‸");
}

#[gpui::test]
fn ctrl_click_on_links_emits_events(cx: &mut TestAppContext) {
    let (editor, cx) = open(
        cx,
        "See [the map](https://example.com/map), [[Packing list|list]] and #travel.\n",
    );
    let events = record_events(&editor, cx);
    let id = block(&editor, cx, 0);
    let text = editor.read_with(cx, |e, _| e.document().blocks()[0].text().to_string());
    let at = |needle: &str| text.find(needle).unwrap() + 1;

    // A plain click places the caret.
    let p = point_of(&editor, cx, pos(id, at("map")));
    cx.simulate_click(p, Modifiers::none());
    assert!(
        events
            .borrow()
            .iter()
            .all(|e| matches!(e, EditorEvent::SelectionChanged))
    );

    cx.simulate_click(p, ctrl());
    let p = point_of(&editor, cx, pos(id, at("list")));
    cx.simulate_click(p, ctrl());
    let p = point_of(&editor, cx, pos(id, at("travel")));
    cx.simulate_click(p, ctrl());
    let clicks: Vec<_> = events
        .borrow()
        .iter()
        .filter(|e| !matches!(e, EditorEvent::SelectionChanged))
        .cloned()
        .collect();
    assert_eq!(
        clicks,
        [
            EditorEvent::LinkClicked("https://example.com/map".into()),
            EditorEvent::WikiLinkClicked("Packing list".into()),
            EditorEvent::TagClicked("travel".into()),
        ]
    );
}

#[gpui::test]
fn dragging_below_the_view_scrolls(cx: &mut TestAppContext) {
    let doc: String = (0..200).map(|i| format!("line {i}\n\n")).collect();
    let (editor, cx) = open(cx, &doc);
    let top = |editor: &Entity<MarkdownEditor>, cx: &mut VisualTestContext| {
        editor.update_in(cx, |e, window, _| {
            let ts = window.text_system().clone();
            let view = e.view.clone();
            view.query(e.layout_input(), &ts).scroll_top()
        })
    };
    assert_eq!(top(&editor, cx), px(0.));
    let third = block(&editor, cx, 2);
    let start = point_of(&editor, cx, pos(third, 1));
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    let outside = point(start.x, px(650.));
    cx.simulate_event(MouseMoveEvent {
        position: outside,
        pressed_button: Some(MouseButton::Left),
        modifiers: Modifiers::none(),
    });
    let after_move = top(&editor, cx);
    cx.executor().advance_clock(Duration::from_millis(400));
    cx.run_until_parked();
    let after_wait = top(&editor, cx);
    assert!(after_wait > after_move, "{after_move:?} -> {after_wait:?}");
    cx.simulate_mouse_up(outside, MouseButton::Left, Modifiers::none());

    // No more scrolling after the button is released.
    cx.executor().advance_clock(Duration::from_millis(400));
    cx.run_until_parked();
    assert_eq!(top(&editor, cx), after_wait);
    let head = editor.read_with(cx, |e, _| e.state.selection.head().unwrap());
    assert_ne!(head.block, block(&editor, cx, 2));
}

/// The scale of the pressed checkbox after the next frame.
fn checkbox_scale(editor: &Entity<MarkdownEditor>, cx: &mut VisualTestContext) -> Option<f32> {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    editor.read_with(cx, |e, _| e.checkbox_scale().map(|(_, s)| s))
}

#[gpui::test]
fn a_pressed_checkbox_springs_to_its_press_scale_and_back(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "- [ ] task\n");
    let item = block(&editor, cx, 0);
    let p = checkbox_point(&editor, cx, item);
    let button = |position| MouseDownEvent {
        position,
        modifiers: Modifiers::none(),
        button: MouseButton::Left,
        click_count: 1,
        first_mouse: false,
    };
    cx.simulate_event(button(p));
    assert_eq!(checkbox_scale(&editor, cx), Some(1.));
    cx.executor().advance_clock(Duration::from_millis(40));
    let s = checkbox_scale(&editor, cx).unwrap();
    assert!(s < 1. && s > super::super::press::PRESSED - 0.05, "{s}");
    cx.executor().advance_clock(Duration::from_millis(500));
    assert_eq!(checkbox_scale(&editor, cx), Some(super::super::press::PRESSED));

    cx.simulate_event(MouseUpEvent {
        position: p,
        modifiers: Modifiers::none(),
        button: MouseButton::Left,
        click_count: 1,
    });
    cx.executor().advance_clock(Duration::from_millis(40));
    assert!(checkbox_scale(&editor, cx).is_some_and(|s| s > super::super::press::PRESSED));
    cx.executor().advance_clock(Duration::from_millis(1000));
    assert_eq!(checkbox_scale(&editor, cx), None);
    assert!(md(&editor, cx).contains("[x]"));

    // With reduced motion the checkbox is not scaled at all.
    cx.update(|_, cx| cx.set_reduce_motion(true));
    cx.simulate_event(button(p));
    assert_eq!(checkbox_scale(&editor, cx), None);
}
