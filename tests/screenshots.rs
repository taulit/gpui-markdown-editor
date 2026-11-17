//! Renders the editor offscreen and writes PNGs to `docs/screenshots/`.
//!
//! No window is opened: GPUI draws into a texture with its headless renderer.
//! The tests are ignored by default because they need a GPU (or a software
//! Vulkan driver) and overwrite files in the repository. Run them with
//! `scripts/screenshot.sh`.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use gpui::{
    AppContext as _, Context, Entity, Focusable as _, HeadlessAppContext, Hsla, IntoElement,
    Keystroke, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ParentElement as _, PlatformInput, PlatformTextSystem, Render, Styled as _, Window, div, point,
    px, rgb, size,
};
use gpui_component::{ActiveTheme as _, Root, Theme, ThemeMode};
use gpui_markdown_editor::extensions::{Highlight, Tag, WikiLink};
use gpui_markdown_editor::{EditorOptions, EditorStyle, MarkdownEditor, TextPos, TextRange};

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

/// A Sheaf-like note with every kind of block and mark.
const E2_NOTE: &str = "\
Weekend in the mountains

Leaving **Friday** at 8, back on *Sunday* evening. The <u>cabin</u> is booked, ~~the hotel~~ cancelled. Door code `4711`, and don't forget the ==charger==. Route on [the map](https://example.com/map), see [[Packing list]] and #travel.

## Packing

- Warm jacket
- Hiking boots
  - the new ones
  - spare laces
- Snacks for **two days**

1. Pack the car
2. Pick up Anna
3. Drive up

## To do

- [x] Book the cabin
- [ ] Check the tires
- [x] Buy groceries
- [ ] Charge the camera

> The best view is from the north ridge, about an hour from the cabin.

```sh
ssh cabin.local
uptime
```

---

| Day | Plan                  | Km  |
|-----|-----------------------|----:|
| Sat | Hike to the lake      | 12  |
| Sun | Breakfast, drive home | 180 |

![Lake at sunrise](lake.jpg)
";

#[test]
#[ignore = "needs a GPU and writes docs/screenshots, run scripts/screenshot.sh"]
fn e1_basic() {
    capture("e1-basic-light", ThemeMode::Light, NOTE, |_, _| {});
    capture("e1-basic-dark", ThemeMode::Dark, NOTE, |_, _| {});
}

#[test]
#[ignore = "needs a GPU and writes docs/screenshots, run scripts/screenshot.sh"]
fn e2_note() {
    for (name, mode) in [("light", ThemeMode::Light), ("dark", ThemeMode::Dark)] {
        capture(&format!("e2-note-{name}"), mode, E2_NOTE, |_, _| {});
        capture(
            &format!("e2-note-end-{name}"),
            mode,
            E2_NOTE,
            |editor, cx| {
                let last = editor.document().blocks().last().unwrap().id();
                let pos = TextPos {
                    block: last,
                    offset: 0,
                };
                editor.scroll_to(
                    TextRange {
                        start: pos,
                        end: pos,
                    },
                    cx,
                );
            },
        );
    }
}

#[test]
#[ignore = "needs a GPU and writes docs/screenshots, run scripts/screenshot.sh"]
fn e2_selection() {
    let select = |editor: &mut MarkdownEditor, cx: &mut Context<MarkdownEditor>| {
        let blocks = editor.document().blocks();
        // From the middle of the first paragraph into the first list item.
        let anchor = TextPos {
            block: blocks[1].id(),
            offset: 60,
        };
        let head = TextPos {
            block: blocks[3].id(),
            offset: 4,
        };
        editor.__set_selection(anchor, head, cx);
    };
    let caret = |editor: &mut MarkdownEditor, cx: &mut Context<MarkdownEditor>| {
        let block = editor.document().blocks()[1].id();
        let pos = TextPos { block, offset: 21 };
        editor.__set_selection(pos, pos, cx);
    };
    capture("e2-selection-light", ThemeMode::Light, E2_NOTE, select);
    capture("e2-selection-dark", ThemeMode::Dark, E2_NOTE, select);
    capture("e2-caret-light", ThemeMode::Light, E2_NOTE, caret);
    capture("e2-caret-dark", ThemeMode::Dark, E2_NOTE, caret);

    // Through the mouse: a click in the title, then a drag from the second
    // paragraph line into the "Packing" list, and a double click on a word
    // in the checklist.
    let at = |x: f32, y: f32| point(px(x), px(y));
    let down = |p, click_count| {
        PlatformInput::MouseDown(MouseDownEvent {
            button: MouseButton::Left,
            position: p,
            modifiers: Modifiers::default(),
            click_count,
            first_mouse: false,
        })
    };
    let up = |p, click_count| {
        PlatformInput::MouseUp(MouseUpEvent {
            button: MouseButton::Left,
            position: p,
            modifiers: Modifiers::default(),
            click_count,
        })
    };
    let drag = |p| {
        PlatformInput::MouseMove(MouseMoveEvent {
            position: p,
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::default(),
        })
    };
    let events = [
        down(at(300., 100.), 1),
        drag(at(400., 200.)),
        drag(at(230., 225.)),
        up(at(230., 225.), 1),
    ];
    capture_with_input(
        "e2-mouse-drag-light",
        ThemeMode::Light,
        E2_NOTE,
        |_, _| {},
        &events,
    );
    let events = [
        down(at(240., 482.), 1),
        up(at(240., 482.), 1),
        down(at(240., 482.), 2),
        up(at(240., 482.), 2),
    ];
    capture_with_input(
        "e2-mouse-word-dark",
        ThemeMode::Dark,
        E2_NOTE,
        |_, _| {},
        &events,
    );
}

