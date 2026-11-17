//! Mark commands: toggling formatting and links.

use std::ops::Range;

use crate::edit::Selection;
use crate::edit::commands::text::pending_toggle;
use crate::edit::history::EditKind;
use crate::edit::inline_ops;
use crate::edit::order;
use crate::edit::state::EditorState;
use crate::edit::step::Step;
use crate::model::{BlockId, Inline, Mark, MarkSpan, TextPos};

impl EditorState {
    /// The text ranges of the selection, per node, without code and raw
    /// blocks and without empty ranges.
    pub(crate) fn selected_segments(&self) -> Vec<(BlockId, Range<usize>)> {
        let Some((start, end)) = self.ordered() else {
            return Vec::new();
        };
        let (Some(from), Some(to)) = (self.locate(start.block), self.locate(end.block)) else {
            return Vec::new();
        };
        order::stops_between(&self.doc, &from, &to)
            .into_iter()
            .filter(|stop| stop.text)
            .filter(|stop| {
                stop.loc.is_cell()
                    || order::block_at(&self.doc.blocks, stop.loc.block_path())
                        .is_some_and(|b| !order::is_literal(&b.kind))
            })
            .filter_map(|stop| {
                let len = order::inline_at(&self.doc, &stop.loc)?.text.len();
                let from = if stop.id == start.block {
                    start.offset
                } else {
                    0
                };
                let to = if stop.id == end.block {
                    end.offset
                } else {
                    len
                };
                (from < to).then_some((stop.id, from..to))
            })
            .collect()
    }

    /// The marks text typed at the caret gets.
    pub(crate) fn caret_marks(&self) -> Vec<Mark> {
        if let Some(pending) = &self.pending_marks {
            return pending.clone();
        }
        match self.selection.head() {
            Some(head) => self
                .text_of(head.block)
                .map(|inline| inline_ops::marks_at(inline, head.offset))
                .unwrap_or_default(),
            None => Vec::new(),
        }
    }

    /// Whether the whole selection has `mark` (for toolbar state). At a
    /// caret: whether typed text gets it.
    pub(crate) fn has_mark(&self, mark: &Mark) -> bool {
        if self.selection.is_caret() {
            return self
                .caret_marks()
                .iter()
                .any(|m| inline_ops::same_kind(m, mark));
        }
        let segments = self.selected_segments();
        !segments.is_empty()
            && segments.iter().all(|(id, range)| {
                self.text_of(*id)
                    .is_some_and(|inline| inline_ops::covered(inline, range.clone(), mark))
            })
    }

    /// Adds `mark` to the selection, or removes it if the whole selection
    /// has it. At a caret it applies to the text typed next.
    pub(crate) fn toggle_mark(&mut self, mark: Mark) -> bool {
        if self.selection.is_caret() {
            let in_literal = self.selection.head().is_some_and(|head| {
                self.locate(head.block).is_some_and(|loc| {
                    order::block_at(&self.doc.blocks, loc.block_path())
                        .is_some_and(|b| !loc.is_cell() && order::is_literal(&b.kind))
                })
            });
            if in_literal {
                return false;
            }
            let marks = pending_toggle(&self.caret_marks(), &mark);
            self.pending_marks = Some(marks);
            self.rule_armed = false;
            return true;
        }
        let segments = self.selected_segments();
        if segments.is_empty() {
            return false;
        }
        let remove = self.has_mark(&mark);
        self.transact(EditKind::Other, None, "", move |tr, sel| {
            for (node, range) in segments {
                let step = if remove {
                    Step::RemoveMark {
                        node,
                        range,
                        mark: mark.clone(),
                    }
                } else {
                    Step::AddMark {
                        node,
                        range,
                        mark: mark.clone(),
                    }
                };
                tr.step(step)?;
            }
            Ok(sel.clone())
        })
    }

    /// Links the selection to `url`, or changes the link at the caret. At a
    /// caret outside a link the URL is inserted as linked text. `None`
    /// removes the links in the selection or the link at the caret.
    pub(crate) fn set_link(&mut self, url: Option<&str>) -> bool {
        let link = url.map(|url| Mark::Link {
            url: url.to_string().into(),
            title: None,
        });
        let kind = Mark::Link {
            url: "".into(),
            title: None,
        };
        if let Some(head) = self.selection.head().filter(|_| self.selection.is_caret()) {
            let Some(inline) = self.text_of(head.block) else {
                return false;
            };
            let span = inline_ops::span_at(inline, head.offset, &kind).map(|s| s.range.clone());
            let in_literal = self.locate(head.block).is_some_and(|loc| {
                !loc.is_cell()
                    && order::block_at(&self.doc.blocks, loc.block_path())
                        .is_some_and(|b| order::is_literal(&b.kind))
            });
            if in_literal {
                return false;
            }
            return match (span, link) {
                (Some(range), Some(link)) => {
                    self.transact(EditKind::Other, None, "", move |tr, sel| {
                        tr.step(Step::AddMark {
                            node: head.block,
                            range,
                            mark: link,
                        })?;
                        Ok(sel.clone())
                    })
                }
                (Some(range), None) => self.transact(EditKind::Other, None, "", move |tr, sel| {
                    tr.step(Step::RemoveMark {
                        node: head.block,
                        range,
                        mark: kind,
                    })?;
                    Ok(sel.clone())
                }),
                (None, Some(link)) => {
                    let Some(url) = url.filter(|url| !url.is_empty()) else {
                        return false;
                    };
                    let text = url.replace(['\n', '\r'], " ");
                    self.transact(EditKind::Other, None, "", move |tr, _| {
                        let len = text.len();
                        let insert = Inline {
                            marks: vec![MarkSpan {
                                range: 0..len,
                                mark: link,
                            }],
                            text,
                        };
                        tr.replace(head.block, head.offset..head.offset, insert)?;
                        Ok(Selection::caret(TextPos {
                            block: head.block,
                            offset: head.offset + len,
                        }))
                    })
                }
                (None, None) => false,
            };
        }
        let segments = self.selected_segments();
        if segments.is_empty() {
            return false;
        }
        self.transact(EditKind::Other, None, "", move |tr, sel| {
            for (node, range) in segments {
                let step = match &link {
                    Some(link) => Step::AddMark {
                        node,
                        range,
                        mark: link.clone(),
                    },
                    None => Step::RemoveMark {
                        node,
                        range,
                        mark: kind.clone(),
                    },
                };
                tr.step(step)?;
            }
            Ok(sel.clone())
        })
    }
}
