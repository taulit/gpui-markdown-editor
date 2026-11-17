//! Mouse: placing the caret, selecting, checkboxes and link clicks.

use std::cmp::Ordering;
use std::time::Duration;

use gpui::{
    Context, FocusHandle, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Task, Window,
};

use super::MarkdownEditor;
use crate::edit::Selection;
use crate::event::EditorEvent;
use crate::extensions::{Tag, WikiLink};
use crate::model::{BlockId, BlockKind, Mark, MarkSpan, TextRange};
use crate::view::{ViewInput, ViewSource};

/// How often the view scrolls while the mouse is dragged outside of it.
const AUTOSCROLL_INTERVAL: Duration = Duration::from_millis(40);

/// What a drag selects by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Granularity {
    Char,
    Word,
    Block,
}

/// A selection being made with the mouse.
pub(super) struct Drag {
    /// What the first click selected; the selection always contains it.
    origin: TextRange,
    granularity: Granularity,
    /// The last mouse position, for scrolling at the edges.
    position: Point<Pixels>,
    autoscroll: Option<Task<()>>,
}

impl ViewSource for MarkdownEditor {
    fn view_input(&self) -> ViewInput<'_> {
        ViewInput {
            layout: self.layout_input(),
            selection: Some(&self.state.selection),
            highlights: &self.highlights,
            caret_visible: self.focused && self.caret_on,
            marked: self.state.valid_marked(),
        }
    }

    fn input_focus(&self) -> &FocusHandle {
        &self.focus
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus.focus(window, cx);
        self.drag = None;
        let p = event.position;
        let ts = window.text_system().clone();
        let view = self.view.clone();
        let mut q = view.query(self.layout_input(), &ts);

        if event.modifiers.secondary(cx)
            && let Some((block, span)) = q.link_at(p)
        {
            drop(q);
            self.emit_link(block, &span, cx);
            return;
        }
        if let Some(item) = q.checkbox_at(p) {
            drop(q);
            self.state.toggle_task_item(item);
            self.after_change(false, window, cx);
            return;
        }
        // Rules and images are selected as a whole, table cells hold text.
        let node = q.node_at(p).filter(|id| {
            self.state
                .doc
                .block(*id)
                .is_some_and(|b| !matches!(b.kind(), BlockKind::Table(_)))
        });
        let mut drag = None;
        let selection = if let Some(node) = node {
            Selection::Node(node)
        } else {
            let Some(pos) = q.position_at(p) else {
                return;
            };
            let (granularity, unit) = match event.click_count {
                1 => (Granularity::Char, None),
                2 => (Granularity::Word, q.word_range(pos)),
                _ => (Granularity::Block, q.block_range(pos.block)),
            };
            let anchor = match &self.state.selection {
                Selection::Text { anchor, .. } if event.modifiers.shift => Some(*anchor),
                _ => None,
            };
            let (origin, selection) = match (unit, anchor) {
                (Some(r), _) => (
                    r,
                    Selection::Text {
                        anchor: r.start,
                        head: r.end,
                    },
                ),
                (None, Some(anchor)) => (
                    TextRange {
                        start: anchor,
                        end: anchor,
                    },
                    Selection::Text { anchor, head: pos },
                ),
                (None, None) => (
                    TextRange {
                        start: pos,
                        end: pos,
                    },
                    Selection::caret(pos),
                ),
            };
            drag = Some(Drag {
                origin,
                granularity,
                position: p,
                autoscroll: None,
            });
            selection
        };
        drop(q);
        self.drag = drag;
        self.state.set_selection(selection);
        self.after_change(false, window, cx);
    }

    fn mouse_drag(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = &mut self.drag else {
            return;
        };
        drag.position = event.position;
        self.drag_to(event.position, window, cx);

        let bounds = self.view.bounds();
        let outside = event.position.y < bounds.top() || event.position.y > bounds.bottom();
        if let Some(drag) = &mut self.drag
            && outside
            && drag.autoscroll.is_none()
        {
            drag.autoscroll = Some(cx.spawn_in(window, async move |this, cx| {
                loop {
                    cx.background_executor().timer(AUTOSCROLL_INTERVAL).await;
                    let dragging = this.update_in(cx, |this, window, cx| {
                        let Some(drag) = &this.drag else {
                            return false;
                        };
                        let p = drag.position;
                        let bounds = this.view.bounds();
                        if p.y < bounds.top() || p.y > bounds.bottom() {
                            // The head goes past the edge, and revealing it
                            // scrolls further the further out the mouse is.
                            this.drag_to(p, window, cx);
                        }
                        true
                    });
                    if !matches!(dragging, Ok(true)) {
                        break;
                    }
                }
            }));
        }
    }

    fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.drag = None;
    }
}

impl MarkdownEditor {
    /// Extends the drag selection to the point `p`.
    fn drag_to(&mut self, p: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = &self.drag else {
            return;
        };
        let (origin, granularity) = (drag.origin, drag.granularity);
        let ts = window.text_system().clone();
        let view = self.view.clone();
        let mut q = view.query(self.layout_input(), &ts);
        let Some(pos) = q.position_at(p) else {
            return;
        };
        let unit = match granularity {
            Granularity::Char => None,
            Granularity::Word => q.word_range(pos),
            Granularity::Block => q.block_range(pos.block),
        };
        drop(q);
        let unit = unit.unwrap_or(TextRange {
            start: pos,
            end: pos,
        });
        // From one cell into another of the same table: a cell rectangle.
        let (from, to) = (origin.start.block, pos.block);
        if from != to
            && origin.start.block == origin.end.block
            && let (Some(a), Some(b)) = (self.state.cell_at(from), self.state.cell_at(to))
            && a.table == b.table
        {
            self.state.set_selection(Selection::Cells {
                anchor: from,
                head: to,
            });
            self.after_change(false, window, cx);
            return;
        }
        let selection = if self.state.cmp_pos(pos, origin.start) == Ordering::Less {
            Selection::Text {
                anchor: origin.end,
                head: unit.start,
            }
        } else {
            Selection::Text {
                anchor: origin.start,
                head: unit.end,
            }
        };
        self.state.set_selection(selection);
        self.after_change(false, window, cx);
    }

    /// Emits the event for a link, wiki link or tag in `block`.
    fn emit_link(&mut self, block: BlockId, span: &MarkSpan, cx: &mut Context<Self>) {
        let text = self
            .state
            .text_of(block)
            .and_then(|inline| inline.text.get(span.range.clone()))
            .unwrap_or_default()
            .to_string();
        let event = match &span.mark {
            Mark::Link { url, .. } => EditorEvent::LinkClicked(url.to_string()),
            Mark::Custom { id, attr } if *id == WikiLink::MARK => {
                EditorEvent::WikiLinkClicked(attr.as_ref().map_or(text, |a| a.to_string()))
            }
            Mark::Custom { id, .. } if *id == Tag::MARK => {
                EditorEvent::TagClicked(text.trim_start_matches('#').to_string())
            }
            _ => return,
        };
        cx.emit(event);
    }
}
