//! Typing, deleting, Enter and Tab.

use std::time::Instant;

use crate::edit::Selection;
use crate::edit::history::EditKind;
use crate::edit::inline_ops;
use crate::edit::ops::{Abort, EditResult};
use crate::edit::order::{self, Loc};
use crate::edit::state::{EditorState, Unit, next_boundary, prev_boundary};
use crate::edit::transaction::Tr;
use crate::model::{BlockId, BlockKind, Inline, Mark, TextPos};

/// Spaces inserted by Tab in a code block.
const CODE_INDENT: &str = "    ";

impl EditorState {
    /// Types `text` at the selection, replacing it. Input rules run after a
    /// single typed character.
    pub(crate) fn insert_text(&mut self, text: &str, now: Instant) -> bool {
        let rules = self.input_rules && text.chars().count() == 1;
        if !self.type_text(text, now) {
            return false;
        }
        if rules {
            self.apply_input_rules();
        }
        true
    }

    /// Types `text` without input rules. Used by the IME as well.
    pub(crate) fn type_text(&mut self, text: &str, now: Instant) -> bool {
        if text.is_empty() {
            return match &self.selection {
                Selection::Text { anchor, head } if anchor != head => self.delete_selection(),
                _ => false,
            };
        }
        if let Selection::Node(id) = self.selection {
            return self.type_after_node(id, text);
        }
        self.collapse_cells();
        let pending = self.pending_marks.take();
        // Marks toggled at a caret stay active for the text typed after it,
        // until the caret moves.
        let keep = pending.clone().filter(|_| self.selection.is_caret());
        let all = self.selection == Selection::All;
        let kind = match self.selection.head() {
            Some(head) if !all => EditKind::Insert { node: head.block },
            _ => EditKind::Other,
        };
        let text = text.to_string();
        let typed = text.clone();
        let range = self.ordered();
        let done = self.transact(kind, Some(now), &typed, move |tr, _| {
            let pos = delete_selected(tr, all, range)?;
            let text = if tr.is_cell(pos.block) {
                text.replace(['\n', '\r'], " ")
            } else {
                text
            };
            let mut marks = match pending {
                Some(marks) => marks,
                None => inline_ops::marks_at(tr.inline(pos.block)?, pos.offset),
            };
            if text.contains('\n') {
                // Inline code cannot hold a line break.
                marks.retain(|mark| *mark != Mark::Code);
            }
            let offset = tr.insert_typed(pos, &text, &marks)?;
            Ok(Selection::caret(TextPos {
                block: pos.block,
                offset,
            }))
        });
        if done && keep.is_some() {
            self.pending_marks = keep;
        }
        done
    }

    /// Typing while a node is selected starts a paragraph below it.
    fn type_after_node(&mut self, id: BlockId, text: &str) -> bool {
        let text = text.to_string();
        self.transact(EditKind::Other, None, "", move |tr, _| {
            let len = text.len();
            let block = tr.new_block(BlockKind::Paragraph, Inline::plain(text));
            let new = tr.insert_after(id, block)?;
            Ok(Selection::caret(TextPos {
                block: new,
                offset: len,
            }))
        })
    }

    /// Deletes the selected text or node.
    pub(crate) fn delete_selection(&mut self) -> bool {
        match self.selection.clone() {
            Selection::Node(id) => self.delete_node(id),
            Selection::Cells { anchor, head } => self.clear_cells(anchor, head),
            Selection::All => self.transact(EditKind::Other, None, "", move |tr, _| {
                Ok(Selection::caret(tr.clear_all()?))
            }),
            Selection::Text { anchor, head } if anchor != head => {
                let Some((start, end)) = self.ordered() else {
                    return false;
                };
                self.transact(EditKind::Other, None, "", move |tr, _| {
                    let pos = tr.delete_range(start, end)?;
                    Ok(Selection::caret(pos))
                })
            }
            Selection::Text { .. } => false,
        }
    }

