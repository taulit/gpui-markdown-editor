//! Editing operations on [`Inline`] text and its mark spans.
//!
//! All functions keep the invariants documented on [`Inline`]: spans sorted,
//! never empty, on `char` boundaries, and formatting spans of one kind never
//! overlapping or touching.

use std::mem::discriminant;
use std::ops::Range;

use crate::markdown::normalize_marks;
use crate::model::{Inline, Mark, MarkSpan};

/// Bold, italic, underline, strike, code and highlight: marks that merge
/// with their neighbours and are inherited at their end.
pub(crate) fn is_formatting(mark: &Mark) -> bool {
    matches!(
        mark,
        Mark::Bold | Mark::Italic | Mark::Underline | Mark::Strike | Mark::Code | Mark::Highlight
    )
}

/// `Code` is formatting for merging, but not inherited at its end: typing
/// behind inline code leaves the code span.
fn inclusive_at_end(mark: &Mark) -> bool {
    is_formatting(mark) && !matches!(mark, Mark::Code)
}

/// Two marks of the same kind (links with different URLs are the same kind).
pub(crate) fn same_kind(a: &Mark, b: &Mark) -> bool {
    match (a, b) {
        (Mark::Custom { id: a, .. }, Mark::Custom { id: b, .. }) => a == b,
        _ => discriminant(a) == discriminant(b),
    }
}

/// Clamps `offset` to the text and moves it back to a `char` boundary.
pub(crate) fn floor_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn normalize(marks: &mut Vec<MarkSpan>) {
    marks.retain(|span| !span.range.is_empty());
    // Equal links (and other unit marks) that overlap are one span.
    let mut index = 0;
    while index < marks.len() {
        if !is_formatting(&marks[index].mark) {
            let mut other = index + 1;
            while other < marks.len() {
                let overlaps = marks[other].range.start < marks[index].range.end
                    && marks[index].range.start < marks[other].range.end;
                if overlaps && marks[other].mark == marks[index].mark {
                    let removed = marks.remove(other);
                    let span = &mut marks[index].range;
                    span.start = span.start.min(removed.range.start);
                    span.end = span.end.max(removed.range.end);
                } else {
                    other += 1;
                }
            }
        }
        index += 1;
    }
    marks.sort_by_key(|span| span.range.start);
    normalize_marks(marks);
}

/// Replaces `range` with `insert`.
///
/// Spans that strictly contain the range grow over the inserted text, spans
/// cut by the range shrink, spans inside it disappear. The spans of `insert`
/// are added on top and merged with equal neighbours.
pub(crate) fn replace(inline: &mut Inline, range: Range<usize>, insert: &Inline) {
    let Range { start, end } = range;
    let added = insert.text.len();
    let removed = end - start;
    let shift = |offset: usize| offset - removed + added;
    inline.text.replace_range(start..end, &insert.text);
    let mut marks = Vec::with_capacity(inline.marks.len() + insert.marks.len());
    for span in inline.marks.drain(..) {
        let Range { start: a, end: b } = span.range;
        let range = if b <= start {
            a..b
        } else if a >= end {
            shift(a)..shift(b)
        } else if a < start && b > end {
            a..shift(b)
        } else if a < start {
            a..start
        } else if b > end {
            start + added..shift(b)
        } else {
            continue;
        };
        marks.push(MarkSpan {
            range,
            mark: span.mark,
        });
    }
    for span in &insert.marks {
        marks.push(MarkSpan {
            range: span.range.start + start..span.range.end + start,
            mark: span.mark.clone(),
        });
    }
    normalize(&mut marks);
    inline.marks = marks;
}

/// The part of `inline` in `range`, with the spans clipped to it.
pub(crate) fn slice(inline: &Inline, range: Range<usize>) -> Inline {
    let text = inline.text[range.clone()].to_string();
    let marks = inline
        .marks
        .iter()
        .filter_map(|span| {
            let start = span.range.start.max(range.start);
            let end = span.range.end.min(range.end);
            (start < end).then(|| MarkSpan {
                range: start - range.start..end - range.start,
                mark: span.mark.clone(),
            })
        })
        .collect();
    Inline { text, marks }
}

