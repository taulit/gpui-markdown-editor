//! Shaped text of one block or cell: runs from marks, wrapped visual lines
//! and the mapping between byte offsets and positions.

use std::hash::{Hash, Hasher};
use std::ops::Range;

use gpui::{
    App, Bounds, Font, FontStyle, FontWeight, Hsla, Pixels, Point, SharedString,
    StrikethroughStyle, TextAlign, TextRun, UnderlineStyle, Window, WindowTextSystem, WrappedLine,
    font, point, px, size,
};

use crate::extension::MarkColor;
use crate::model::{Inline, Mark, MarkSpan};
use crate::style::EditorStyle;

/// The base look of a piece of text before marks are applied.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextSpec {
    pub family: SharedString,
    pub size: Pixels,
    pub line_height: Pixels,
    pub weight: FontWeight,
    pub italic: bool,
    pub color: Hsla,
    /// Strikes the whole text through (ticked checklist items).
    pub strike: bool,
    /// Ignores marks (code blocks, raw source).
    pub plain: bool,
    /// Tabular figures (OpenType `tnum`), for table text.
    pub tabular: bool,
}

impl TextSpec {
    pub fn hash_into(&self, h: &mut impl Hasher) {
        self.family.hash(h);
        self.size.as_f32().to_bits().hash(h);
        self.line_height.as_f32().to_bits().hash(h);
        self.weight.0.to_bits().hash(h);
        self.italic.hash(h);
        hash_color(self.color, h);
        self.strike.hash(h);
        self.plain.hash(h);
        self.tabular.hash(h);
    }

    fn font(&self) -> Font {
        let mut f = font(self.family.clone());
        f.weight = self.weight;
        if self.italic {
            f.style = FontStyle::Italic;
        }
        if self.tabular {
            f.features = gpui::FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)]));
        }
        f
    }
}

pub(crate) fn hash_color(c: Hsla, h: &mut impl Hasher) {
    for v in [c.h, c.s, c.l, c.a] {
        v.to_bits().hash(h);
    }
}

/// Hashes what decides the shape of an inline text.
pub(crate) fn hash_inline(inline: &Inline, h: &mut impl Hasher) {
    inline.text.hash(h);
    for span in &inline.marks {
        span.range.hash(h);
        std::mem::discriminant(&span.mark).hash(h);
        if let Mark::Custom { id, .. } = &span.mark {
            id.0.hash(h);
        }
    }
}

/// One visual (wrapped) line of a [`TextLayout`].
#[derive(Clone, Debug)]
pub(crate) struct VisualLine {
    /// Index of the hard line (`\n`-separated segment) this line belongs to.
    pub segment: usize,
    /// Byte range in the whole text.
    pub start: usize,
    pub end: usize,
    /// x of `start` in the unwrapped segment layout.
    pub start_x: Pixels,
    pub width: Pixels,
}

struct Segment {
    start: usize,
    line: WrappedLine,
}

/// A shaped and wrapped text.
pub(crate) struct TextLayout {
    pub text: SharedString,
    pub line_height: Pixels,
    pub font_size: Pixels,
    segments: Vec<Segment>,
    pub lines: Vec<VisualLine>,
    /// The widest visual line.
    pub width: Pixels,
}

impl std::fmt::Debug for TextLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextLayout")
            .field("text", &self.text)
            .field("lines", &self.lines.len())
            .finish_non_exhaustive()
    }
}

