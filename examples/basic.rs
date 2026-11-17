//! A window with one editor and a status line.
//!
//! Run with `cargo run --example basic`.

use gpui::{
    App, AppContext as _, Bounds, Context, Entity, Focusable as _, IntoElement, ParentElement as _,
    Render, Styled as _, Window, WindowBounds, WindowOptions, div, px, size,
};
use gpui_component::{ActiveTheme as _, Root};
use gpui_markdown_editor::extensions::{Highlight, Tag, WikiLink};
use gpui_markdown_editor::{EditorEvent, EditorOptions, MarkdownEditor};

const NOTE: &str = "\
# Weekend trip

Leaving **Friday** at 8, back on Sunday evening. See [[Packing list]] and #travel.

## To do

- [x] Book the cabin
- [ ] Check the tires
- [ ] Buy groceries

| Day | Plan |
|---|---|
| Sat | Hike to the lake |
| Sun | Breakfast, drive home |

> Don't forget the ==charger==.
";

struct Example {
    editor: Entity<MarkdownEditor>,
}

impl Example {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let options = EditorOptions {
            first_line_is_title: true,
            extensions: vec![Box::new(WikiLink), Box::new(Tag), Box::new(Highlight)],
            ..Default::default()
        };
        let editor = cx.new(|cx| MarkdownEditor::new(window, cx, options));
        editor.update(cx, |editor, cx| editor.set_markdown(NOTE, cx));
        editor.focus_handle(cx).focus(window, cx);

        cx.subscribe(&editor, |_, _, event: &EditorEvent, cx| {
            if matches!(event, EditorEvent::Changed) {
                // A real app saves `editor.markdown()` here, debounced, and
                // then calls `mark_saved`.
                cx.notify();
            }
        })
        .detach();

        Self { editor }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let editor = self.editor.read(cx);
        let status = format!(
            "{} words{}",
            editor.word_count(),
            if editor.is_dirty() { " · edited" } else { "" }
        );

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(div().flex_1().min_h_0().child(self.editor.clone()))
            .child(
                div()
                    .px_3()
                    .py_1()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(status),
            )
    }
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        gpui_component::init(cx);
        gpui_markdown_editor::init(cx);

        let bounds = Bounds::centered(None, size(px(900.), px(700.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..Default::default()
        };
        cx.open_window(options, |window, cx| {
            let example = cx.new(|cx| Example::new(window, cx));
            // gpui-component draws its menus and tooltips in a `Root` view.
            cx.new(|cx| Root::new(example, window, cx))
        })
        .expect("failed to open the window");
        cx.activate(true);
    });
}