/// Appends `other` to `inline`.
pub(crate) fn concat(inline: &Inline, other: &Inline) -> Inline {
    let mut out = inline.clone();
    let end = out.text.len();
    replace(&mut out, end..end, other);
    out
}

/// Removes the marks of `kind` from `range`.
pub(crate) fn remove_mark(inline: &mut Inline, range: Range<usize>, kind: &Mark) {
    let mut marks = Vec::with_capacity(inline.marks.len() + 1);
    for span in inline.marks.drain(..) {
        let cut = same_kind(&span.mark, kind)
            && span.range.start < range.end
            && range.start < span.range.end;
        if !cut {
            marks.push(span);
            continue;
        }
        if span.range.start < range.start {
            marks.push(MarkSpan {
                range: span.range.start..range.start,
                mark: span.mark.clone(),
            });
        }
        if span.range.end > range.end {
            marks.push(MarkSpan {
                range: range.end..span.range.end,
                mark: span.mark,
            });
        }
    }
    normalize(&mut marks);
    inline.marks = marks;
}

/// Adds `mark` to `range`. A link replaces the links it overlaps.
pub(crate) fn add_mark(inline: &mut Inline, range: Range<usize>, mark: Mark) {
    if range.is_empty() {
        return;
    }
    if !is_formatting(&mark) {
        remove_mark(inline, range.clone(), &mark);
    }
    inline.marks.push(MarkSpan { range, mark });
    normalize(&mut inline.marks);
}

/// `true` if every character in `range` has a mark of `kind`.
pub(crate) fn covered(inline: &Inline, range: Range<usize>, kind: &Mark) -> bool {
    let mut at = range.start;
    let mut spans: Vec<&MarkSpan> = inline
        .marks
        .iter()
        .filter(|span| same_kind(&span.mark, kind))
        .collect();
    spans.sort_by_key(|span| span.range.start);
    for span in spans {
        if span.range.start <= at && span.range.end > at {
            at = span.range.end;
        }
        if at >= range.end {
            return true;
        }
    }
    at >= range.end
}

/// The span of `kind` around `offset`, if any (start and end included).
pub(crate) fn span_at<'a>(inline: &'a Inline, offset: usize, kind: &Mark) -> Option<&'a MarkSpan> {
    inline
        .marks
        .iter()
        .filter(|span| same_kind(&span.mark, kind))
        .find(|span| span.range.start < offset && offset < span.range.end)
        .or_else(|| {
            inline.marks.iter().find(|span| {
                same_kind(&span.mark, kind)
                    && span.range.start <= offset
                    && offset <= span.range.end
            })
        })
}

/// The marks text typed at `offset` gets.
///
/// Inside a span the text joins it. At the end of a span only formatting
/// marks carry over; links, code, raw and custom marks end there. At the very
/// start of the text the formatting of the first character carries over.
pub(crate) fn marks_at(inline: &Inline, offset: usize) -> Vec<Mark> {
    let mut out: Vec<Mark> = Vec::new();
    for span in &inline.marks {
        let Range { start, end } = span.range;
        let inside = start < offset && offset < end;
        let at_end = end == offset && inclusive_at_end(&span.mark);
        let at_start = offset == 0 && start == 0 && inclusive_at_end(&span.mark);
        if (inside || at_end || at_start) && !out.contains(&span.mark) {
            out.push(span.mark.clone());
        }
    }
    out
}

/// `text` with the formatting marks of `marks` over all of it. Unit marks
/// such as links are left out: they only cover typed text that lands inside
/// them.
pub(crate) fn typed(text: &str, marks: &[Mark]) -> Inline {
    Inline {
        text: text.to_string(),
        marks: marks
            .iter()
            .filter(|mark| is_formatting(mark))
            .map(|mark| MarkSpan {
                range: 0..text.len(),
                mark: mark.clone(),
            })
            .collect(),
    }
}

