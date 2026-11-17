//! Markdown input rules: typing `# ` at the start of a paragraph makes it a
//! heading, closing `**x**` makes `x` bold, and so on.
//!
//! A rule runs right after the character that completes it was typed, as a
//! history entry of its own. Undo (or Backspace right away) gives back the
//! literal text.

use crate::edit::Selection;
use crate::edit::commands::blocks::ensure_text_after;
use crate::edit::history::EditKind;
use crate::edit::inline_ops;
use crate::edit::ops::Abort;
use crate::edit::order;
use crate::edit::state::EditorState;
use crate::edit::step::Step;
use crate::model::{BlockKind, Inline, Mark, TextPos};

/// Delimiters of inline rules, longest first so `**` wins over `*`.
const INLINE_RULES: &[(&str, Mark)] = &[
    ("**", Mark::Bold),
    ("~~", Mark::Strike),
    ("==", Mark::Highlight),
    ("*", Mark::Italic),
    ("`", Mark::Code),
];

/// The block a paragraph turns into when its text before the caret is
/// `prefix`.
fn block_rule(prefix: &str) -> Option<BlockKind> {
    Some(match prefix {
        "# " => BlockKind::Heading(1),
        "## " => BlockKind::Heading(2),
        "### " => BlockKind::Heading(3),
        "- " | "* " => BlockKind::BulletItem,
        "[] " | "[ ] " | "- [ ] " | "* [ ] " => BlockKind::TaskItem { checked: false },
        "[x] " | "- [x] " => BlockKind::TaskItem { checked: true },
        "> " => BlockKind::Quote,
        "```" => BlockKind::CodeBlock { lang: None },
        _ => {
            let digits = prefix.strip_suffix(". ")?;
            if digits.is_empty() || digits.len() > 9 || !digits.bytes().all(|b| b.is_ascii_digit())
            {
                return None;
            }
            BlockKind::OrderedItem {
                number: digits.parse().ok()?,
            }
        }
    })
}

impl EditorState {
    /// Runs the first input rule that matches at the caret. Returns `true`
    /// if one fired.
    pub(crate) fn apply_input_rules(&mut self) -> bool {
        let fired = self.block_input_rule() || self.inline_input_rule();
        if fired {
            self.rule_armed = true;
        }
        fired
    }

    fn block_input_rule(&mut self) -> bool {
        let Some(head) = self.selection.head().filter(|_| self.selection.is_caret()) else {
            return false;
        };
        let Some(loc) = self.locate(head.block) else {
            return false;
        };
        if loc.is_cell() {
            return false;
        }
        let Some(block) = order::block_at(&self.doc.blocks, loc.block_path()) else {
            return false;
        };
        let text = &block.content.text;
        let Some(prefix) = text.get(..head.offset) else {
            return false;
        };
        let node = head.block;
        let len = prefix.len();
        if block.kind == BlockKind::Paragraph && prefix == "---" && text == "---" {
            return self.transact(EditKind::Other, None, "", move |tr, _| {
                tr.replace(node, 0..len, Inline::default())?;
                tr.set_kind(node, BlockKind::Rule)?;
                let next = ensure_text_after(tr, node)?;
                Ok(Selection::caret(TextPos {
                    block: next,
                    offset: 0,
                }))
            });
        }
        let kind = match block.kind {
            BlockKind::Paragraph => block_rule(prefix),
            BlockKind::BulletItem => match prefix {
                "[] " | "[ ] " => Some(BlockKind::TaskItem { checked: false }),
                "[x] " => Some(BlockKind::TaskItem { checked: true }),
                _ => None,
            },
            _ => None,
        };
        let Some(kind) = kind else {
            return false;
        };
        self.transact(EditKind::Other, None, "", move |tr, _| {
            tr.replace(node, 0..len, Inline::default())?;
            tr.set_kind(node, kind)?;
            Ok(Selection::caret(TextPos {
                block: node,
                offset: 0,
            }))
        })
    }

    fn inline_input_rule(&mut self) -> bool {
        let Some(head) = self.selection.head().filter(|_| self.selection.is_caret()) else {
            return false;
        };
        let Some(loc) = self.locate(head.block) else {
            return false;
        };
        let literal = !loc.is_cell()
            && order::block_at(&self.doc.blocks, loc.block_path())
                .is_none_or(|b| order::is_literal(&b.kind));
        if literal {
            return false;
        }
        let Some(inline) = order::inline_at(&self.doc, &loc) else {
            return false;
        };
        let Some((open, close, delimiter, mark)) = find_pair(inline, head.offset) else {
            return false;
        };
        let node = head.block;
        let width = delimiter.len();
        let inner_end = close - width;
        let applied = self.transact(EditKind::Other, None, "", move |tr, _| {
            tr.replace(node, close..close + width, Inline::default())?;
            tr.replace(node, open..open + width, Inline::default())?;
            if inner_end <= open {
                return Err(Abort::NotApplicable);
            }
            tr.step(Step::AddMark {
                node,
                range: open..inner_end,
                mark,
            })?;
            Ok(Selection::caret(TextPos {
                block: node,
                offset: inner_end,
            }))
        });
        if applied {
            // Text typed next is outside the new mark.
            let mark = INLINE_RULES
                .iter()
                .find(|(d, _)| *d == delimiter)
                .map(|(_, m)| m.clone());
            if let Some(mark) = mark {
                let mut marks = self.caret_marks();
                marks.retain(|m| !inline_ops::same_kind(m, &mark));
                self.pending_marks = Some(marks);
            }
        }
        applied
    }
}

/// Finds a delimiter pair closed at `caret`: `(open, close, delimiter,
/// mark)` with `close` the offset of the closing delimiter.
fn find_pair(inline: &Inline, caret: usize) -> Option<(usize, usize, &'static str, Mark)> {
    let text = inline.text.get(..caret)?;
    for (delimiter, mark) in INLINE_RULES {
        let Some(close) = text.strip_suffix(delimiter).map(str::len) else {
            continue;
        };
        let single_star = *delimiter == "*";
        if single_star && text[..close].ends_with('*') {
            continue;
        }
        let Some(open) = text[..close].rfind(delimiter) else {
            continue;
        };
        let inner = &text[open + delimiter.len()..close];
        if inner.is_empty()
            || inner.starts_with(char::is_whitespace)
            || inner.ends_with(char::is_whitespace)
        {
            continue;
        }
        if single_star && (text[..open].ends_with('*') || inner.starts_with('*')) {
            continue;
        }
        // Delimiters inside code or raw source are literal.
        let literal = inline.marks.iter().any(|span| {
            matches!(span.mark, Mark::Code | Mark::Raw)
                && span.range.start < caret
                && open < span.range.end
        });
        if literal {
            continue;
        }
        return Some((open, close, delimiter, mark.clone()));
    }
    None
}