    /// Removes a node block; the caret goes to the text after it, or before
    /// it, or into a new paragraph if there is no text left.
    pub(crate) fn delete_node(&mut self, id: BlockId) -> bool {
        self.transact(EditKind::Other, None, "", move |tr, _| {
            let Loc::Block(path) = tr.loc(id)? else {
                return Err(Abort::NotApplicable);
            };
            let after = order::next_after_subtree(tr.doc(), &path);
            let next = match after {
                Some(stop) if stop.text => Some(TextPos {
                    block: stop.id,
                    offset: 0,
                }),
                Some(stop) => order::next_text(tr.doc(), &stop.loc).map(|s| TextPos {
                    block: s.id,
                    offset: 0,
                }),
                None => None,
            };
            let previous = order::prev_text(tr.doc(), &Loc::Block(path.clone())).map(|stop| {
                let len = order::inline_at(tr.doc(), &stop.loc).map_or(0, |i| i.text.len());
                TextPos {
                    block: stop.id,
                    offset: len,
                }
            });
            let caret = match next.or(previous) {
                Some(pos) => pos,
                None => {
                    let block = tr.new_block(BlockKind::Paragraph, Inline::default());
                    let new = tr.insert_after(id, block)?;
                    TextPos {
                        block: new,
                        offset: 0,
                    }
                }
            };
            tr.remove(id)?;
            Ok(Selection::caret(caret))
        })
    }

    /// Backspace.
    pub(crate) fn delete_backward(&mut self, unit: Unit, now: Instant) -> bool {
        if self.rule_armed {
            return self.undo();
        }
        let head = match &self.selection {
            Selection::Text { anchor, head } if anchor == head => *head,
            _ => return self.delete_selection(),
        };
        let Some(text) = self.text_of(head.block).map(|i| i.text.clone()) else {
            return false;
        };
        match prev_boundary(&text, head.offset, unit) {
            Some(start) => {
                let pos = TextPos {
                    block: head.block,
                    offset: start,
                };
                self.delete_in_node(
                    pos,
                    head,
                    EditKind::DeleteBackward { node: head.block },
                    now,
                )
            }
            None => self.backspace_at_start(head.block),
        }
    }

    /// Delete.
    pub(crate) fn delete_forward(&mut self, unit: Unit, now: Instant) -> bool {
        self.rule_armed = false;
        let head = match &self.selection {
            Selection::Text { anchor, head } if anchor == head => *head,
            _ => return self.delete_selection(),
        };
        let Some(text) = self.text_of(head.block).map(|i| i.text.clone()) else {
            return false;
        };
        match next_boundary(&text, head.offset, unit) {
            Some(end) => {
                let pos = TextPos {
                    block: head.block,
                    offset: end,
                };
                self.delete_in_node(head, pos, EditKind::DeleteForward { node: head.block }, now)
            }
            None => self.delete_at_end(head.block),
        }
    }

    fn delete_in_node(
        &mut self,
        start: TextPos,
        end: TextPos,
        kind: EditKind,
        now: Instant,
    ) -> bool {
        self.transact(kind, Some(now), "", move |tr, _| {
            tr.replace(start.block, start.offset..end.offset, Inline::default())?;
            Ok(Selection::caret(start))
        })
    }

    /// Backspace at the start of a text node.
    fn backspace_at_start(&mut self, node: BlockId) -> bool {
        let Some(Loc::Block(path)) = self.locate(node) else {
            return false;
        };
        let Some(block) = order::block_at(&self.doc.blocks, &path) else {
            return false;
        };
        if block.kind != BlockKind::Paragraph {
            return self.transact(EditKind::Other, None, "", move |tr, sel| {
                tr.set_kind(node, BlockKind::Paragraph)?;
                Ok(sel.clone())
            });
        }
        let empty = block.content.text.is_empty();
        let Some(previous) = order::prev_stop(&self.doc, &Loc::Block(path.clone())) else {
            return false;
        };
        if !previous.text || previous.loc.is_cell() {
            let target = match &previous.loc {
                Loc::Cell { table, .. } => order::block_at(&self.doc.blocks, table).map(|b| b.id),
                Loc::Block(_) => Some(previous.id),
            };
            let Some(target) = target else {
                return false;
            };
            if empty {
                return self.transact(EditKind::Other, None, "", move |tr, _| {
                    tr.remove(node)?;
                    Ok(Selection::Node(target))
                });
            }
            return self.set_selection(Selection::Node(target));
        }
        let into = previous.id;
        self.transact(EditKind::Other, None, "", move |tr, _| {
            let offset = tr.join(into, node)?;
            Ok(Selection::caret(TextPos {
                block: into,
                offset,
            }))
        })
    }

