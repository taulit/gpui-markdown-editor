//! Attachments: a paragraph that is only a link to a local non-image file.

use std::path::PathBuf;
use std::sync::Arc;

use gpui::{ExternalPaths, FileDropEvent, Modifiers, TestAppContext};

use super::*;
use crate::view::Decoration;

const DOC: &str = "intro\n\n[report.pdf](report.pdf)\n\n[site](https://x.org/a.pdf)\n\n[pic](pic.png)\n\n[gone.zip](gone.zip)\n";

fn fixture() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gme-attach-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("report.pdf"), vec![0u8; 2048]).unwrap();
    dir
}

fn options_in(dir: PathBuf) -> EditorOptions {
    EditorOptions {
        resolve_src: Some(Arc::new(move |src: &str| Some(dir.join(src)))),
        ..options()
    }
}

/// Whether top-level block `ix` is drawn as an attachment row, and the
/// texts of its other decorations (the size label).
fn row(
    editor: &Entity<MarkdownEditor>,
    cx: &mut VisualTestContext,
    ix: usize,
) -> (bool, Vec<String>) {
    editor.update_in(cx, |editor, window, _| {
        let ts = window.text_system().clone();
        let view = editor.view.clone();
        let layout = view
            .query(editor.layout_input(), &ts)
            .layout(ix)
            .expect("layout");
        let attachment = layout
            .decorations
            .iter()
            .any(|d| matches!(d, Decoration::Attachment { .. }));
        let labels = layout
            .decorations
            .iter()
            .filter_map(|d| match d {
                Decoration::Text { text, .. } => Some(text.text.to_string()),
                _ => None,
            })
            .collect();
        (attachment, labels)
    })
}

#[gpui::test]
fn local_file_links_render_as_attachment_rows(cx: &mut TestAppContext) {
    let dir = fixture();
    let (editor, cx) = open_with(cx, DOC, options_in(dir.clone()));
    assert_eq!(row(&editor, cx, 1), (true, vec!["2.0 KB".to_string()]));
    assert!(!row(&editor, cx, 2).0, "a URL is a link");
    assert!(!row(&editor, cx, 3).0, "an image file is a link");
    // Not resolvable to a file: no size.
    assert_eq!(row(&editor, cx, 4), (true, Vec::new()));
    assert_eq!(md(&editor, cx), DOC, "the Markdown is untouched");
    std::fs::remove_dir_all(dir).ok();
}

#[gpui::test]
fn clicking_an_attachment_emits_its_event(cx: &mut TestAppContext) {
    let dir = fixture();
    let (editor, cx) = open_with(cx, DOC, options_in(dir.clone()));
    let events = record_events(&editor, cx);
    let b = block(&editor, cx, 1);
    let name = point_of(
        &editor,
        cx,
        TextPos {
            block: b,
            offset: 0,
        },
    );
    // The icon left of the name.
    let icon = gpui::point(name.x - px(18.), name.y);
    cx.simulate_click(icon, Modifiers::none());
    let clicked = EditorEvent::AttachmentClicked("report.pdf".into());
    assert_eq!(count(&events, &clicked), 1);

    // A plain click on the name places the caret, to edit it.
    let on_name = point_of(
        &editor,
        cx,
        TextPos {
            block: b,
            offset: 3,
        },
    );
    cx.simulate_click(on_name, Modifiers::none());
    assert_eq!(caret(&editor, cx), "rep‸ort.pdf");
    assert_eq!(count(&events, &clicked), 1);

    // Ctrl/Cmd+click on the name emits the attachment event, not a link.
    let ctrl = if cfg!(target_os = "macos") {
        Modifiers::command()
    } else {
        Modifiers::control()
    };
    cx.simulate_click(on_name, ctrl);
    assert_eq!(count(&events, &clicked), 2);
    assert!(
        !events
            .borrow()
            .iter()
            .any(|e| matches!(e, EditorEvent::LinkClicked(_)))
    );
    std::fs::remove_dir_all(dir).ok();
}

#[gpui::test]
fn dropped_files_become_attachments(cx: &mut TestAppContext) {
    let dir = std::env::temp_dir().join(format!("gme-attach-drop-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("notes.txt");
    std::fs::write(&file, b"text").unwrap();
    let (editor, cx) = open(cx, "Text\n");
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
        paths: ExternalPaths(vec![file.clone()].into()),
    });
    cx.simulate_event(FileDropEvent::Submit { position: at });
    cx.run_until_parked();
    let path = file.to_string_lossy().to_string();
    assert_eq!(md(&editor, cx), format!("Text\n\n[notes.txt]({path})\n"));
    assert_eq!(caret(&editor, cx), "notes.txt‸");
    assert!(row(&editor, cx, 1).0);
    press(cx, "secondary-z");
    assert_eq!(md(&editor, cx), "Text\n");
    std::fs::remove_dir_all(dir).ok();
}
