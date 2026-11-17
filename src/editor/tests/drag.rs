//! Dropping image files and dragging blocks by their handle.

use std::sync::{Arc, Mutex};

use gpui::{
    Entity, ExternalPaths, FileDropEvent, Modifiers, MouseButton, Pixels, Point, TestAppContext,
    VisualTestContext, point, px,
};

use super::*;
use crate::meta::DocMeta;

/// The drag handle of top-level block `ix`.
fn handle(editor: &Entity<MarkdownEditor>, cx: &mut VisualTestContext, ix: usize) -> Point<Pixels> {
    editor.update_in(cx, |editor, window, _| {
        let ts = window.text_system().clone();
        let view = editor.view.clone();
        let mut q = view.query(editor.layout_input(), &ts);
        q.drag_handle(ix).expect("handle").center()
    })
}

fn texts(editor: &Entity<MarkdownEditor>, cx: &mut VisualTestContext) -> Vec<String> {
    editor.read_with(cx, |e, _| {
        e.document()
            .blocks()
            .iter()
            .map(|b| b.text().to_string())
            .collect()
    })
}

fn drag(cx: &mut VisualTestContext, from: Point<Pixels>, to: Point<Pixels>) {
    cx.simulate_mouse_move(from, None, Modifiers::none());
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
}

#[gpui::test]
fn handle_moves_a_block_in_one_undo_step(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "One\n\nTwo\n\n- Three\n  - nested\n");
    let from = handle(&editor, cx, 2);
    // Onto the upper half of the first block: before it.
    let b = block(&editor, cx, 0);
    let first = point_of(
        &editor,
        cx,
        TextPos {
            block: b,
            offset: 0,
        },
    );
    drag(cx, from, point(first.x, first.y - px(4.)));
    assert_eq!(md(&editor, cx), "- Three\n  - nested\n\nOne\n\nTwo\n");
    press(cx, "secondary-z");
    assert_eq!(md(&editor, cx), "One\n\nTwo\n\n- Three\n  - nested\n");

    // Down to the end.
    let from = handle(&editor, cx, 0);
    let b = block(&editor, cx, 2);
    let last = point_of(
        &editor,
        cx,
        TextPos {
            block: b,
            offset: 0,
        },
    );
    drag(cx, from, point(last.x, last.y + px(30.)));
    assert_eq!(texts(&editor, cx), ["Two", "Three", "One"]);
}

#[gpui::test]
fn a_click_on_the_handle_puts_the_caret_in_the_block(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "One\n\nTwo\n");
    let p = handle(&editor, cx, 1);
    cx.simulate_mouse_move(p, None, Modifiers::none());
    cx.simulate_click(p, Modifiers::none());
    assert_eq!(caret(&editor, cx), "‸Two");
    assert_eq!(md(&editor, cx), "One\n\nTwo\n");
}

#[gpui::test]
fn a_folded_heading_moves_with_its_section(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "Intro\n\n# A\n\na\n\n# B\n\nb\n");
    let mut meta = DocMeta::default();
    meta.folded_headings.insert(0);
    editor.update(cx, |e, cx| e.set_meta(meta, cx));
    cx.run_until_parked();
    let from = handle(&editor, cx, 1);
    let b = block(&editor, cx, 4);
    let end = point_of(
        &editor,
        cx,
        TextPos {
            block: b,
            offset: 0,
        },
    );
    drag(cx, from, point(end.x, end.y + px(30.)));
    assert_eq!(texts(&editor, cx), ["Intro", "B", "b", "A", "a"]);
    // Still folded, now with ordinal 1.
    assert_eq!(
        editor.read_with(cx, |e, _| e
            .meta()
            .folded_headings
            .iter()
            .copied()
            .collect::<Vec<_>>()),
        [1]
    );
}

#[gpui::test]
fn dropped_image_files_go_through_on_image(cx: &mut TestAppContext) {
    let dir = std::env::temp_dir().join(format!("gme-drop-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let image = dir.join("shot.png");
    std::fs::write(&image, b"\x89PNG fake").unwrap();
    let other = dir.join("notes.txt");
    std::fs::write(&other, b"text").unwrap();

    let received = Arc::new(Mutex::new(Vec::new()));
    let sink = received.clone();
    let options = EditorOptions {
        on_image: Some(Arc::new(move |bytes: Vec<u8>, mime: &str| {
            sink.lock().unwrap().push((bytes.len(), mime.to_string()));
            Some("assets/shot.png".to_string())
        })),
        ..options()
    };
    let (editor, cx) = open_with(cx, "Text\n", options);
    let b = block(&editor, cx, 0);
    let at = point_of(
        &editor,
        cx,
        TextPos {
            block: b,
            offset: 2,
        },
    );
    cx.simulate_event(FileDropEvent::Entered {
        position: at,
        paths: ExternalPaths(vec![other, image].into()),
    });
    cx.simulate_event(FileDropEvent::Submit { position: at });
    cx.run_until_parked();
    assert_eq!(*received.lock().unwrap(), [(9, "image/png".to_string())]);
    assert_eq!(md(&editor, cx), "Text\n\n![](assets/shot.png)\n");
    std::fs::remove_dir_all(&dir).ok();
}

#[gpui::test]
fn dropped_images_skip_large_and_special_files(cx: &mut TestAppContext) {
    let dir = std::env::temp_dir().join(format!("gme-drop-bg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let image = dir.join("shot.png");
    std::fs::write(&image, b"\x89PNG fake").unwrap();
    // Larger than the cap (sparse, nothing is written).
    let huge = dir.join("huge.png");
    std::fs::File::create(&huge)
        .unwrap()
        .set_len(crate::view::images::MAX_IMAGE_BYTES + 1)
        .unwrap();
    let mut paths = vec![huge, image];
    #[cfg(unix)]
    {
        // A FIFO named like an image must not block.
        let fifo = dir.join("pipe.png");
        if std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .is_ok_and(|s| s.success())
        {
            paths.insert(0, fifo);
        }
    }

    let received = Arc::new(Mutex::new(Vec::new()));
    let sink = received.clone();
    let options = EditorOptions {
        on_image: Some(Arc::new(move |bytes: Vec<u8>, _: &str| {
            sink.lock().unwrap().push(bytes.len());
            Some("assets/shot.png".to_string())
        })),
        ..options()
    };
    let (editor, cx) = open_with(cx, "Text\n", options);
    let b = block(&editor, cx, 0);
    let at = point_of(
        &editor,
        cx,
        TextPos {
            block: b,
            offset: 4,
        },
    );
    cx.simulate_event(FileDropEvent::Entered {
        position: at,
        paths: ExternalPaths(paths.into()),
    });
    cx.simulate_event(FileDropEvent::Submit { position: at });
    cx.run_until_parked();
    assert_eq!(*received.lock().unwrap(), [9]);
    assert_eq!(md(&editor, cx), "Text\n\n![](assets/shot.png)\n");
    std::fs::remove_dir_all(&dir).ok();
}
