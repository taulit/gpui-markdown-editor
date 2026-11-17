//! The find bar (Ctrl+F, [`Command::OpenFind`](crate::Command::OpenFind)):
//! a query field with the match count, previous/next and a case toggle.
//!
//! The matches are painted like [`MarkdownEditor::highlight`] ranges, the
//! current one stronger, and the current match is scrolled into view (its
//! section is unfolded first). Enter and F3 go to the next match,
//! Shift+Enter and Shift+F3 to the previous one. Escape closes the bar,
//! selects the current match and gives the focus back to the document.

use std::cmp::Ordering;

use gpui::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, Pixels, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, Window, div, px,
};
use gpui_component::Sizable as _;
use gpui_component::input::{Input, InputEvent, InputState};

use super::super::MarkdownEditor;
use super::super::keys::{
    CloseFind, FIND_CONTEXT, FindNext, FindPrevious, OpenFind, ToggleFindCase,
};
use super::{MARGIN, Palette, appear, overlay, panel, tool_button};
use crate::edit::Selection;
use crate::model::{TextPos, TextRange};

/// Width of the bar.
const WIDTH: Pixels = px(380.);

/// The open find bar.
pub(in crate::editor) struct FindBar {
    pub input: Entity<InputState>,
    pub case_sensitive: bool,
    /// The matches of the query in document order.
    pub matches: Vec<TextRange>,
    /// Index of the current match in `matches`.
    pub current: Option<usize>,
    /// The query `matches` were found for.
    query: String,
    _subscription: Subscription,
}

impl FindBar {
    pub fn current_range(&self) -> Option<TextRange> {
        self.current.and_then(|i| self.matches.get(i)).copied()
    }

    /// "3 of 12", "No results" or nothing for an empty query.
    fn count_label(&self, query: &str) -> SharedString {
        match (self.current, self.matches.len()) {
            _ if query.is_empty() => SharedString::default(),
            (_, 0) => "No results".into(),
            (Some(i), n) => format!("{} of {n}", i + 1).into(),
            (None, n) => format!("{n} found").into(),
        }
    }
}