    /// Delete at the end of a text node: the next text block is joined in,
    /// a node block is selected.
    fn delete_at_end(&mut self, node: BlockId) -> bool {
        let Some(loc) = self.locate(node) else {
            return false;
        };
        if loc.is_cell() {
            return false;
        }
        let Some(next) = order::next_stop(&self.doc, &loc) else {
            return false;
        };
        if !next.text || next.loc.is_cell() {
            let target = match &next.loc {
                Loc::Cell { table, .. } => order::block_at(&self.doc.blocks, table).map(|b| b.id),
                Loc::Block(_) => Some(next.id),
            };
            return target.is_some_and(|target| self.set_selection(Selection::Node(target)));
        }
        let from = next.id;
        self.transact(EditKind::Other, None, "", move |tr, _| {
            let offset = tr.join(node, from)?;
            Ok(Selection::caret(TextPos {
                block: node,
                offset,
            }))
        })
    }

    /// Shift+Enter: a line break inside the block.
    pub(crate) fn soft_break(&mut self, now: Instant) -> bool {
        if matches!(self.selection, Selection::Node(_)) {
            return false;
        }
        if self
            .selection
            .head()
            .and_then(|head| self.locate(head.block))
            .is_some_and(|loc| loc.is_cell())
        {
            return false;
        }
        self.type_text("\n", now)
    }

    /// Enter.
    ///
    /// - list and task items: a new item of the same kind (tasks start
    ///   unticked); in an empty item the item is outdented, at the top level
    ///   it becomes a paragraph
    /// - headings: at the end a paragraph follows, elsewhere the heading is
    ///   split
    /// - code blocks: a line break; on an empty last line Enter leaves the
    ///   block (the empty line is removed and a paragraph follows)
    /// - raw blocks: a line break
    /// - table cells: the caret moves to the cell below; a new row is added
    ///   below the last one
    /// - a selected node: a paragraph below it
    pub(crate) fn enter(&mut self, now: Instant) -> bool {
        if let Selection::Node(id) = self.selection {
            return self.transact(EditKind::Other, None, "", move |tr, _| {
                let block = tr.new_block(BlockKind::Paragraph, Inline::default());
                let new = tr.insert_after(id, block)?;
                Ok(Selection::caret(TextPos {
                    block: new,
                    offset: 0,
                }))
            });
        }
        if self.selection == Selection::All {
            return self.delete_selection();
        }
        let Some((start, end)) = self.ordered() else {
            return false;
        };
        let Some(loc) = self.locate(start.block) else {
            return false;
        };
        if loc.is_cell() {
            return self.cell_enter(start.block);
        }
        let Some(block) = order::block_at(&self.doc.blocks, loc.block_path()) else {
            return false;
        };
        let kind = block.kind.clone();
        let text = block.content.text.clone();
        let at_end = end.block == start.block && end.offset == text.len();

        match &kind {
            BlockKind::CodeBlock { .. } => {
                if start == end && at_end && text.ends_with('\n') {
                    return self.transact(EditKind::Other, None, "", move |tr, _| {
                        let len = text.len();
                        tr.replace(start.block, len - 1..len, Inline::default())?;
                        let block = tr.new_block(BlockKind::Paragraph, Inline::default());
                        let new = tr.insert_after(start.block, block)?;
                        Ok(Selection::caret(TextPos {
                            block: new,
                            offset: 0,
                        }))
                    });
                }
                return self.type_text("\n", now);
            }
            BlockKind::Raw => return self.type_text("\n", now),
            _ => {}
        }

        let empty = start == end && text.is_empty();
        if empty && order::is_item(&kind) {
            let nested = self.is_nested_item(start.block);
            if nested {
                return self.outdent_blocks(vec![start.block]);
            }
            return self.transact(EditKind::Other, None, "", move |tr, sel| {
                tr.set_kind(start.block, BlockKind::Paragraph)?;
                Ok(sel.clone())
            });
        }
        if empty && kind == BlockKind::Quote {
            return self.transact(EditKind::Other, None, "", move |tr, sel| {
                tr.set_kind(start.block, BlockKind::Paragraph)?;
                Ok(sel.clone())
            });
        }

        self.transact(EditKind::Other, None, "", move |tr, _| {
            let pos = tr.delete_range(start, end)?;
            let block = tr.block(pos.block)?;
            let len = block.content.text.len();
            let kind = block.kind.clone();
            let new_kind = match kind {
                BlockKind::Heading(_) if pos.offset == len => BlockKind::Paragraph,
                BlockKind::Heading(_) if pos.offset == 0 && len > 0 => {
                    // An empty paragraph above, the caret stays in the heading.
                    let above = tr.new_block(BlockKind::Paragraph, Inline::default());
                    tr.insert_before(pos.block, above)?;
                    return Ok(Selection::caret(pos));
                }
                BlockKind::TaskItem { .. } => BlockKind::TaskItem { checked: false },
                BlockKind::OrderedItem { number } => BlockKind::OrderedItem {
                    number: number.saturating_add(1),
                },
                other => other,
            };
            let new = tr.split(pos, new_kind)?;
            Ok(Selection::caret(TextPos {
                block: new,
                offset: 0,
            }))
        })
    }

