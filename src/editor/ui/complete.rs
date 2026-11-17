//! Completions for wiki links (`[[`) and tags (`#`) from
//! [`EditorOptions::completions`](crate::EditorOptions::completions).
//!
//! Opens when `[[` is typed (with the [`WikiLink`] extension) or `#` at the
//! start of a word (with [`Tag`]), asks the host for suggestions on every
//! keystroke and works like the slash menu: Up, Down, Enter or Tab, Escape,
//! click. Choosing replaces the typed `[[query` or `#query` with the
//! linked note or the tag.

use gpui::{
    Context, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _, Pixels,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    prelude::FluentBuilder as _, px, size,
};

use super::super::MarkdownEditor;
use super::{Palette, overlay, panel, place};
use crate::edit::Selection;
use crate::edit::history::EditKind;
use crate::extensions::{Tag, WikiLink};
use crate::model::{BlockId, Inline, Mark, MarkSpan, TextPos};
use crate::options::{Completion, CompletionKind};

const WIDTH: Pixels = px(260.);
const VISIBLE: usize = 8;
const ROW_HEIGHT: Pixels = px(34.);

/// The open list.
pub(in crate::editor) struct CompletionMenu {
    pub kind: CompletionKind,
    block: BlockId,
    /// Offset of `[[` or `#`.
    start: usize,
    pub items: Vec<Completion>,
    pub selected: usize,
}

impl CompletionMenu {
    fn trigger_len(&self) -> usize {
        match self.kind {
            CompletionKind::WikiLink => 2,
            _ => 1,
        }
    }
}

impl MarkdownEditor {
    fn has_extension(&self, name: &str) -> bool {
        self.state.doc.extensions.iter().any(|e| e.name() == name)
    }

    /// Opens the list if `typed` completed a trigger at the caret.
    pub(in crate::editor) fn maybe_open_completion(&mut self, typed: &str) {
        if self.options.completions.is_none() || self.state.marked.is_some() {
            return;
        }
        let Selection::Text { anchor, head } = self.state.selection.clone() else {
            return;
        };
        if anchor != head {
            return;
        }
        let Some(text) = self.state.text_of(head.block).map(|t| t.text.as_str()) else {
            return;
        };
        let Some(before) = text.get(..head.offset) else {
            return;
        };
        let (kind, start) =
            if typed.ends_with('[') && before.ends_with("[[") && self.has_extension("wikilink") {
                (CompletionKind::WikiLink, head.offset - 2)
            } else if typed == "#"
                && self.has_extension("tag")
                && before[..before.len() - 1]
                    .chars()
                    .next_back()
                    .is_none_or(char::is_whitespace)
            {
                (CompletionKind::Tag, head.offset - 1)
            } else {
                return;
            };
        self.completion = Some(CompletionMenu {
            kind,
            block: head.block,
            start,
            items: Vec::new(),
            selected: 0,
        });
        self.refresh_completion();
    }

    /// Asks the host again after an edit or caret move, or closes the list.
    pub(in crate::editor) fn refresh_completion(&mut self) {
        let Some(menu) = &self.completion else {
            return;
        };
        let query = (|| {
            let Selection::Text { anchor, head } = &self.state.selection else {
                return None;
            };
            let from = menu.start + menu.trigger_len();
            if anchor != head || head.block != menu.block || head.offset < from {
                return None;
            }
            let text = &self.state.text_of(menu.block)?.text;
            let trigger = text.get(menu.start..from)?;
            let ok = match menu.kind {
                CompletionKind::WikiLink => trigger == "[[",
                _ => trigger == "#",
            };
            let query = text.get(from..head.offset)?;
            let stop = match menu.kind {
                CompletionKind::WikiLink => query.contains([']', '\n', '|']),
                _ => query.contains(char::is_whitespace),
            };
            (ok && !stop).then(|| query.to_string())
        })();
        let (Some(query), Some(provider)) = (query, self.options.completions.clone()) else {
            self.completion = None;
            return;
        };
        let kind = menu.kind;
        let items = provider(kind, &query);
        if let Some(menu) = &mut self.completion {
            menu.items = items;
            menu.selected = menu.selected.min(menu.items.len().saturating_sub(1));
        }
    }