impl MarkdownEditor {
    /// Opens the find bar, or focuses it and selects its query if it is
    /// open. Text selected within one block becomes the query.
    pub(in crate::editor) fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.options.ui.find_bar {
            return;
        }
        let seed = match self.state.ordered() {
            Some((a, b)) if a.block == b.block && a != b => self
                .state
                .text_of(a.block)
                .and_then(|t| t.text.get(a.offset..b.offset))
                .filter(|s| !s.contains('\n'))
                .map(str::to_string),
            _ => None,
        };
        if self.find.is_none() {
            let last = self.last_query.clone();
            let input = cx.new(|cx| {
                let mut state = InputState::new(window, cx).placeholder("Find");
                state.set_value(last, window, cx);
                state
            });
            let subscription =
                cx.subscribe(&input, |this, _, event: &InputEvent, cx| match event {
                    // The field may report a change when nothing changed, which
                    // must not reset the current match.
                    InputEvent::Change if this.query_changed(cx) => this.refresh_find(true, cx),
                    _ => {}
                });
            self.find = Some(FindBar {
                input,
                case_sensitive: false,
                matches: Vec::new(),
                current: None,
                query: String::new(),
                _subscription: subscription,
            });
        }
        let Some(find) = &self.find else {
            return;
        };
        let input = find.input.clone();
        input.update(cx, |state, cx| {
            if let Some(seed) = seed {
                state.set_value(seed, window, cx);
            }
            state.focus(window, cx);
            state.select_all(window, cx);
        });
        self.refresh_find(true, cx);
    }

    /// Recomputes the matches after the query, the case setting or the
    /// document changed. `pick` chooses the first match at or after the
    /// caret, else the current index is kept.
    pub(in crate::editor) fn refresh_find(&mut self, pick: bool, cx: &mut Context<Self>) {
        let Some(find) = &self.find else {
            return;
        };
        let query = find.input.read(cx).value().to_string();
        let matches = self.find_matches(&query, find.case_sensitive);
        let caret = self.state.ordered().map(|(start, _)| start);
        let previous = find.current;
        let current = if matches.is_empty() {
            None
        } else if pick {
            Some(
                caret
                    .and_then(|c| {
                        matches
                            .iter()
                            .position(|m| self.state.cmp_pos(m.start, c) != Ordering::Less)
                    })
                    .unwrap_or(0),
            )
        } else {
            Some(previous.unwrap_or(0).min(matches.len() - 1))
        };
        if let Some(find) = &mut self.find {
            find.query = query;
            find.matches = matches;
            find.current = current;
        }
        if pick {
            self.reveal_match(cx);
        }
        cx.notify();
    }

    fn query_changed(&self, cx: &gpui::App) -> bool {
        self.find
            .as_ref()
            .is_some_and(|f| *f.input.read(cx).value() != *f.query)
    }

    /// Moves to the next (or previous) match, wrapping around.
    pub(in crate::editor) fn find_step(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(find) = &mut self.find else {
            return;
        };
        let n = find.matches.len();
        if n == 0 {
            return;
        }
        find.current = Some(match (find.current, forward) {
            (None, _) => 0,
            (Some(i), true) => (i + 1) % n,
            (Some(i), false) => (i + n - 1) % n,
        });
        self.reveal_match(cx);
        cx.notify();
    }

    fn reveal_match(&mut self, cx: &mut Context<Self>) {
        let Some(range) = self.find.as_ref().and_then(FindBar::current_range) else {
            return;
        };
        self.unfold_around(range.start.block, cx);
        self.view.request_reveal(range);
    }

    fn toggle_find_case(&mut self, cx: &mut Context<Self>) {
        if let Some(find) = &mut self.find {
            find.case_sensitive = !find.case_sensitive;
            self.refresh_find(true, cx);
        }
    }

    /// Closes the find bar and focuses the document. With `select` the
    /// current match becomes the selection.
    pub(in crate::editor) fn close_find(
        &mut self,
        select: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(find) = self.find.take() else {
            return;
        };
        self.last_query = find.input.read(cx).value().to_string();
        if select && let Some(range) = find.current_range() {
            self.state.set_selection(Selection::Text {
                anchor: range.start,
                head: range.end,
            });
        }
        self.focus.focus(window, cx);
        self.after_change(false, window, cx);
    }

    /// The matches of `query` in the text of the document.
    pub(in crate::editor) fn find_matches(
        &self,
        query: &str,
        case_sensitive: bool,
    ) -> Vec<TextRange> {
        let mut ranges = Vec::new();
        if query.is_empty() {
            return ranges;
        }
        let needle = query.to_lowercase();
        super::super::visit_texts(self.state.doc.blocks(), &mut |block, text| {
            let found = if case_sensitive {
                text.match_indices(query)
                    .map(|(start, s)| start..start + s.len())
                    .collect()
            } else {
                super::super::find_ignore_case(text, &needle)
            };
            ranges.extend(found.into_iter().map(|range| TextRange {
                start: TextPos {
                    block,
                    offset: range.start,
                },
                end: TextPos {
                    block,
                    offset: range.end,
                },
            }));
        });
        ranges
    }

    /// The bar, at the top right of the editor.
    pub(in crate::editor) fn render_find(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        let find = self.find.as_ref()?;
        let palette = Palette::new(cx);
        let query = find.input.read(cx).value();
        let label = find.count_label(&query);
        let area = self.view.bounds();
        let x = (area.size.width - WIDTH - px(16.)).max(MARGIN);
        let button = |id: &'static str, text: &'static str, active: bool| {
            tool_button(id, &palette, active, text)
                .w(px(30.))
                .h(px(28.))
                .text_size(px(13.))
                .aria_label(match id {
                    "find-case" => "Match case",
                    "find-previous" => "Previous match",
                    "find-next" => "Next match",
                    _ => "Close",
                })
        };
        let bar = panel(&palette)
            .id("find-bar")
            .key_context(FIND_CONTEXT)
            .role(gpui::Role::Search)
            .aria_label("Find in document")
            .occlude()
            .w(WIDTH)
            .p(px(5.))
            .gap(px(2.))
            .items_center()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_action(
                cx.listener(|this, _: &CloseFind, window, cx| this.close_find(true, window, cx)),
            )
            .on_action(cx.listener(|this, _: &OpenFind, window, cx| this.open_find(window, cx)))
            .on_action(cx.listener(|this, _: &FindNext, _, cx| this.find_step(true, cx)))
            .on_action(cx.listener(|this, _: &FindPrevious, _, cx| this.find_step(false, cx)))
            .on_action(cx.listener(|this, _: &ToggleFindCase, _, cx| this.toggle_find_case(cx)))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .child(Input::new(&find.input).appearance(false).small()),
            )
            .child(
                div()
                    .debug_selector(|| "find-count".into())
                    .flex_none()
                    .px(px(6.))
                    .text_size(px(12.))
                    .text_color(palette.muted)
                    .child(label),
            )
            .child(
                button("find-case", "Aa", find.case_sensitive)
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_find_case(cx))),
            )
            .child(
                button("find-previous", "\u{2191}", false)
                    .on_click(cx.listener(|this, _, _, cx| this.find_step(false, cx))),
            )
            .child(
                button("find-next", "\u{2193}", false)
                    .on_click(cx.listener(|this, _, _, cx| this.find_step(true, cx))),
            )
            .child(
                button("find-close", "\u{00d7}", false)
                    .text_size(px(17.))
                    .on_click(
                        cx.listener(|this, _, window, cx| this.close_find(false, window, cx)),
                    ),
            );
        Some(overlay(
            gpui::point(x, px(10.)),
            appear(bar, "find-bar", false),
        ))
    }
}
