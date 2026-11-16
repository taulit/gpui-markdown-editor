//! Renders the editor offscreen and writes PNGs to `docs/screenshots/`.
//!
//! No window is opened: GPUI draws into a texture with its headless renderer.
//! The tests are ignored by default because they need a GPU (or a software
//! Vulkan driver) and overwrite files in the repository. Run them with
//! `scripts/screenshot.sh`.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use gpui::{
    AppContext as _, Context, Entity, HeadlessAppContext, Hsla, IntoElement, ParentElement as _,
    PlatformTextSystem, Render, Styled as _, Window, div, px, rgb, size,
};
use gpui_component::{ActiveTheme as _, Root, Theme, ThemeMode};
use gpui_markdown_editor::extensions::{Highlight, Tag, WikiLink};
use gpui_markdown_editor::{EditorOptions, EditorStyle, MarkdownEditor};

const NOTE: &str = "\
# Weekend trip

Leaving **Friday** at 8, back on Sunday evening. See [[Packing list]] and #travel.

## To do

- [x] Book the cabin
- [ ] Check the tires
- [ ] Buy groceries

| Day | Plan                  |
|-----|-----------------------|
| Sat | Hike to the lake      |
| Sun | Breakfast, drive home |

> Don't forget the ==charger==.
";

#[test]
#[ignore = "needs a GPU and writes docs/screenshots, run scripts/screenshot.sh"]
fn e1_basic() {
    capture("e1-basic-light", ThemeMode::Light, NOTE);
    capture("e1-basic-dark", ThemeMode::Dark, NOTE);
}

/// Renders `markdown` in a 900x700 window with the Sheaf look and writes it
/// to `docs/screenshots/<name>.png`. The image has twice that size because
/// the offscreen window has a scale factor of 2.
fn capture(name: &str, mode: ThemeMode, markdown: &str) {
    // Every platform instance starts its own worker threads, so the text
    // system is taken from one platform and shared by all screenshots.
    static TEXT_SYSTEM: OnceLock<Arc<dyn PlatformTextSystem>> = OnceLock::new();
    let text_system =
        TEXT_SYSTEM.get_or_init(|| gpui_platform::current_platform(true).text_system());

    let mut cx = HeadlessAppContext::with_platform(
        text_system.clone(),
        Arc::new(()),
        gpui_platform::current_headless_renderer,
    );
    cx.update(|cx| {
        gpui_component::init(cx);
        apply_sheaf_theme(mode, cx);
    });

    let window = cx
        .open_window(size(px(900.), px(700.)), |window, cx| {
            let editor = cx.new(|cx| {
                let options = EditorOptions {
                    first_line_is_title: true,
                    extensions: vec![Box::new(WikiLink), Box::new(Tag), Box::new(Highlight)],
                    ..Default::default()
                };
                let mut editor = MarkdownEditor::new(window, cx, options);
                editor.set_style(EditorStyle::from_theme(cx), cx);
                editor.set_markdown(markdown, cx);
                editor
            });
            let page = cx.new(|_| Page { editor });
            cx.new(|cx| Root::new(page, window, cx))
        })
        .expect("failed to open the offscreen window");
    cx.run_until_parked();
    cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
        .expect("failed to draw");

    let image = cx
        .capture_screenshot(window.into())
        .expect("offscreen rendering failed");
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("docs/screenshots")
        .join(format!("{name}.png"));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    image.save(&path).unwrap();
    println!("wrote {}", path.display());
}

/// The editor on the "view" surface of Sheaf.
struct Page {
    editor: Entity<MarkdownEditor>,
}

impl Render for Page {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(cx.theme().background)
            .child(self.editor.clone())
    }
}

/// Colors and font of the Sheaf prototype, see `docs/design/plan/04-design.md`.
fn apply_sheaf_theme(mode: ThemeMode, cx: &mut gpui::App) {
    Theme::change(mode, None, cx);

    let (view, foreground, tint): (Hsla, Hsla, Hsla) = if mode.is_dark() {
        (rgb(0x1d1d20).into(), rgb(0xf5f5f7).into(), gpui::white())
    } else {
        (rgb(0xffffff).into(), rgb(0x1d1d1f).into(), gpui::black())
    };
    let accent: Hsla = rgb(0x3584e4).into();
    let font = ["Adwaita Sans", "Inter", "Cantarell"]
        .into_iter()
        .find(|family| {
            cx.text_system()
                .all_font_names()
                .iter()
                .any(|f| f == family)
        });

    Theme::update(cx, |theme| {
        theme.background = view;
        theme.foreground = foreground;
        theme.muted_foreground = foreground.opacity(0.62);
        theme.muted = tint.opacity(if mode.is_dark() { 0.07 } else { 0.05 });
        theme.border = tint.opacity(if mode.is_dark() { 0.08 } else { 0.09 });
        theme.primary = accent;
        theme.primary_foreground = gpui::white();
        theme.link = accent;
        theme.caret = accent;
        theme.selection = accent.opacity(0.4);
        if let Some(font) = font {
            theme.font_family = font.into();
        }
    });
}
