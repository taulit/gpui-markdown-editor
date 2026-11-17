//! A note in the style of Apple Notes: the first line is the title, wiki
//! links, tags and highlights are on, and every event is printed.
//!
//! Run with `cargo run --example notes`. Everything the editor offers is
//! reachable here (Cmd instead of Ctrl on macOS):
//!
//! - The format bar at the bottom follows the selection; "Aa" opens the
//!   styles.
//! - Type `/` at the start of a line for the slash menu, including the
//!   host entries "Meeting notes" and "Paste HTML sample".
//! - Ctrl+K edits a link; with the caret in a link a bubble offers Open,
//!   Edit and Remove. Ctrl+click a link, wiki link or tag to see its event.
//! - Ctrl+F finds (Enter, Shift+Enter, F3, Alt+C for case, Escape).
//! - Hover a heading for its chevron to fold the section (or Ctrl+.); the
//!   state is printed as `MetaChanged`.
//! - Hover a block for its drag handle on the left and drag it elsewhere.
//! - Hover the table for its "+" bars and row and column handles, drag a
//!   column border to resize it; click the image and drag a handle to
//!   resize it; paste an image or drop an image file to add one.
//! - Type `[[Note]]` or `#tag ` to get a wiki link or a tag.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use gpui::{
    App, AppContext as _, Bounds, Context, Entity, Focusable as _, IntoElement, ParentElement as _,
    Render, Styled as _, Window, WindowBounds, WindowOptions, div, px, size,
};
use gpui_component::{ActiveTheme as _, Root};
use gpui_markdown_editor::extensions::{Highlight, Tag, WikiLink};
use gpui_markdown_editor::markdown::html_to_markdown;
use gpui_markdown_editor::{Command, EditorEvent, EditorOptions, MarkdownEditor, SlashItem};

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

> Type `# ` for a heading, `[] ` for a checkbox, `**bold**` for bold, `/` for blocks.

## Packing

- Warm jacket
- Hiking boots
  - the new ones
- Snacks

## Notes

Fold the sections above with the chevron next to their heading.
";

/// HTML as it comes from a browser, for the "Paste HTML sample" entry.
const HTML: &str = "<h3>Pasted from the web</h3><p>Some <b>bold</b>, <i>italic</i> and \
<a href=\"https://example.com\">a link</a>.</p><ul><li>one</li><li>two</li></ul>";

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
            slash_items: vec![
                SlashItem::new("meeting", "Meeting notes", |editor, window, cx| {
                    let template = "## Attendees\n\n- \n\n## Decisions\n\n- [ ] \n".to_string();
                    editor.command(Command::InsertMarkdown(template), window, cx);
                })
                .description("Insert the meeting template")
                .keywords(["template"]),
                SlashItem::new("html", "Paste HTML sample", |editor, window, cx| {
                    // GPUI offers no HTML clipboard format; a host that reads
                    // HTML itself converts it like this.
                    editor.command(Command::InsertMarkdown(html_to_markdown(HTML)), window, cx);
                })
                .description("Rich text converted to Markdown"),
            ],
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
