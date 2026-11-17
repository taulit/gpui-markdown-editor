//! A note in the style of Apple Notes: the first line is the title, wiki
//! links, tags and highlights are on, and every event is printed.
//!
//! Run with `cargo run --example notes`. Ctrl+click (Cmd+click on macOS) a
//! link, wiki link or tag to see its event.

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

> Type `# ` for a heading, `[] ` for a checkbox, `**bold**` for bold.
";

struct Notes {
    editor: Entity<MarkdownEditor>,
}

impl Notes {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let options = EditorOptions {
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