impl TextLayout {
    /// Shapes `text` with `runs`, wrapping at `wrap_width` (no wrapping for
    /// `None`).
    pub fn shape(
        ts: &WindowTextSystem,
        text: &str,
        runs: &[TextRun],
        spec: &TextSpec,
        wrap_width: Option<Pixels>,
    ) -> Self {
        let shared = SharedString::new(text);
        let wrap_width = wrap_width.map(|w| w.max(px(1.)));
        let shaped = ts
            .shape_text(shared.clone(), spec.size, runs, wrap_width, None)
            .unwrap_or_default();

        let mut segments = Vec::with_capacity(shaped.len());
        let mut start = 0;
        for (line, piece) in shaped.into_iter().zip(text.split('\n')) {
            segments.push(Segment { start, line });
            start += piece.len() + 1;
        }

        let mut lines = Vec::new();
        let mut width = Pixels::ZERO;
        for (ix, seg) in segments.iter().enumerate() {
            let layout = &seg.line.unwrapped_layout;
            let mut prev = (0usize, Pixels::ZERO);
            let boundaries = seg.line.wrap_boundaries.iter().filter_map(|b| {
                let glyph = layout.runs.get(b.run_ix)?.glyphs.get(b.glyph_ix)?;
                Some((glyph.index, glyph.position.x))
            });
            for end in boundaries.chain([(layout.len, layout.width)]) {
                let line_width = (end.1 - prev.1).max(Pixels::ZERO);
                width = width.max(line_width);
                lines.push(VisualLine {
                    segment: ix,
                    start: seg.start + prev.0,
                    end: seg.start + end.0.max(prev.0),
                    start_x: prev.1,
                    width: line_width,
                });
                prev = end;
            }
        }
        if lines.is_empty() {
            lines.push(VisualLine {
                segment: 0,
                start: 0,
                end: 0,
                start_x: Pixels::ZERO,
                width: Pixels::ZERO,
            });
        }

        Self {
            text: shared,
            line_height: spec.line_height,
            font_size: spec.size,
            segments,
            lines,
            width,
        }
    }

    pub fn height(&self) -> Pixels {
        self.line_height * self.lines.len() as f32
    }

    pub fn len(&self) -> usize {
        self.text.len()
    }

    /// The x of `offset` relative to the start of the visual line `line`.
    pub fn x_in_line(&self, line: usize, offset: usize) -> Pixels {
        let Some(l) = self.lines.get(line) else {
            return Pixels::ZERO;
        };
        let Some(seg) = self.segments.get(l.segment) else {
            return Pixels::ZERO;
        };
        let local = offset.clamp(l.start, l.end) - seg.start;
        seg.line.unwrapped_layout.x_for_index(local) - l.start_x
    }

    /// The visual line that shows `offset`. At a soft wrap the offset belongs
    /// to the following line.
    pub fn line_for_offset(&self, offset: usize) -> usize {
        let ix = self.lines.partition_point(|l| l.end < offset);
        if ix >= self.lines.len() {
            return self.lines.len() - 1;
        }
        let l = &self.lines[ix];
        if l.end == offset
            && let Some(next) = self.lines.get(ix + 1)
            && next.segment == l.segment
        {
            return ix + 1;
        }
        ix
    }

    /// The closest offset to `x` (relative to the line start) on `line`.
    pub fn offset_in_line(&self, line: usize, x: Pixels) -> usize {
        let line = line.min(self.lines.len() - 1);
        let l = &self.lines[line];
        let Some(seg) = self.segments.get(l.segment) else {
            return 0;
        };
        let layout = &seg.line.unwrapped_layout;
        let lx = x + l.start_x;
        let mut ix = layout.closest_index_for_x(lx);
        // gpui answers the end for any x past the start of the last glyph;
        // the left half of that glyph belongs before it.
        if ix == layout.len && ix > 0 {
            let seg_text = self
                .text
                .get(seg.start..seg.start + layout.len)
                .unwrap_or("");
            let last = prev_boundary(seg_text, layout.len);
            let mid = (layout.x_for_index(last) + layout.width) / 2.;
            if lx < mid {
                ix = last;
            }
        }
        let mut offset = (seg.start + ix).clamp(l.start, l.end);
        let wrapped = self
            .lines
            .get(line + 1)
            .is_some_and(|next| next.segment == l.segment);
        if wrapped && offset == l.end && l.end > l.start {
            // Past the end of a soft-wrapped line: stay on this line, before
            // the space the line was broken at.
            offset = prev_boundary(&self.text, l.end);
        }
        floor_boundary(&self.text, offset)
    }

    /// The offset of the character under `x` on `line`, `None` past its end.
    pub fn char_at(&self, line: usize, x: Pixels) -> Option<usize> {
        let l = self.lines.get(line)?;
        if x < Pixels::ZERO || x >= l.width {
            return None;
        }
        let seg = self.segments.get(l.segment)?;
        let ix = seg.line.unwrapped_layout.index_for_x(x + l.start_x)?;
        Some((seg.start + ix).clamp(l.start, l.end))
    }