    pub(crate) fn is_nested_item(&self, id: BlockId) -> bool {
        let Some(Loc::Block(path)) = self.locate(id) else {
            return false;
        };
        path.len() > 1
            && order::block_at(&self.doc.blocks, &path[..path.len() - 1])
                .is_some_and(|parent| order::is_item(&parent.kind))
    }

    /// Tab: indents list items, inserts spaces in code, moves to the next
    /// table cell.
    pub(crate) fn tab(&mut self, now: Instant) -> bool {
        let Some(head) = self.selection.head() else {
            return false;
        };
        let Some(loc) = self.locate(head.block) else {
            return false;
        };
        if loc.is_cell() {
            return self.next_cell(head.block, true);
        }
        let in_code = order::block_at(&self.doc.blocks, loc.block_path())
            .is_some_and(|block| matches!(block.kind, BlockKind::CodeBlock { .. }));
        if in_code {
            if !self.selection.is_caret() {
                return self.shift_code_lines(true);
            }
            return self.type_text(CODE_INDENT, now);
        }
        self.indent()
    }

    /// Shift+Tab: outdents list items, removes indentation in code, moves to
    /// the previous table cell.
    pub(crate) fn shift_tab(&mut self) -> bool {
        let Some(head) = self.selection.head() else {
            return false;
        };
        let Some(loc) = self.locate(head.block) else {
            return false;
        };
        if loc.is_cell() {
            return self.next_cell(head.block, false);
        }
        let in_code = order::block_at(&self.doc.blocks, loc.block_path())
            .is_some_and(|block| matches!(block.kind, BlockKind::CodeBlock { .. }));
        if in_code {
            if !self.selection.is_caret() {
                return self.shift_code_lines(false);
            }
            return self.dedent_code_line(head);
        }
        self.outdent()
    }

