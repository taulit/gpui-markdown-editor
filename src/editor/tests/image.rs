//! Images: loading, failure, resizing, insertion and pasting.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{ClipboardItem, Image, ImageFormat, Modifiers, MouseButton, TestAppContext};

use super::*;
use crate::command::Command;

/// A directory with a 200x100 `pic.png` and a corrupt `bad.png`.
fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gme-images-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("pic.png"), png(200, 100)).unwrap();
    std::fs::write(dir.join("bad.png"), b"\x89PNG not really").unwrap();
    dir
}

fn png(width: u32, height: u32) -> Vec<u8> {
    let img = ::image::RgbaImage::from_fn(width, height, |x, _| {
        ::image::Rgba([(x % 256) as u8, 120, 200, 255])
    });
    let mut out = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut out),
        ::image::ImageFormat::Png,
    )
    .unwrap();
    out
}

fn options_in(dir: &Path) -> EditorOptions {
    let dir = dir.to_path_buf();
    EditorOptions {
        resolve_src: Some(Arc::new(move |src: &str| {
            let path = dir.join(src);
            path.exists().then_some(path)
        })),
        ..options()
    }
}

fn image_state(editor: &Entity<MarkdownEditor>, cx: &mut VisualTestContext, src: &str) -> String {
    editor
        .read_with(cx, |e, _| e.view.image_state(src).0)
        .unwrap_or_else(|| "none".into())
}

fn bounds(
    editor: &Entity<MarkdownEditor>,
    cx: &mut VisualTestContext,
    ix: usize,
) -> gpui::Bounds<Pixels> {
    let id = block(editor, cx, ix);
    editor.update_in(cx, |editor, window, _| {
        let ts = window.text_system().clone();
        let view = editor.view.clone();
        view.query(editor.layout_input(), &ts)
            .block_bounds(id)
            .expect("bounds")
    })
}

/// Lets the images load and the editor draw again.
fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, _| window.refresh());
    }
    cx.run_until_parked();
}

#[gpui::test]
fn images_load_and_failures_keep_the_placeholder(cx: &mut TestAppContext) {
    let dir = fixture("load");
    let (editor, cx) = open_with(
        cx,
        "intro\n\n![Lake](pic.png)\n\n![Gone](missing.png)\n\n![Broken](bad.png)\n",
        options_in(&dir),
    );
    settle(cx);
    assert_eq!(image_state(&editor, cx, "pic.png"), "Ready(200x100)");
    assert_eq!(image_state(&editor, cx, "missing.png"), "Failed");
    assert_eq!(image_state(&editor, cx, "bad.png"), "Failed");

    // Loaded: natural size with the aspect ratio; failed: the placeholder.
    let b = bounds(&editor, cx, 1);
    assert_eq!(b.size.height, px(100. + 24.));
    let placeholder = editor.read_with(cx, |e, _| e.style().image_placeholder_height);
    assert_eq!(bounds(&editor, cx, 2).size.height, placeholder + px(24.));
    assert_eq!(bounds(&editor, cx, 3).size.height, placeholder + px(24.));
    std::fs::remove_dir_all(dir).ok();
}

#[cfg(unix)]
#[gpui::test]
fn device_paths_fail_fast_into_the_placeholder(cx: &mut TestAppContext) {
    // Without `resolve_src` absolute paths load as they are: a device is
    // refused instead of being read without end.
    let (editor, cx) = open(cx, "![z](/dev/zero)\n\n![d](/tmp)\n");
    settle(cx);
    assert_eq!(image_state(&editor, cx, "/dev/zero"), "Failed");
    assert_eq!(image_state(&editor, cx, "/tmp"), "Failed");
}

