//! The link popover and the link bubble.
//!
//! - The popover (Ctrl+K, the link button of the format bar) has a field for
//!   the URL. Enter applies it to the selection (at a caret inside a link it
//!   changes that link, elsewhere it inserts the URL as linked text), Escape
//!   cancels, "Remove" removes the link. The same popover asks for the
//!   source of an image inserted from the slash menu or the format bar.
//! - The bubble appears below a link while the caret is in it: the URL,
//!   "Open" (emits [`EditorEvent::LinkClicked`]), "Edit" and "Remove".

use gpui::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, Pixels, SharedString, StatefulInteractiveElement as _, Styled as _, Window,
    div, prelude::FluentBuilder as _, px, size,
};
use gpui_component::Sizable as _;
use gpui_component::input::{Input, InputState};

use super::super::MarkdownEditor;
use super::super::keys::{ApplyPrompt, ClosePrompt, PROMPT_CONTEXT};
use super::{Palette, appear, overlay, panel, place, tool_button};
use crate::edit::Selection;
use crate::edit::inline_ops::span_at;
use crate::event::EditorEvent;
use crate::model::{BlockId, Mark};

const WIDTH: Pixels = px(340.);
const HEIGHT: Pixels = px(42.);
const BUBBLE_HEIGHT: Pixels = px(38.);

/// What the popover asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::editor) enum PromptKind {
    Link,
    Image,
}

/// The open popover.
pub(in crate::editor) struct LinkPrompt {
    pub kind: PromptKind,
    pub input: Entity<InputState>,
    /// The selection when the popover opened, restored before applying.
    selection: Selection,
    /// The caret or selection is in a link: "Remove" is offered.
    existing: bool,
}

/// The link at the caret: its node, byte range and URL.
pub(in crate::editor) struct LinkAt {
    pub block: BlockId,
    pub range: std::ops::Range<usize>,
    pub url: SharedString,
}

fn link_kind() -> Mark {
    Mark::Link {
        url: "".into(),
        title: None,
    }
}

impl MarkdownEditor {
    /// The link around a collapsed caret.
    pub(in crate::editor) fn link_at_caret(&self) -> Option<LinkAt> {
        let head = self
            .state
            .selection
            .head()
            .filter(|_| self.state.selection.is_caret())?;
        let inline = self.state.text_of(head.block)?;
        let span = span_at(inline, head.offset, &link_kind())?;
        let Mark::Link { url, .. } = &span.mark else {
            return None;
        };
        Some(LinkAt {
            block: head.block,
            range: span.range.clone(),
            url: url.clone(),
        })
    }

    /// Ctrl+K and the link button: opens the popover for the selection or
    /// the link at the caret, prefilled with its URL or a URL from the
    /// clipboard.
    pub(in crate::editor) fn open_link_prompt(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let existing = self.state.has_mark(&link_kind());
        let url = self.link_at_caret().map(|l| l.url.to_string()).or_else(|| {
            let ordered = self.state.ordered()?;
            let inline = self.state.text_of(ordered.0.block)?;
            match &span_at(inline, ordered.0.offset, &link_kind())?.mark {
                Mark::Link { url, .. } => Some(url.to_string()),
                _ => None,
            }
        });
        let url = url.or_else(|| {
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .map(|text| text.trim().to_string())
                .filter(|text| crate::edit::is_url(text))
        });
        self.open_prompt(
            PromptKind::Link,
            url.unwrap_or_default(),
            existing,
            window,
            cx,
        );
    }

    /// Asks for the source of an image to insert below the current block.
    pub(in crate::editor) fn open_image_prompt(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_prompt(PromptKind::Image, String::new(), false, window, cx);
    }