    /// Tab and Shift+Tab with a selection in a code block: indents or
    /// outdents every selected line and keeps the selection on them.
    fn shift_code_lines(&mut self, indent: bool) -> bool {
        let Selection::Text { anchor, head } = self.selection.clone() else {
            return false;
        };
        if anchor.block != head.block {
            return false;
        }
        let node = head.block;
        let Some(text) = self.text_of(node).map(|i| i.text.clone()) else {
            return false;
        };
        let (from, to) = (
            anchor.offset.min(head.offset),
            anchor.offset.max(head.offset),
        );
        let first = text[..from].rfind('\n').map_or(0, |i| i + 1);
        // A selection ending at the start of a line leaves that line alone.
        let last = if to > from && text[..to].ends_with('\n') {
            to - 1
        } else {
            to
        };
        let mut starts = vec![first];
        starts.extend(
            text[first..last]
                .match_indices('\n')
                .map(|(i, _)| first + i + 1),
        );
        // (line start, bytes inserted (+) or removed (-)), last line first.
        let edits: Vec<(usize, usize)> = starts
            .iter()
            .map(|&start| {
                let len = if indent {
                    CODE_INDENT.len()
                } else {
                    text[start..]
                        .bytes()
                        .take(CODE_INDENT.len())
                        .take_while(|b| *b == b' ')
                        .count()
                };
                (start, len)
            })
            .filter(|(_, len)| *len > 0)
            .collect();
        if edits.is_empty() {
            return false;
        }
        let shift = |offset: usize| -> usize {
            let mut out = offset;
            for &(start, len) in &edits {
                if indent {
                    if start < offset || (start == offset && start == first) {
                        out += len;
                    }
                } else if start < offset {
                    out -= len.min(offset - start);
                }
            }
            out
        };
        let after = Selection::Text {
            anchor: TextPos {
                block: node,
                offset: shift(anchor.offset),
            },
            head: TextPos {
                block: node,
                offset: shift(head.offset),
            },
        };
        self.transact(EditKind::Other, None, "", move |tr, _| {
            for &(start, len) in edits.iter().rev() {
                if indent {
                    tr.replace(node, start..start, Inline::plain(CODE_INDENT))?;
                } else {
                    tr.replace(node, start..start + len, Inline::default())?;
                }
            }
            Ok(after)
        })
    }

    fn dedent_code_line(&mut self, head: TextPos) -> bool {
        let Some(text) = self.text_of(head.block).map(|i| i.text.clone()) else {
            return false;
        };
        let line_start = text[..head.offset].rfind('\n').map_or(0, |i| i + 1);
        let spaces = text[line_start..]
            .bytes()
            .take(CODE_INDENT.len())
            .take_while(|b| *b == b' ')
            .count();
        if spaces == 0 {
            return false;
        }
        self.transact(EditKind::Other, None, "", move |tr, sel| {
            tr.replace(
                head.block,
                line_start..line_start + spaces,
                Inline::default(),
            )?;
            let shift = |pos: TextPos| TextPos {
                block: pos.block,
                offset: if pos.block == head.block && pos.offset >= line_start {
                    pos.offset.saturating_sub(spaces).max(line_start)
                } else {
                    pos.offset
                },
            };
            Ok(match sel {
                Selection::Text { anchor, head } => Selection::Text {
                    anchor: shift(*anchor),
                    head: shift(*head),
                },
                other => other.clone(),
            })
        })
    }

    /// Multi-line plain text: the first line goes into the current block,
    /// every further line becomes a paragraph. In code and raw blocks the
    /// text is inserted as is.
    pub(crate) fn insert_plain_text(&mut self, text: &str) -> bool {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let literal_or_cell = self.selection.head().is_some_and(|head| {
            self.locate(head.block).is_some_and(|loc| {
                loc.is_cell()
                    || order::block_at(&self.doc.blocks, loc.block_path())
                        .is_some_and(|b| order::is_literal(&b.kind))
            })
        });
        if !text.contains('\n') || literal_or_cell {
            let typed = self.type_text(&text, Instant::now());
            self.history.seal();
            return typed;
        }
        // Blank lines separate paragraphs; they do not become empty ones.
        let lines: Vec<&str> = text.split('\n').collect();
        let last = lines.len() - 1;
        let blocks: Vec<Inline> = lines
            .iter()
            .enumerate()
            .filter(|(index, line)| *index == 0 || *index == last || !line.trim().is_empty())
            .map(|(_, line)| Inline::plain(*line))
            .collect();
        self.insert_inline_lines(blocks)
    }

