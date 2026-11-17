//! Interaction tests: keystrokes, mouse and IME against a test window.
//!
//! The test platform shapes text with a monospace stand-in (every glyph is
//! 0.6 em wide), which is enough for geometry: clicks are aimed with the
//! view's own queries.

mod clipboard;
mod complete;
mod drag;
mod editing;
mod extension;
mod find;
mod fold;
mod format_bar;
mod image;
mod ime;
mod link;
mod mouse;
mod slash;
mod table;
mod timing;

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    Entity, Focusable as _, OperatingSystem, Pixels, Point, TestAppContext, VisualTestContext,
    point, px, size,
};

use super::MarkdownEditor;
use crate::edit::Selection;
use crate::event::EditorEvent;
use crate::extensions::{Highlight, Tag, WikiLink};
use crate::model::{BlockId, TextPos};
use crate::options::EditorOptions;

pub(super) fn options() -> EditorOptions {
    EditorOptions {
        extensions: vec![Box::new(WikiLink), Box::new(Tag), Box::new(Highlight)],
        ..Default::default()
    }
}

/// An editor with `md` in a focused 800x600 test window.
pub(super) fn open<'a>(
    cx: &'a mut TestAppContext,
    md: &str,
) -> (Entity<MarkdownEditor>, &'a mut VisualTestContext) {
    open_with(cx, md, options())
}

pub(super) fn open_with<'a>(
    cx: &'a mut TestAppContext,
    md: &str,
    options: EditorOptions,
) -> (Entity<MarkdownEditor>, &'a mut VisualTestContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        crate::init(cx);
    });
    let md = md.to_string();
    let (editor, cx) = cx.add_window_view(move |window, cx| {
        let mut editor = MarkdownEditor::new(window, cx, options);
        editor.set_markdown(&md, cx);
        editor
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.update(|window, cx| editor.focus_handle(cx).focus(window, cx));
    cx.run_until_parked();
    (editor, cx)
}

pub(super) fn md(editor: &Entity<MarkdownEditor>, cx: &mut VisualTestContext) -> String {
    editor.read_with(cx, |editor, _| editor.markdown())
}

/// The text of the caret's node with `‸` at the head and `«` at the anchor
/// (if it is in the same node).
pub(super) fn caret(editor: &Entity<MarkdownEditor>, cx: &mut VisualTestContext) -> String {
    editor.read_with(cx, |editor, _| {
        let state = &editor.state;
        match &state.selection {
            Selection::Node(_) => "<node>".to_string(),
            Selection::All => "<all>".to_string(),
            Selection::Cells { .. } => "<cells>".to_string(),
            Selection::Text { anchor, head } => {
                let mut text = state.text_of(head.block).unwrap().text.clone();
                let mut marks = vec![(head.offset, '‸')];
                if anchor.block == head.block && anchor != head {
                    marks.push((anchor.offset, '«'));
                }
                marks.sort_by_key(|m| std::cmp::Reverse(m.0));
                for (offset, mark) in marks {
                    text.insert(offset, mark);
                }
                text
            }
        }
    })
}

/// The id of the `ix`-th top-level block.
pub(super) fn block(
    editor: &Entity<MarkdownEditor>,
    cx: &mut VisualTestContext,
    ix: usize,
) -> BlockId {
    editor.read_with(cx, |editor, _| editor.document().blocks()[ix].id())
}

/// Puts the caret (or a range) somewhere.
pub(super) fn select(
    editor: &Entity<MarkdownEditor>,
    cx: &mut VisualTestContext,
    anchor: TextPos,
    head: TextPos,
) {
    editor.update(cx, |editor, cx| editor.__set_selection(anchor, head, cx));
    cx.run_until_parked();
}

/// The window point in the middle of the caret at `pos`.
pub(super) fn point_of(
    editor: &Entity<MarkdownEditor>,
    cx: &mut VisualTestContext,
    pos: TextPos,
) -> Point<Pixels> {
    editor.update_in(cx, |editor, window, _| {
        let ts = window.text_system().clone();
        let view = editor.view.clone();
        let bounds = view
            .query(editor.layout_input(), &ts)
            .caret_bounds(pos)
            .expect("caret bounds");
        point(bounds.left() + px(1.), bounds.center().y)
    })
}

pub(super) fn record_events(
    editor: &Entity<MarkdownEditor>,
    cx: &mut VisualTestContext,
) -> Rc<RefCell<Vec<EditorEvent>>> {
    let events = Rc::new(RefCell::new(Vec::new()));
    cx.update(|_, cx| {
        let events = events.clone();
        cx.subscribe(editor, move |_, event: &EditorEvent, _| {
            events.borrow_mut().push(event.clone());
        })
        .detach();
    });
    events
}

pub(super) fn count(events: &RefCell<Vec<EditorEvent>>, event: &EditorEvent) -> usize {
    events.borrow().iter().filter(|e| *e == event).count()
}

/// Presses keys like `simulate_keystrokes`, with platform-neutral names:
/// `word-` is Ctrl (Alt on macOS), `doc-start` and `doc-end` are
/// Ctrl+Home/End (Cmd+Up/Down on macOS).
pub(super) fn press(cx: &mut VisualTestContext, keys: &str) {
    let mac = cx.update(|_, cx| cx.platform_info().os) == OperatingSystem::Mac;
    let keys: Vec<String> = keys
        .split(' ')
        .map(|key| {
            let key = key
                .replace("doc-start", if mac { "cmd-up" } else { "ctrl-home" })
                .replace("doc-end", if mac { "cmd-down" } else { "ctrl-end" });
            key.replace("word-", if mac { "alt-" } else { "ctrl-" })
        })
        .collect();
    cx.simulate_keystrokes(&keys.join(" "));
}

/// Types `text`; `\n` presses Enter.
pub(super) fn type_text(cx: &mut VisualTestContext, text: &str) {
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            cx.simulate_keystrokes("enter");
        }
        if !line.is_empty() {
            cx.simulate_input(line);
        }
    }
}