    /// One rectangle per visual line covered by `range`, relative to the
    /// text origin, without alignment offsets (see `line_offsets`).
    pub fn range_rects(&self, range: Range<usize>, offsets: &[Pixels]) -> Vec<Bounds<Pixels>> {
        let mut rects = Vec::new();
        for (ix, l) in self.lines.iter().enumerate() {
            if l.end < range.start || l.start > range.end {
                continue;
            }
            if l.end == range.start && range.start != range.end && l.end != l.start {
                continue;
            }
            let a = range.start.max(l.start);
            let b = range.end.min(l.end);
            let x0 = self.x_in_line(ix, a);
            let x1 = self.x_in_line(ix, b);
            let dx = offsets.get(ix).copied().unwrap_or_default();
            rects.push(Bounds::new(
                point(dx + x0, self.line_height * ix as f32),
                size((x1 - x0).max(Pixels::ZERO), self.line_height),
            ));
        }
        rects
    }

    /// Alignment offset of every visual line for `align` in `width`.
    pub fn line_offsets(&self, align: TextAlign, width: Pixels) -> Vec<Pixels> {
        if align == TextAlign::Left {
            return Vec::new();
        }
        self.lines
            .iter()
            .map(|l| match align {
                TextAlign::Left => Pixels::ZERO,
                TextAlign::Center => ((width - l.width) / 2.).max(Pixels::ZERO),
                TextAlign::Right => (width - l.width).max(Pixels::ZERO),
            })
            .collect()
    }

    pub fn paint(
        &self,
        origin: Point<Pixels>,
        align: TextAlign,
        align_width: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) {
        let mut y = origin.y;
        for seg in &self.segments {
            let rows = seg.line.wrap_boundaries.len() + 1;
            let bounds = Bounds::new(
                point(origin.x, y),
                size(align_width, self.line_height * rows as f32),
            );
            let _ = seg.line.paint(
                bounds.origin,
                self.line_height,
                align,
                Some(bounds),
                window,
                cx,
            );
            y += self.line_height * rows as f32;
        }
    }
}