/// Spans that strictly contain `offset` and thus grow over text inserted
/// there.
pub(crate) fn containing(inline: &Inline, offset: usize) -> Vec<Mark> {
    inline
        .marks
        .iter()
        .filter(|span| span.range.start < offset && offset < span.range.end)
        .map(|span| span.mark.clone())
        .collect()
}

/// Checks the invariants of [`Inline`]. Used by tests and debug assertions.
pub(crate) fn check(inline: &Inline) -> Result<(), String> {
    let text = &inline.text;
    let mut previous: Option<&MarkSpan> = None;
    for span in &inline.marks {
        let Range { start, end } = span.range;
        if start >= end {
            return Err(format!("empty span {span:?}"));
        }
        if end > text.len() || !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            return Err(format!("span {span:?} outside text or off boundary: {text:?}"));
        }
        if let Some(previous) = previous
            && previous.range.start > start
        {
            return Err(format!("spans not sorted: {:?}", inline.marks));
        }
        previous = Some(span);
    }
    for (index, a) in inline.marks.iter().enumerate() {
        for b in &inline.marks[index + 1..] {
            if !same_kind(&a.mark, &b.mark) {
                continue;
            }
            let formatting = is_formatting(&a.mark);
            let touches = a.range.start <= b.range.end && b.range.start <= a.range.end;
            let overlaps = a.range.start < b.range.end && b.range.start < a.range.end;
            if (formatting && touches) || (matches!(a.mark, Mark::Link { .. }) && overlaps) {
                return Err(format!("spans {a:?} and {b:?} overlap or touch"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bold(range: Range<usize>) -> MarkSpan {
        MarkSpan {
            range,
            mark: Mark::Bold,
        }
    }

    fn link(range: Range<usize>) -> MarkSpan {
        MarkSpan {
            range,
            mark: Mark::Link {
                url: "u".into(),
                title: None,
            },
        }
    }

    #[test]
    fn insertion_inside_a_span_extends_it() {
        let mut inline = Inline {
            text: "abcd".into(),
            marks: vec![bold(1..3)],
        };
        replace(&mut inline, 2..2, &Inline::plain("XY"));
        assert_eq!(inline.text, "abXYcd");
        assert_eq!(inline.marks, vec![bold(1..5)]);
        replace(&mut inline, 5..5, &Inline::plain("Z"));
        assert_eq!(inline.marks, vec![bold(1..5)]);
    }

    #[test]
    fn deleting_across_spans_shrinks_them() {
        let mut inline = Inline {
            text: "aaaabbbbcc".into(),
            marks: vec![bold(0..4), link(6..10)],
        };
        replace(&mut inline, 2..8, &Inline::default());
        assert_eq!(inline.text, "aacc");
        assert_eq!(inline.marks, vec![bold(0..2), link(2..4)]);
    }

    #[test]
    fn adding_and_removing_marks() {
        let mut inline = Inline::plain("hello world");
        add_mark(&mut inline, 0..5, Mark::Bold);
        add_mark(&mut inline, 5..8, Mark::Bold);
        assert_eq!(inline.marks, vec![bold(0..8)]);
        assert!(covered(&inline, 2..8, &Mark::Bold));
        assert!(!covered(&inline, 2..9, &Mark::Bold));
        remove_mark(&mut inline, 2..4, &Mark::Bold);
        assert_eq!(inline.marks, vec![bold(0..2), bold(4..8)]);
        check(&inline).unwrap();
    }

    #[test]
    fn typing_inherits_formatting_but_not_links() {
        let inline = Inline {
            text: "abcdef".into(),
            marks: vec![bold(0..3), link(3..6)],
        };
        assert_eq!(marks_at(&inline, 3), vec![Mark::Bold]);
        assert_eq!(marks_at(&inline, 6), Vec::<Mark>::new());
        assert_eq!(marks_at(&inline, 0), vec![Mark::Bold]);
        assert_eq!(marks_at(&inline, 4), vec![link(0..1).mark]);
    }
}