#[gpui::test]
fn dragging_a_handle_resizes_the_image(cx: &mut TestAppContext) {
    let dir = fixture("resize");
    let (editor, cx) = open_with(cx, "intro\n\n![Lake](pic.png)\n", options_in(&dir));
    settle(cx);
    let id = block(&editor, cx, 1);
    let b = bounds(&editor, cx, 1);
    // A click selects the image.
    let image_center = point(b.left() + px(100.), b.top() + px(12. + 50.));
    cx.simulate_click(image_center, Modifiers::none());
    assert_eq!(caret(&editor, cx), "<node>");
    cx.run_until_parked();

    let handle = point(b.left() + px(200. - 8. - 3.5), image_center.y);
    let target = point(handle.x - px(60.), handle.y);
    cx.simulate_mouse_down(handle, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(target, MouseButton::Left, Modifiers::none());
    // The preview shows before the width is applied.
    assert_eq!(bounds(&editor, cx, 1).size.height, px(70. + 24.));
    assert_eq!(md(&editor, cx), "intro\n\n![Lake](pic.png)\n");
    cx.simulate_mouse_up(target, MouseButton::Left, Modifiers::none());
    assert_eq!(md(&editor, cx), "intro\n\n![Lake|140](pic.png)\n");
    assert_eq!(
        editor.read_with(cx, |e, _| e.state.selection.clone()),
        Selection::Node(id)
    );

    press(cx, "secondary-z");
    assert_eq!(md(&editor, cx), "intro\n\n![Lake](pic.png)\n");
    std::fs::remove_dir_all(dir).ok();
}

#[gpui::test]
fn insert_image_command_and_pasted_image_data(cx: &mut TestAppContext) {
    let dir = fixture("insert");
    let received: Arc<std::sync::Mutex<Vec<(usize, String)>>> = Default::default();
    let options = EditorOptions {
        on_image: Some({
            let received = received.clone();
            Arc::new(move |bytes: Vec<u8>, mime: &str| {
                received
                    .lock()
                    .unwrap()
                    .push((bytes.len(), mime.to_string()));
                Some("pasted.png".to_string())
            })
        }),
        ..options_in(&dir)
    };
    let (editor, cx) = open_with(cx, "first\n", options);
    editor.update_in(cx, |e, window, cx| {
        e.command(
            Command::InsertImage {
                src: "pic.png".into(),
                alt: "Lake".into(),
            },
            window,
            cx,
        )
    });
    assert_eq!(md(&editor, cx), "first\n\n![Lake](pic.png)\n");
    settle(cx);
    assert_eq!(image_state(&editor, cx, "pic.png"), "Ready(200x100)");

    let data = png(4, 4);
    cx.write_to_clipboard(ClipboardItem::new_image(&Image::from_bytes(
        ImageFormat::Png,
        data.clone(),
    )));
    let first = block(&editor, cx, 0);
    select(
        &editor,
        cx,
        TextPos {
            block: first,
            offset: 5,
        },
        TextPos {
            block: first,
            offset: 5,
        },
    );
    press(cx, "secondary-v");
    assert_eq!(
        *received.lock().unwrap(),
        [(data.len(), "image/png".to_string())]
    );
    assert!(
        md(&editor, cx).contains("![](pasted.png)"),
        "{}",
        md(&editor, cx)
    );
    std::fs::remove_dir_all(dir).ok();
}

#[gpui::test]
fn only_images_near_the_viewport_load(cx: &mut TestAppContext) {
    let dir = fixture("many");
    for i in 0..150 {
        std::fs::copy(dir.join("pic.png"), dir.join(format!("p{i}.png"))).unwrap();
    }
    let mut text = String::new();
    for i in 0..150 {
        text.push_str(&format!("Paragraph {i}\n\n![Image {i}](p{i}.png)\n\n"));
    }
    let (editor, cx) = open_with(cx, &text, options_in(&dir));
    settle(cx);
    assert_eq!(image_state(&editor, cx, "p0.png"), "Ready(200x100)");
    assert_eq!(image_state(&editor, cx, "p149.png"), "none");
    let held = editor.read_with(cx, |e, _| e.view.image_state("p0.png").1);
    assert!(held < 20, "{held} images requested");
    std::fs::remove_dir_all(dir).ok();
}