/// Text runs for `inline` and the ranges that get a painted background
/// (inline code, highlights).
pub(crate) fn build_runs(
    inline: &Inline,
    spec: &TextSpec,
    style: &EditorStyle,
    extensions: &crate::extension::Extensions,
) -> (Vec<TextRun>, Vec<(Range<usize>, Hsla)>) {
    let text = &inline.text;
    let base = TextRun {
        len: text.len(),
        font: spec.font(),
        color: spec.color,
        background_color: None,
        underline: None,
        strikethrough: spec.strike.then_some(StrikethroughStyle {
            thickness: px(1.),
            color: None,
        }),
    };
    if spec.plain || inline.marks.is_empty() || text.is_empty() {
        return (vec![base], Vec::new());
    }

    let marks: Vec<&MarkSpan> = inline
        .marks
        .iter()
        .filter(|m| {
            m.range.start < m.range.end
                && m.range.end <= text.len()
                && text.is_char_boundary(m.range.start)
                && text.is_char_boundary(m.range.end)
        })
        .collect();
    let mut cuts: Vec<usize> = Vec::with_capacity(marks.len() * 2 + 2);
    cuts.push(0);
    cuts.push(text.len());
    for m in &marks {
        cuts.push(m.range.start);
        cuts.push(m.range.end);
    }
    cuts.sort_unstable();
    cuts.dedup();

    let mut runs: Vec<TextRun> = Vec::with_capacity(cuts.len());
    let mut backgrounds: Vec<(Range<usize>, Hsla)> = Vec::new();
    for w in cuts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let mut run = base.clone();
        run.len = b - a;
        let mut weight = spec.weight;
        let mut background = None;
        for m in marks
            .iter()
            .filter(|m| m.range.start <= a && m.range.end >= b)
        {
            match &m.mark {
                Mark::Bold => weight = weight.max_weight(FontWeight::BOLD),
                Mark::Italic => run.font.style = FontStyle::Italic,
                Mark::Underline => {
                    run.underline.get_or_insert(UnderlineStyle {
                        thickness: px(1.),
                        color: None,
                        wavy: false,
                    });
                }
                Mark::Strike => {
                    run.strikethrough = Some(StrikethroughStyle {
                        thickness: px(1.),
                        color: None,
                    })
                }
                Mark::Code => {
                    run.font.family = style.code_font_family.clone();
                    background = Some(style.code_background);
                }
                Mark::Highlight => background = Some(style.highlight_color),
                Mark::Link { .. } => link_look(&mut run, style),
                Mark::Raw => {
                    run.font.family = style.code_font_family.clone();
                    run.color = style.muted_color;
                }
                Mark::Custom { id, .. } => {
                    let Some(look) = extensions.iter().find_map(|e| e.mark_style(*id)) else {
                        continue;
                    };
                    let color = |c: MarkColor| match c {
                        MarkColor::Link => style.link_color,
                        MarkColor::Tag => style.tag_color,
                        MarkColor::Highlight => style.highlight_color,
                        MarkColor::Muted => style.muted_color,
                        MarkColor::Fixed(c) => c,
                        #[allow(unreachable_patterns)]
                        _ => style.text_color,
                    };
                    let fg = look.color.map(color);
                    if let Some(fg) = fg
                        && !spec.strike
                    {
                        run.color = fg;
                    }
                    if let Some(bg) = look.background {
                        background = Some(color(bg));
                    }
                    if let Some(w) = look.weight {
                        weight = weight.max_weight(w);
                    }
                    if look.italic {
                        run.font.style = FontStyle::Italic;
                    }
                    if look.underline {
                        run.underline = Some(UnderlineStyle {
                            thickness: px(1.),
                            color: Some(fg.unwrap_or(run.color).opacity(0.4)),
                            wavy: false,
                        });
                    }
                }
            }
        }
        run.font.weight = weight;
        if let Some(color) = background {
            match backgrounds.last_mut() {
                Some((range, c)) if range.end == a && *c == color => range.end = b,
                _ => backgrounds.push((a..b, color)),
            }
        }
        match runs.last_mut() {
            Some(last) if same_look(last, &run) => last.len += run.len,
            _ => runs.push(run),
        }
    }
    (runs, backgrounds)
}

fn link_look(run: &mut TextRun, style: &EditorStyle) {
    if run.strikethrough.is_none() {
        run.color = style.link_color;
    }
    run.underline = Some(UnderlineStyle {
        thickness: px(1.),
        color: Some(style.link_color.opacity(0.4)),
        wavy: false,
    });
}

fn same_look(a: &TextRun, b: &TextRun) -> bool {
    a.font == b.font
        && a.color == b.color
        && a.underline == b.underline
        && a.strikethrough == b.strikethrough
        && a.background_color == b.background_color
}

trait MaxWeight {
    fn max_weight(self, other: FontWeight) -> FontWeight;
}

impl MaxWeight for FontWeight {
    fn max_weight(self, other: FontWeight) -> FontWeight {
        if other.0 > self.0 { other } else { self }
    }
}

/// The largest char boundary `<= offset` (clamped to the text).
pub(crate) fn floor_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

/// The char boundary before `offset`.
pub(crate) fn prev_boundary(text: &str, offset: usize) -> usize {
    let offset = floor_boundary(text, offset);
    text[..offset]
        .char_indices()
        .next_back()
        .map_or(0, |(ix, _)| ix)
}

#[cfg(test)]
mod spec_tests {
    use super::*;

    #[test]
    fn tabular_specs_enable_tnum() {
        let spec = TextSpec {
            family: "Inter".into(),
            size: gpui::px(14.),
            line_height: gpui::px(20.),
            weight: gpui::FontWeight::NORMAL,
            italic: false,
            color: gpui::black(),
            strike: false,
            plain: false,
            tabular: true,
        };
        assert_eq!(
            spec.font().features.tag_value_list(),
            [("tnum".to_string(), 1)]
        );
        let plain = TextSpec {
            tabular: false,
            ..spec
        };
        assert!(plain.font().features.tag_value_list().is_empty());
    }
}