#[test]
#[ignore = "needs a GPU and writes docs/screenshots, run scripts/screenshot.sh"]
fn e2_long() {
    let markdown = long_document(10_000);
    let scroll = |editor: &mut MarkdownEditor, cx: &mut Context<MarkdownEditor>| {
        let block = editor.document().blocks()[5_000].id();
        let pos = TextPos { block, offset: 0 };
        editor.__set_selection(pos, pos, cx);
    };
    capture("e2-long-light", ThemeMode::Light, &markdown, scroll);
    capture("e2-long-dark", ThemeMode::Dark, &markdown, scroll);
}

#[test]
#[ignore = "needs a GPU and writes docs/screenshots, run scripts/screenshot.sh"]
fn e2_typing() {
    // Typed into an empty note: the input rules make a heading, a checklist
    // and bold text.
    let typed = "Shopping\n## Market\n[] Apples\nBread\nCheese\n\nOnly **organic** milk, and ~~no~~ *sugar*.";
    let keys: Vec<String> = typed
        .chars()
        .map(|ch| match ch {
            '\n' => "enter".to_string(),
            ' ' => "space".to_string(),
            ch => ch.to_string(),
        })
        .collect();
    for (name, mode) in [("light", ThemeMode::Light), ("dark", ThemeMode::Dark)] {
        capture_with_keys(
            &format!("e2-typing-{name}"),
            mode,
            "",
            |_, _| {},
            &[],
            &keys,
            None,
        );
    }
}

const E3_TABLE: &str = "\
Trip budget

| Item        | Who  | Cost (€) |
|:------------|:----:|---------:|
| Cabin       | Anna |      240 |
| Fuel        | Ben  |       85 |
| Groceries   | Both |       62 |

| Day | Plan                          | Km  |
|-----|-------------------------------|----:|
| Sat | Hike to the lake, swim, picnic | 12  |
| Sun | Breakfast, drive home         | 180 |
";

fn e3_table_setup(rect: bool) -> impl Fn(&mut MarkdownEditor, &mut Context<MarkdownEditor>) {
    move |editor, cx| {
        let tables: Vec<_> = editor
            .document()
            .blocks()
            .iter()
            .filter_map(|b| match b.kind() {
                gpui_markdown_editor::BlockKind::Table(t) => Some(t.clone()),
                _ => None,
            })
            .collect();
        // The first column of the second table is dragged wider.
        let mut meta = gpui_markdown_editor::DocMeta::default();
        meta.column_widths.insert(1, vec![110., 240., 70.]);
        editor.set_meta(meta, cx);
        if rect {
            let (a, b) = (tables[0].rows[1][1].id(), tables[0].rows[2][2].id());
            editor.__select_cells_or_node(a, Some(b), cx);
        } else {
            let cell = tables[1].rows[1][1].id();
            let pos = TextPos {
                block: cell,
                offset: 4,
            };
            editor.__set_selection(pos, pos, cx);
        }
    }
}

#[test]
#[ignore = "needs a GPU and writes docs/screenshots, run scripts/screenshot.sh"]
fn e3_table() {
    for (name, mode) in [("light", ThemeMode::Light), ("dark", ThemeMode::Dark)] {
        capture(
            &format!("e3-table-{name}"),
            mode,
            E3_TABLE,
            e3_table_setup(true),
        );
        capture(
            &format!("e3-table-caret-{name}"),
            mode,
            E3_TABLE,
            e3_table_setup(false),
        );
    }
}

