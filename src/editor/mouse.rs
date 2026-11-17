//! Mouse: placing the caret, selecting, checkboxes and link clicks.

use std::cmp::Ordering;
use std::time::Duration;

use gpui::{
    Context, FocusHandle, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, Task,
    Window, point, px,
};

use super::{MarkdownEditor, table};
use crate::edit::Selection;
use crate::event::EditorEvent;
use crate::model::{BlockId, BlockKind, Mark, MarkSpan, TextRange};
use crate::view::{Affordance, ViewInput, ViewSource};

/// How often the view scrolls while the mouse is dragged outside of it.
const AUTOSCROLL_INTERVAL: Duration = Duration::from_millis(40);

/// What a drag selects by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Granularity {
    Char,
    Word,
    Block,
}

/// A selected image being resized by one of its handles.
pub(super) struct ImageDrag {
    pub id: BlockId,
    /// Dragging the right handle (else the left one).
    right: bool,
    start_x: Pixels,
    start_width: f32,
    /// The width shown now, in pixels.
    pub width: u32,
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
            selected_cells: match self.state.selection {
                Selection::Cells { anchor, head } => self
                    .state
                    .cell_rect(anchor, head)
                    .map(|r| r.cells.into_iter().flatten().collect())
                    .unwrap_or_default(),
                _ => Vec::new(),
            },
            highlights: &self.highlights,
            search: self.find.as_ref().map_or(&[], |f| &f.matches),
            search_current: self.find.as_ref().and_then(|f| f.current_range()),
            caret_visible: self.focused && self.caret_on,
            marked: self.state.valid_marked(),
            hover: self.hover,
            open_handle: self.table_menu.map(|m| m.handle),
            resolve_src: self.options.resolve_src.as_ref(),
            folded: &self.folded,
            drag_handles: self.drag.is_none()
                && self.image_drag.is_none()
                && self.column_drag.is_none(),
            block_drag: self.block_drag.map(|d| (d.from, d.gap)),
        }
    }

    fn input_focus(&self) -> &FocusHandle {
        &self.focus
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus.focus(window, cx);
        self.drag = None;
        self.table_menu = None;
        let p = event.position;
        let ts = window.text_system().clone();
        let view = self.view.clone();
        let mut q = view.query(self.layout_input(), &ts);

        if let Some(ix) = q.drag_handle_at(p) {
            drop(q);
            self.start_block_drag(ix);
            cx.notify();
            return;
        }
        if let Some(heading) = q.fold_toggle_at(p) {
            drop(q);
            self.toggle_fold(heading, window, cx);
            return;
        }
        if event.modifiers.secondary(cx)
            && let Some((block, span)) = q.link_at(p)
        {
            drop(q);
            self.emit_link(block, &span, cx);
            return;
        }
        if let Selection::Node(id) = self.state.selection
            && let Some((right, bounds)) = q.image_handle_at(p, id)
        {
            drop(q);
            let w = bounds.size.width.as_f32();
            self.image_drag = Some(ImageDrag {
                id,
                right,
                start_x: p.x,
                start_width: w,
                width: w.round() as u32,
            });
            return;
        }
        let caret = self.state.selection.head().map(|h| h.block);
        if let Some(a) = q.affordance_at(p, self.hover.or(Some(p)), caret) {
            let anchor = match a {
                Affordance::RowHandle { table, row } => q
                    .table_geom(table)
                    .map(|g| point(g.bounds.left() - px(16.), g.rows[row].1)),
                Affordance::ColumnHandle { table, col } => q
                    .table_geom(table)
                    .map(|g| point(g.cols[col].0, g.bounds.top())),
                _ => None,
            }
            .unwrap_or(p);
            if let Affordance::ColumnBorder { table, col } = a {
                let start = q
                    .table_geom(table)
                    .map(|g| g.cols.iter().map(|(l, r)| (*r - *l).as_f32()).collect());
                drop(q);
                if let Some(start) = start {
                    self.column_drag = Some(table::ColumnDrag {
                        table,
                        col,
                        start_x: p.x,
                        start,
                    });
                }
                return;
            }
            drop(q);
            if self.click_affordance(a, anchor, window, cx) {
                return;
            }
            q = view.query(self.layout_input(), &ts);
        }
        if let Some(block) = q.attachment_at(p) {
            drop(q);
            self.emit_attachment(block, cx);
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
        if self.block_drag.is_some() {
            let ts = window.text_system().clone();
            let view = self.view.clone();
            let gap = view
                .query(self.layout_input(), &ts)
                .drop_gap_at(event.position);
            if let Some(drag) = &mut self.block_drag {
                drag.gap = Some(gap);
            }
            cx.notify();
            return;
        }
        if let Some(drag) = &mut self.image_drag {
            let dx = (event.position.x - drag.start_x).as_f32();
            let dx = if drag.right { dx } else { -dx };
            let max = self.view.content_width().as_f32();
            drag.width = (drag.start_width + dx).clamp(40., max.max(40.)).round() as u32;
            cx.notify();
            return;
        }
        if let Some(drag) = &self.column_drag {
            let widths = drag.widths_at(event.position.x);
            self.column_widths.insert(drag.table, widths);
            cx.notify();
            return;
        }
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

    fn mouse_up(&mut self, _: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.block_drag.is_some() {
            self.finish_block_drag(window, cx);
            return;
        }
        if self.drag.take().is_some() {
            // The format bar hides while the mouse selects.
            cx.notify();
        }
        if let Some(drag) = self.image_drag.take() {
            if (drag.width as f32 - drag.start_width).abs() >= 1. {
                self.state.set_image_width(drag.id, Some(drag.width));
            }
            self.after_change(false, window, cx);
        }
        if let Some(drag) = self.column_drag.take() {
            self.record_widths(drag.table);
            self.sync_meta(cx);
        }
    }

    fn a11y_toggle(&mut self, id: BlockId, window: &mut Window, cx: &mut Context<Self>) {
        self.state.toggle_task_item(id);
        self.after_change(false, window, cx);
    }

    fn mouse_hover(
        &mut self,
        position: Option<Point<Pixels>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if position == self.hover {
            return;
        }
        let ts = window.text_system().clone();
        let view = self.view.clone();
        let mut q = view.query(self.layout_input(), &ts);
        let caret = self.state.selection.head().map(|h| h.block);
        // Repaint only when what the table affordances or the fold chevrons
        // show changes.
        let mut key = |p: Option<Point<Pixels>>| {
            p.map(|p| {
                let heading = q
                    .heading_row_at(p)
                    .map(|(id, r)| (id, r.dilate(px(4.)).contains(&p)));
                (
                    q.table_focus_at(p),
                    q.affordance_at(p, Some(p), caret),
                    heading,
                )
            })
        };
        let changed = key(self.hover) != key(position);
        drop(q);
        self.hover = position;
        if changed {
            cx.notify();
        }
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
    /// Emits [`EditorEvent::AttachmentClicked`] for attachment paragraph
    /// `block`. `false` if it is not one.
    fn emit_attachment(&mut self, block: BlockId, cx: &mut Context<Self>) -> bool {
        let url = self
            .state
            .doc
            .block(block)
            .and_then(crate::view::attachment_of)
            .map(|url| url.to_string());
        if let Some(url) = &url {
            cx.emit(EditorEvent::AttachmentClicked(url.clone()));
        }
        url.is_some()
    }

    fn emit_link(&mut self, block: BlockId, span: &MarkSpan, cx: &mut Context<Self>) {
        if self.emit_attachment(block, cx) {
            return;
        }
        let text = self
            .state
            .text_of(block)
            .and_then(|inline| inline.text.get(span.range.clone()))
            .unwrap_or_default()
            .to_string();
        let event = match &span.mark {
            Mark::Link { url, .. } => Some(EditorEvent::LinkClicked(url.to_string())),
            mark @ Mark::Custom { .. } => self
                .state
                .doc
                .extensions
                .iter()
                .find_map(|e| e.click(mark, &text)),
            _ => None,
        };
        if let Some(event) = event {
            cx.emit(event);
        }
    }
}