    pub(in crate::editor) fn completion_move(&mut self, delta: isize) {
        if let Some(menu) = &mut self.completion {
            let n = menu.items.len() as isize;
            if n > 0 {
                menu.selected = (menu.selected as isize + delta).rem_euclid(n) as usize;
            }
        }
    }

    /// Replaces the typed trigger and query with suggestion `index`.
    pub(in crate::editor) fn completion_choose(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(menu) = self.completion.take() else {
            return;
        };
        let (Some(item), Some(head)) = (menu.items.get(index), self.state.selection.head()) else {
            return;
        };
        let (text, id) = match menu.kind {
            CompletionKind::WikiLink => (item.insert.to_string(), WikiLink::MARK),
            _ => (
                format!("#{}", item.insert.trim_start_matches('#')),
                Tag::MARK,
            ),
        };
        if text.is_empty() || text.contains('\n') {
            return;
        }
        let mark = Mark::Custom { id, attr: None };
        let node = menu.block;
        let range = menu.start..head.offset;
        let len = text.len();
        let replacement = Inline {
            marks: vec![MarkSpan {
                range: 0..len,
                mark: mark.clone(),
            }],
            text,
        };
        let start = menu.start;
        let applied = self
            .state
            .transact(EditKind::Other, None, "", move |tr, _| {
                tr.replace(node, range, replacement)?;
                Ok(Selection::caret(TextPos {
                    block: node,
                    offset: start + len,
                }))
            });
        if applied {
            // Typing on continues outside the link or tag.
            let mut marks = self.state.caret_marks();
            marks.retain(|m| !crate::edit::inline_ops::same_kind(m, &mark));
            self.state.pending_marks = Some(marks);
        }
        self.after_change(false, window, cx);
    }

    /// The list below (or above) the trigger.
    pub(in crate::editor) fn render_completion(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let menu = self.completion.as_ref()?;
        if menu.items.is_empty() {
            return None;
        }
        let palette = Palette::new(cx);
        let ts = window.text_system().clone();
        let anchor = self
            .view
            .query(self.layout_input(), &ts)
            .bounds_for_range(menu.block, menu.start..menu.start + menu.trigger_len())?;
        let first = menu.selected.saturating_sub(VISIBLE - 1);
        let shown = menu.items.iter().enumerate().skip(first).take(VISIBLE);
        let height = ROW_HEIGHT * shown.len() as f32 + px(12.);
        let position = place(anchor, size(WIDTH, height), self.view.bounds(), false);
        let count = menu.items.len();
        let rows = shown.map(|(index, item)| {
            let selected = index == menu.selected;
            let hover = palette.hover;
            div()
                .id(SharedString::from(format!("completion-{index}")))
                .debug_selector(move || format!("completion-{index}"))
                .role(gpui::Role::ListBoxOption)
                .aria_label(item.label.clone())
                .aria_selected(selected)
                .aria_position_in_set(index + 1)
                .aria_size_of_set(count)
                .flex()
                .items_center()
                .gap(px(8.))
                .h(ROW_HEIGHT)
                .px(px(12.))
                .rounded(px(9.))
                .cursor_pointer()
                .when(selected, |s| s.bg(palette.selected()))
                .when(!selected, |s| s.hover(move |s| s.bg(hover)))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.completion_choose(index, window, cx);
                    }),
                )
                .child(
                    div()
                        .flex_1()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(13.5))
                        .child(item.label.clone()),
                )
                .children(item.detail.clone().map(|d| {
                    div()
                        .flex_none()
                        .text_size(px(11.5))
                        .text_color(palette.muted)
                        .child(d)
                }))
        });
        let list = panel(&palette)
            .id("completion-menu")
            .role(gpui::Role::ListBox)
            .aria_label(match menu.kind {
                CompletionKind::WikiLink => "Notes",
                _ => "Tags",
            })
            .occlude()
            .flex_col()
            .w(WIDTH)
            .p(px(6.))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .children(rows);
        Some(overlay(position, list))
    }
}