const E3_IMAGE: &str = "\
Lake trip

![Lake at sunrise](lake.png)

The selected one, made narrower:

![Ridge|220](ridge.png)

![Missing photo](gone.png)
";

#[test]
#[ignore = "needs a GPU and writes docs/screenshots, run scripts/screenshot.sh"]
fn e3_image() {
    let dir = std::env::temp_dir().join(format!("gme-e3-image-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    write_gradient(
        &dir.join("lake.png"),
        1200,
        360,
        [255, 170, 90],
        [70, 120, 200],
    );
    write_gradient(
        &dir.join("ridge.png"),
        900,
        600,
        [60, 140, 90],
        [230, 230, 210],
    );
    let select = |editor: &mut MarkdownEditor, cx: &mut Context<MarkdownEditor>| {
        let ridge = editor.document().blocks()[3].id();
        editor.__select_cells_or_node(ridge, None, cx);
    };
    for (name, mode) in [("light", ThemeMode::Light), ("dark", ThemeMode::Dark)] {
        capture_with_keys(
            &format!("e3-image-{name}"),
            mode,
            E3_IMAGE,
            select,
            &[],
            &[],
            Some(&dir),
        );
    }
    std::fs::remove_dir_all(dir).ok();
}

/// Writes a PNG with a diagonal gradient between two colors.
/// Keystrokes that type `text` (`\n` is Enter).
fn keys_for(text: &str) -> Vec<String> {
    text.chars()
        .map(|ch| match ch {
            '\n' => "enter".to_string(),
            ' ' => "space".to_string(),
            ch => ch.to_string(),
        })
        .collect()
}

/// Keystrokes from a space separated list, as in `simulate_keystrokes`.
fn keys(list: &str) -> Vec<String> {
    list.split(' ').map(str::to_string).collect()
}

#[test]
#[ignore = "needs a GPU and writes docs/screenshots, run scripts/screenshot.sh"]
fn e3_find() {
    for (name, mode) in [("light", ThemeMode::Light), ("dark", ThemeMode::Dark)] {
        let mut typed = keys("ctrl-f");
        typed.extend(keys_for("the"));
        typed.extend(keys("enter"));
        capture_with_keys(
            &format!("e3-find-{name}"),
            mode,
            E2_NOTE,
            |_, _| {},
            &[],
            &typed,
            None,
        );
    }
}

const E3_SHORT: &str = "\
Saturday

Market at 9, then the bike shop. Remember the **receipt** for the wheel.

";

#[test]
#[ignore = "needs a GPU and writes docs/screenshots, run scripts/screenshot.sh"]
fn e3_slash() {
    for (name, mode) in [("light", ThemeMode::Light), ("dark", ThemeMode::Dark)] {
        let mut typed = keys("ctrl-end enter");
        typed.extend(keys_for("/"));
        capture_with_keys(
            &format!("e3-slash-{name}"),
            mode,
            E3_SHORT,
            |_, _| {},
            &[],
            &typed,
            None,
        );
    }
}

const E3_LINK: &str = "\
Saturday

Market at 9, then the [bike shop](https://bikes.example/opening-hours). Remember the receipt for the wheel.

Lunch at the lake if the weather holds.
";

#[test]
#[ignore = "needs a GPU and writes docs/screenshots, run scripts/screenshot.sh"]
fn e3_link() {
    // "receipt" selected, Ctrl+K and a URL typed.
    let select = |editor: &mut MarkdownEditor, cx: &mut Context<MarkdownEditor>| {
        let block = editor.document().blocks()[1].id();
        let text = editor.document().blocks()[1].text().to_string();
        let start = text.find("receipt").unwrap();
        let anchor = TextPos {
            block,
            offset: start,
        };
        let head = TextPos {
            block,
            offset: start + "receipt".len(),
        };
        editor.__set_selection(anchor, head, cx);
    };
    // The caret in the link shows the bubble.
    let caret = |editor: &mut MarkdownEditor, cx: &mut Context<MarkdownEditor>| {
        let block = editor.document().blocks()[1].id();
        let pos = TextPos { block, offset: 26 };
        editor.__set_selection(pos, pos, cx);
    };
    for (name, mode) in [("light", ThemeMode::Light), ("dark", ThemeMode::Dark)] {
        let mut typed = keys("ctrl-k");
        typed.extend(keys_for("https://shop.example/receipts"));
        capture_with_keys(
            &format!("e3-link-{name}"),
            mode,
            E3_LINK,
            select,
            &[],
            &typed,
            None,
        );
        capture(&format!("e3-link-bubble-{name}"), mode, E3_LINK, caret);
    }
}

fn write_gradient(path: &Path, width: u32, height: u32, from: [u8; 3], to: [u8; 3]) {
    let img = image::RgbaImage::from_fn(width, height, |x, y| {
        let t = (x as f32 / width as f32 + y as f32 / height as f32) / 2.;
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t) as u8;
        image::Rgba([
            mix(from[0], to[0]),
            mix(from[1], to[1]),
            mix(from[2], to[2]),
            255,
        ])
    });
    img.save(path).unwrap();
}

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

/// Renders `markdown` in a 900x700 window with the Sheaf look and writes it
/// to `docs/screenshots/<name>.png`. The image has twice that size because
/// the offscreen window has a scale factor of 2.
fn capture(
    name: &str,
    mode: ThemeMode,
    markdown: &str,
    setup: impl Fn(&mut MarkdownEditor, &mut Context<MarkdownEditor>) + 'static,
) {
    capture_with_input(name, mode, markdown, setup, &[]);
}

/// Like [`capture`], dispatching `events` after the first frame.
fn capture_with_input(
    name: &str,
    mode: ThemeMode,
    markdown: &str,
    setup: impl Fn(&mut MarkdownEditor, &mut Context<MarkdownEditor>) + 'static,
    events: &[PlatformInput],
) {
    capture_with_keys(name, mode, markdown, setup, events, &[], None);
}

/// Like [`capture_with_input`], typing `keys` (keystrokes such as `a`,
/// `space`, `enter`) after the events.
fn capture_with_keys(
    name: &str,
    mode: ThemeMode,
    markdown: &str,
    setup: impl Fn(&mut MarkdownEditor, &mut Context<MarkdownEditor>) + 'static,
    events: &[PlatformInput],
    keys: &[String],
    image_dir: Option<&Path>,
) {
    let image_dir = image_dir.map(Path::to_path_buf);
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
        gpui_markdown_editor::init(cx);
        apply_sheaf_theme(mode, cx);
    });

    let window = cx
        .open_window(size(px(900.), px(700.)), |window, cx| {
            let editor = cx.new(|cx| {
                let resolve_src = image_dir.clone().map(|dir| {
                    let resolver: gpui_markdown_editor::SrcResolver =
                        Arc::new(move |src: &str| Some(dir.join(src)).filter(|p| p.exists()));
                    resolver
                });
                let options = EditorOptions {
                    resolve_src,
                    first_line_is_title: true,
                    extensions: vec![Box::new(WikiLink), Box::new(Tag), Box::new(Highlight)],
                    ..Default::default()
                };
                let mut editor = MarkdownEditor::new(window, cx, options);
                editor.set_style(EditorStyle::from_theme(cx), cx);
                editor.set_markdown(markdown, cx);
                setup(&mut editor, cx);
                editor
            });
            // The caret is only drawn in a focused editor.
            editor.focus_handle(cx).focus(window, cx);
            let page = cx.new(|_| Page { editor });
            cx.new(|cx| Root::new(page, window, cx))
        })
        .expect("failed to open the offscreen window");
    // Images decode on background threads: draw until they are in.
    for _ in 0..8 {
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .expect("failed to draw");
        if image_dir.is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(40));
    }
    if !events.is_empty() || !keys.is_empty() {
        cx.update_window(window.into(), |_, window, cx| {
            for event in events {
                window.dispatch_event(event.clone(), cx);
            }
            let os = cx.platform_info().os;
            for key in keys {
                let keystroke = Keystroke::parse_with_os(key, os).expect("keystroke");
                window.dispatch_keystroke(keystroke, cx);
            }
        })
        .expect("failed to dispatch input");
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .expect("failed to draw");
    }

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
    // Adwaita Sans and Inter come as variable fonts only, and gpui-pre 0.3
    // loads variable faces at their default weight on Linux, which would
    // hide bold text. Noto Sans has static weights.
    let font = ["Noto Sans", "Adwaita Sans", "Inter", "Cantarell"]
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
        // Popovers and menus: `pop` of the prototype.
        theme.popover = if mode.is_dark() {
            rgb(0x38383d).into()
        } else {
            rgb(0xffffff).into()
        };
        theme.popover_foreground = foreground;
        if let Some(font) = font {
            theme.font_family = font.into();
        }
    });
}