    /// Inserts paragraphs at the selection. The first merges into the
    /// current block, the text behind the caret is appended to the last.
    pub(crate) fn insert_inline_lines(&mut self, lines: Vec<Inline>) -> bool {
        if let Selection::Node(id) = self.selection {
            return self.transact(EditKind::Other, None, "", move |tr, _| {
                let mut after = id;
                let mut caret = None;
                for line in lines {
                    let len = line.text.len();
                    let block = tr.new_block(BlockKind::Paragraph, line);
                    after = tr.insert_after(after, block)?;
                    caret = Some(TextPos {
                        block: after,
                        offset: len,
                    });
                }
                caret.map(Selection::caret).ok_or(Abort::NotApplicable)
            });
        }
        let all = self.selection == Selection::All;
        let range = self.ordered();
        self.transact(EditKind::Other, None, "", move |tr, _| {
            let pos = delete_selected(tr, all, range)?;
            insert_lines(tr, pos, lines)
        })
    }
}

/// Deletes the selection: the whole document if `all` (one empty paragraph
/// is left), else the text range. Returns where the caret goes.
pub(crate) fn delete_selected(
    tr: &mut Tr<'_>,
    all: bool,
    range: Option<(TextPos, TextPos)>,
) -> EditResult<TextPos> {
    if all {
        return Ok(tr.clear_all()?);
    }
    let (start, end) = range.ok_or(Abort::NotApplicable)?;
    tr.delete_range(start, end)
}

/// Inserts `lines` at `pos`: see [`EditorState::insert_inline_lines`].
pub(crate) fn insert_lines(
    tr: &mut Tr<'_>,
    pos: TextPos,
    lines: Vec<Inline>,
) -> EditResult<Selection> {
    if tr.is_cell(pos.block) || tr.is_literal(pos.block) {
        // One line in a cell, the text as is in code.
        let separator = if tr.is_cell(pos.block) { " " } else { "\n" };
        let mut joined = Inline::default();
        for (index, line) in lines.iter().enumerate() {
            if index > 0 {
                joined = inline_ops::concat(&joined, &Inline::plain(separator));
            }
            joined = inline_ops::concat(&joined, line);
        }
        if tr.is_literal(pos.block) {
            joined.marks.clear();
        }
        let len = joined.text.len();
        tr.replace(pos.block, pos.offset..pos.offset, joined)?;
        return Ok(Selection::caret(TextPos {
            block: pos.block,
            offset: pos.offset + len,
        }));
    }
    let mut lines = lines.into_iter();
    let Some(first) = lines.next() else {
        return Ok(Selection::caret(pos));
    };
    let rest: Vec<Inline> = lines.collect();
    let first_len = first.text.len();
    tr.replace(pos.block, pos.offset..pos.offset, first)?;
    let split_at = TextPos {
        block: pos.block,
        offset: pos.offset + first_len,
    };
    if rest.is_empty() {
        return Ok(Selection::caret(split_at));
    }
    // In list items every line is an item of the same kind, and the
    // children follow the last one, as with Enter.
    let kind = tr.block(pos.block)?.kind.clone();
    let line_kind = match kind {
        BlockKind::BulletItem => BlockKind::BulletItem,
        BlockKind::TaskItem { .. } => BlockKind::TaskItem { checked: false },
        BlockKind::OrderedItem { number } => BlockKind::OrderedItem {
            number: number.saturating_add(1),
        },
        _ => BlockKind::Paragraph,
    };
    let tail = tr.split(split_at, line_kind.clone())?;
    let (parent, mut index) = tr.parent(tail)?;
    let count = rest.len();
    let mut caret = TextPos {
        block: tail,
        offset: 0,
    };
    for (n, line) in rest.into_iter().enumerate() {
        if n + 1 == count {
            let len = line.text.len();
            tr.replace(tail, 0..0, line)?;
            caret = TextPos {
                block: tail,
                offset: len,
            };
        } else {
            let block = tr.new_block(line_kind.clone(), line);
            tr.insert_block(parent, index, block)?;
            index += 1;
        }
    }
    Ok(Selection::caret(caret))
}

/// `true` for marks that can be toggled at a caret as pending marks.
pub(crate) fn pending_toggle(current: &[Mark], mark: &Mark) -> Vec<Mark> {
    let mut marks: Vec<Mark> = current.to_vec();
    if let Some(index) = marks.iter().position(|m| inline_ops::same_kind(m, mark)) {
        marks.remove(index);
    } else {
        marks.push(mark.clone());
    }
    marks
}
