//! A note in the style of Apple Notes: the first line is the title, wiki
//! links, tags and highlights are on, and every event is printed.
//!
//! Run with `cargo run --example notes`. Ctrl+click (Cmd+click on macOS) a
//! link, wiki link or tag to see its event. Hover the table for its "+"
//! bars and row and column handles, drag a column border to resize it, click
//! the image and drag a handle to resize it, paste an image to add one.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use gpui::{
    App, AppContext as _, Bounds, Context, Entity, Focusable as _, IntoElement, ParentElement as _,
    Render, Styled as _, Window, WindowBounds, WindowOptions, div, px, size,
};
use gpui_component::{ActiveTheme as _, Root};
use gpui_markdown_editor::extensions::{Highlight, Tag, WikiLink};
use gpui_markdown_editor::{EditorEvent, EditorOptions, MarkdownEditor};

const NOTE: &str = "\
Weekend in the mountains

Leaving **Friday** at 8, back on *Sunday* evening. Route on [the map](https://example.com/map), see [[Packing list]] and #travel. Don't forget the ==charger==.

## To do

- [x] Book the cabin
- [ ] Check the tires
- [ ] Buy groceries

| Day | Plan                  | Km  |
|-----|-----------------------|----:|
| Sat | Hike to the lake      | 12  |
| Sun | Breakfast, drive home | 180 |

![Sunrise over the lake](sunrise.png)

> Type `# ` for a heading, `[] ` for a checkbox, `**bold**` for bold.
";

/// The folder images of the note live in: a generated picture, and pasted
/// images.
fn image_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("gpui-markdown-editor-notes");
    std::fs::create_dir_all(&dir).expect("image folder");
    let picture = dir.join("sunrise.png");
    if !picture.exists() {
        let (w, h) = (1200u32, 500u32);
        let img = image::RgbaImage::from_fn(w, h, |x, y| {
            let t = (x as f32 / w as f32 + y as f32 / h as f32) / 2.;
            let mix = |a: f32, b: f32| (a + (b - a) * t) as u8;
            image::Rgba([mix(255., 70.), mix(170., 120.), mix(90., 200.), 255])
        });
        img.save(&picture).expect("write the picture");
    }
    dir
}

struct Notes {
    editor: Entity<MarkdownEditor>,
}

impl Notes {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let dir = image_dir();
        let resolve_dir = dir.clone();
        let pasted = AtomicUsize::new(0);
        let options = EditorOptions {
            resolve_src: Some(Arc::new(move |src: &str| {
                Some(resolve_dir.join(src)).filter(|path| path.exists())
            })),
            on_image: Some(Arc::new(move |bytes: Vec<u8>, mime: &str| {
                let ext = mime.strip_prefix("image/").unwrap_or("png");
                let name = format!("pasted-{}.{ext}", pasted.fetch_add(1, Ordering::Relaxed));
                std::fs::write(dir.join(&name), bytes).ok()?;
                Some(name)
            })),
            first_line_is_title: true,
            extensions: vec![Box::new(WikiLink), Box::new(Tag), Box::new(Highlight)],
            ..Default::default()
        };
        let editor = cx.new(|cx| MarkdownEditor::new(window, cx, options));
        editor.update(cx, |editor, cx| editor.set_markdown(NOTE, cx));
        editor.focus_handle(cx).focus(window, cx);

        cx.subscribe(&editor, |_, editor, event: &EditorEvent, cx| {
            match event {
                EditorEvent::Changed => {
                    let editor = editor.read(cx);
                    println!(
                        "Changed: {} words, {} bytes of Markdown",
                        editor.word_count(),
                        editor.markdown().len()
                    );
                }
                EditorEvent::SelectionChanged => println!("SelectionChanged"),
                EditorEvent::MetaChanged => {
                    println!("MetaChanged: {:?}", editor.read(cx).meta());
                }
                other => println!("{other:?}"),
            }
            cx.notify();
        })
        .detach();

        Self { editor }
    }
}

impl Render for Notes {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.editor.clone())
    }
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        gpui_component::init(cx);
        gpui_markdown_editor::init(cx);

        let bounds = Bounds::centered(None, size(px(900.), px(760.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..Default::default()
        };
        cx.open_window(options, |window, cx| {
            let notes = cx.new(|cx| Notes::new(window, cx));
            cx.new(|cx| Root::new(notes, window, cx))
        })
        .expect("failed to open the window");
        cx.activate(true);
    });
}