    fn open_prompt(
        &mut self,
        kind: PromptKind,
        value: String,
        existing: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.slash = None;
        let placeholder = self.tr(match kind {
            PromptKind::Link => "link.placeholder",
            PromptKind::Image => "image.placeholder",
        });
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder(placeholder);
            state.set_value(value, window, cx);
            state
        });
        input.update(cx, |state, cx| {
            state.focus(window, cx);
            state.select_all(window, cx);
        });
        self.prompt = Some(LinkPrompt {
            kind,
            input,
            selection: self.state.selection.clone(),
            existing,
        });
        cx.notify();
    }

    /// Applies the popover's value and closes it.
    pub(in crate::editor) fn apply_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(prompt) = self.prompt.take() else {
            return;
        };
        let value = prompt.input.read(cx).value().trim().to_string();
        self.focus.focus(window, cx);
        self.state.set_selection(prompt.selection);
        match prompt.kind {
            PromptKind::Link if value.is_empty() => {
                if prompt.existing {
                    self.state.set_link(None);
                }
            }
            PromptKind::Link => {
                self.state.set_link(Some(&value));
            }
            PromptKind::Image if !value.is_empty() => {
                self.state.insert_image(&value, "");
            }
            PromptKind::Image => {}
        }
        self.after_change(false, window, cx);
    }

    /// Closes the popover without changes, restoring the selection.
    pub(in crate::editor) fn cancel_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(prompt) = self.prompt.take() else {
            return;
        };
        self.focus.focus(window, cx);
        self.state.set_selection(prompt.selection);
        self.after_change(false, window, cx);
    }

    /// Removes the link at the caret or in the selection the popover was
    /// opened for.
    fn remove_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(prompt) = self.prompt.take() {
            self.state.set_selection(prompt.selection);
            self.focus.focus(window, cx);
        }
        self.state.set_link(None);
        self.after_change(false, window, cx);
    }

    /// The popover below the selection.
    pub(in crate::editor) fn render_prompt(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let prompt = self.prompt.as_ref()?;
        let palette = Palette::new(cx);
        let anchor = self.selection_anchor(&prompt.selection, window)?;
        let position = place(anchor, size(WIDTH, HEIGHT), self.view.bounds(), false);
        let text_button = |id: &'static str, label: SharedString| {
            tool_button(id, &palette, false, label.clone())
                .h(px(30.))
                .px(px(10.))
                .text_size(px(12.5))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .role(gpui::Role::Button)
                .aria_label(label)
        };
        let (label, apply) = match prompt.kind {
            PromptKind::Link => (self.tr("link.edit"), self.tr("link.apply")),
            PromptKind::Image => (self.tr("image.insert"), self.tr("image.insert_short")),
        };
        let bar = panel(&palette)
            .id("link-popover")
            .key_context(PROMPT_CONTEXT)
            .role(gpui::Role::Dialog)
            .aria_label(label)
            .occlude()
            .w(WIDTH)
            .h(HEIGHT)
            .p(px(5.))
            .gap(px(2.))
            .items_center()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down_out(cx.listener(|this, _, window, cx| this.cancel_prompt(window, cx)))
            .on_action(
                cx.listener(|this, _: &ClosePrompt, window, cx| this.cancel_prompt(window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ApplyPrompt, window, cx| this.apply_prompt(window, cx)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .child(Input::new(&prompt.input).appearance(false).small()),
            )
            .when(prompt.existing, |s| {
                s.child(
                    text_button("link-remove", self.tr("link.remove"))
                        .on_click(cx.listener(|this, _, window, cx| this.remove_link(window, cx))),
                )
            })
            .child(
                text_button("link-apply", apply)
                    .text_color(palette.accent)
                    .on_click(cx.listener(|this, _, window, cx| this.apply_prompt(window, cx))),
            );
        Some(overlay(position, appear(bar, "link-prompt", false)))
    }

    /// The bubble below the link at the caret, while nothing else is open.
    pub(in crate::editor) fn render_link_bubble(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        if !self.options.ui.link_popover
            || !self.focused
            || self.prompt.is_some()
            || self.slash.is_some()
            || self.drag.is_some()
        {
            return None;
        }
        let link = self.link_at_caret()?;
        let palette = Palette::new(cx);
        let ts = window.text_system().clone();
        let anchor = self
            .view
            .query(self.layout_input(), &ts)
            .bounds_for_range(link.block, link.range.clone())?;
        let width = px(330.);
        let position = place(
            anchor,
            size(width, BUBBLE_HEIGHT),
            self.view.bounds(),
            false,
        );
        let text_button = |id: &'static str, label: SharedString| {
            tool_button(id, &palette, false, label.clone())
                .h(px(28.))
                .px(px(9.))
                .text_size(px(12.5))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .role(gpui::Role::Button)
                .aria_label(label)
        };
        let url = link.url.clone();
        let bubble = panel(&palette)
            .id("link-bubble")
            .role(gpui::Role::Toolbar)
            .aria_label(self.tr("link.bubble"))
            .occlude()
            .max_w(width)
            .h(BUBBLE_HEIGHT)
            .p(px(4.))
            .pl(px(10.))
            .gap(px(2.))
            .items_center()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .debug_selector(|| "link-url".into())
                    .flex_1()
                    .min_w(px(0.))
                    .max_w(px(170.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(px(12.5))
                    .text_color(palette.muted)
                    .child(link.url.clone()),
            )
            .child(text_button("link-open", self.tr("link.open")).on_click(
                cx.listener(move |_, _, _, cx| cx.emit(EditorEvent::LinkClicked(url.to_string()))),
            ))
            .child(
                text_button("link-edit", self.tr("link.edit_short"))
                    .on_click(cx.listener(|this, _, window, cx| this.open_link_prompt(window, cx))),
            )
            .child(
                text_button("link-unlink", self.tr("link.remove"))
                    .on_click(cx.listener(|this, _, window, cx| this.remove_link(window, cx))),
            );
        Some(overlay(position, appear(bubble, "link-bubble", false)))
    }

    /// The window bounds of the start of `selection`, to place a popup at.
    pub(in crate::editor) fn selection_anchor(
        &self,
        selection: &Selection,
        window: &mut Window,
    ) -> Option<gpui::Bounds<Pixels>> {
        let ts = window.text_system().clone();
        let view = self.view.clone();
        let mut q = view.query(self.layout_input(), &ts);
        match selection {
            Selection::Text { anchor, head } => {
                let (start, end) = if self.state.cmp_pos(*anchor, *head).is_le() {
                    (*anchor, *head)
                } else {
                    (*head, *anchor)
                };
                let end = if end.block == start.block {
                    end.offset
                } else {
                    start.offset
                };
                q.bounds_for_range(start.block, start.offset..end)
            }
            Selection::Node(id) => q.block_bounds(*id),
            Selection::Cells { head, .. } => q.block_bounds(*head),
            Selection::All => None,
        }
    }
}
