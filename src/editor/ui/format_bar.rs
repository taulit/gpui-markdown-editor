//! The format bar: the adaptive toolbar floating at the bottom of the
//! editor, as in the Sheaf prototype (plan §10, design: bottom 20 centered,
//! radius 14, padding 5, buttons 38 x 36, hairline separators).
//!
//! What it offers follows the selection:
//!
//! - caret: Aa (styles), checklist, table, image, link
//! - text selected in one block: Aa, bold, italic, underline, strike, code,
//!   highlight, link
//! - text over several blocks: Aa, the marks, bullet list, indent, quote
//! - caret or cells in a table: bold, italic, add row, add column, delete
//!   row, delete column
//!
//! Mark buttons show whether the selection has the mark. The bar is hidden
//! while the editor is not focused, while the mouse selects and in code
//! blocks. "Aa" opens the style popover above the bar.

use gpui::{
    AnyElement, Context, Div, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, SharedString, Stateful, StatefulInteractiveElement as _, Styled as _, Window, div,
    prelude::FluentBuilder as _, px,
};

use super::super::MarkdownEditor;
use super::{Palette, panel, tool_button};
use crate::command::Command;
use crate::edit::Selection;
use crate::model::{BlockKind, Mark};

/// What the bar shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::editor) enum BarMode {
    Caret,
    Text,
    Lines,
    Table,
}

/// A button of the bar.
#[derive(Clone, Debug, PartialEq)]
enum Tool {
    Styles,
    Mark(Mark),
    Command(Command),
    Link,
    Image,
    AddRow,
    AddColumn,
    DeleteRow,
    DeleteColumn,
}

const BUTTON_W: Pixels = px(38.);

type StyleFn = fn(Stateful<Div>) -> Stateful<Div>;
const BUTTON_H: Pixels = px(36.);

impl MarkdownEditor {
    /// The mode of the bar, or `None` when it is hidden.
    pub(in crate::editor) fn format_bar_mode(&self) -> Option<BarMode> {
        if !self.options.ui.format_bar
            || !self.focused
            || self.drag.is_some()
            || self.slash.is_some()
        {
            return None;
        }
        let in_code = self
            .state
            .current_block()
            .and_then(|id| self.state.doc.block(id))
            .is_some_and(|b| matches!(b.kind(), BlockKind::CodeBlock { .. }));
        if in_code {
            return None;
        }
        if self.state.current_cell().is_some()
            || matches!(self.state.selection, Selection::Cells { .. })
        {
            return Some(BarMode::Table);
        }
        match &self.state.selection {
            Selection::Text { anchor, head } if anchor == head => Some(BarMode::Caret),
            Selection::Text { anchor, head } if anchor.block == head.block => Some(BarMode::Text),
            Selection::Text { .. } | Selection::All => Some(BarMode::Lines),
            Selection::Node(_) => Some(BarMode::Caret),
            Selection::Cells { .. } => Some(BarMode::Table),
        }
    }

    fn run_tool(&mut self, tool: &Tool, window: &mut Window, cx: &mut Context<Self>) {
        match tool {
            Tool::Styles => {
                self.style_menu = !self.style_menu;
                cx.notify();
                return;
            }
            Tool::Mark(mark) => self.command(Command::ToggleMark(mark.clone()), window, cx),
            Tool::Command(command) => self.command(command.clone(), window, cx),
            Tool::Link => self.open_link_prompt(window, cx),
            Tool::Image => self.open_image_prompt(window, cx),
            Tool::AddRow | Tool::AddColumn | Tool::DeleteRow | Tool::DeleteColumn => {
                let state = &mut self.state;
                match tool {
                    Tool::AddRow => state.table_insert_row(true),
                    Tool::AddColumn => state.table_insert_column(true),
                    Tool::DeleteRow => state.table_remove_row(),
                    _ => state.table_remove_column(),
                };
                self.after_change(false, window, cx);
            }
        }
        self.style_menu = false;
    }

    /// The tools of `mode`, `None` standing for a separator.
    fn tools(mode: BarMode) -> Vec<Option<Tool>> {
        let marks = [
            Mark::Bold,
            Mark::Italic,
            Mark::Underline,
            Mark::Strike,
            Mark::Code,
            Mark::Highlight,
        ]
        .map(|m| Some(Tool::Mark(m)));
        let set = |kind| Some(Tool::Command(Command::SetBlock(kind)));
        let mut tools = vec![];
        match mode {
            BarMode::Caret => tools.extend([
                Some(Tool::Styles),
                None,
                set(BlockKind::TaskItem { checked: false }),
                Some(Tool::Command(Command::InsertTable { rows: 3, cols: 3 })),
                Some(Tool::Image),
                Some(Tool::Link),
            ]),
            BarMode::Text => {
                tools.extend([Some(Tool::Styles), None]);
                tools.extend(marks);
                tools.extend([None, Some(Tool::Link)]);
            }
            BarMode::Lines => {
                tools.extend([Some(Tool::Styles), None]);
                tools.extend(marks.into_iter().take(4));
                tools.extend([
                    None,
                    set(BlockKind::BulletItem),
                    Some(Tool::Command(Command::Indent)),
                    set(BlockKind::Quote),
                ]);
            }
            BarMode::Table => {
                tools.extend(marks.into_iter().take(2));
                tools.extend([
                    None,
                    Some(Tool::AddRow),
                    Some(Tool::AddColumn),
                    None,
                    Some(Tool::DeleteRow),
                    Some(Tool::DeleteColumn),
                ]);
            }
        }
        tools
    }

    /// The bar and, if open, the style popover above it.
    pub(in crate::editor) fn render_format_bar(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let Some(mode) = self.format_bar_mode() else {
            self.style_menu = false;
            return None;
        };
        let palette = Palette::new(cx);
        let mut buttons: Vec<AnyElement> = Vec::new();
        for tool in Self::tools(mode) {
            let Some(tool) = tool else {
                buttons.push(
                    div()
                        .w(px(1.))
                        .h(BUTTON_H - px(12.))
                        .mx(px(4.))
                        .bg(palette.hair)
                        .into_any_element(),
                );
                continue;
            };
            let active = match &tool {
                Tool::Mark(mark) => self.state.has_mark(mark),
                Tool::Styles => self.style_menu,
                _ => false,
            };
            let (id, label) = tool_name(&tool);
            let wide = matches!(
                tool,
                Tool::AddRow | Tool::AddColumn | Tool::DeleteRow | Tool::DeleteColumn
            );
            let button = tool_button(id, &palette, active, tool_icon(&tool, &palette))
                .min_w(BUTTON_W)
                .when(wide, |b| b.px(px(8.)))
                .h(BUTTON_H)
                .role(gpui::Role::Button)
                .aria_label(label)
                .when(matches!(tool, Tool::Mark(_)), |b| {
                    b.aria_toggled(if active {
                        gpui::Toggled::True
                    } else {
                        gpui::Toggled::False
                    })
                })
                .on_click(cx.listener(move |this, _, window, cx| this.run_tool(&tool, window, cx)));
            buttons.push(button.into_any_element());
        }
        let bar = panel(&palette)
            .id("format-bar")
            .role(gpui::Role::Toolbar)
            .aria_label("Format")
            .occlude()
            .p(px(5.))
            .gap(px(2.))
            .items_center()
            .children(buttons);
        let menu = self
            .style_menu
            .then(|| div().mb(px(4.)).child(self.render_style_menu(&palette, cx)));
        // A column filling the editor, its content at the bottom center:
        // the bar 20 above the edge, the style popover right above it.
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .flex_col()
                .justify_end()
                .items_center()
                .pb(px(20.))
                .children(menu)
                .child(bar)
                .into_any_element(),
        )
    }

    /// The style popover: block styles and the basic marks.
    fn render_style_menu(&self, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let current = self
            .state
            .current_block()
            .and_then(|id| self.state.doc.block(id))
            .map(|b| b.kind().clone());
        let styles: [(&str, &str, BlockKind, StyleFn); 5] = [
            ("style-title", "Title", BlockKind::Heading(1), |d| {
                d.text_size(px(22.)).font_weight(FontWeight::EXTRA_BOLD)
            }),
            ("style-heading", "Heading", BlockKind::Heading(2), |d| {
                d.text_size(px(17.)).font_weight(FontWeight::BOLD)
            }),
            (
                "style-subheading",
                "Subheading",
                BlockKind::Heading(3),
                |d| d.text_size(px(15.)).font_weight(FontWeight::BOLD),
            ),
            ("style-text", "Text", BlockKind::Paragraph, |d| {
                d.text_size(px(14.))
            }),
            (
                "style-mono",
                "Monospace",
                BlockKind::CodeBlock { lang: None },
                |d| d.text_size(px(13.)),
            ),
        ];
        let hover = palette.hover;
        let mono = self.style.code_font_family.clone();
        let rows = styles.into_iter().map(|(id, label, kind, look)| {
            let selected = current.as_ref() == Some(&kind)
                || (kind == BlockKind::Paragraph && current.is_none());
            let row = div()
                .id(SharedString::from(id))
                .debug_selector(move || format!("ui-{id}"))
                .role(gpui::Role::MenuItem)
                .aria_label(label)
                .aria_selected(selected)
                .px(px(12.))
                .py(px(8.))
                .rounded(px(9.))
                .cursor_pointer()
                .when(selected, |s| s.bg(palette.selected()))
                .hover(move |s| s.bg(hover))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.style_menu = false;
                    // Choosing the current style must not toggle it off.
                    let same = this
                        .state
                        .current_block()
                        .and_then(|id| this.state.doc.block(id))
                        .is_some_and(|b| *b.kind() == kind);
                    if !same {
                        this.command(Command::SetBlock(kind.clone()), window, cx);
                    }
                    cx.notify();
                }));
            look(row)
                .when(id == "style-mono", |d| d.font_family(mono.clone()))
                .child(label)
                .into_any_element()
        });
        let marks = [
            (Mark::Bold, "B"),
            (Mark::Italic, "I"),
            (Mark::Underline, "U"),
            (Mark::Strike, "S"),
        ]
        .into_iter()
        .map(|(mark, _)| {
            let tool = Tool::Mark(mark.clone());
            let (id, label) = tool_name(&tool);
            tool_button(
                SharedString::from(format!("style-{id}")),
                palette,
                self.state.has_mark(&mark),
                tool_icon(&tool, palette),
            )
            .flex_1()
            .h(px(32.))
            .role(gpui::Role::Button)
            .aria_label(label)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.command(Command::ToggleMark(mark.clone()), window, cx)
            }))
        });
        panel(palette)
            .id("style-menu")
            .role(gpui::Role::Menu)
            .aria_label("Styles")
            .occlude()
            .flex_col()
            .w(px(250.))
            .p(px(6.))
            .children(rows)
            .child(
                div()
                    .flex()
                    .gap(px(4.))
                    .pt(px(6.))
                    .px(px(6.))
                    .pb(px(2.))
                    .mt(px(4.))
                    .border_t_1()
                    .border_color(palette.hair)
                    .children(marks),
            )
            .into_any_element()
    }
}

/// The stable id and the accessible label of a tool.
fn tool_name(tool: &Tool) -> (&'static str, &'static str) {
    match tool {
        Tool::Styles => ("format-styles", "Styles"),
        Tool::Mark(Mark::Bold) => ("format-bold", "Bold"),
        Tool::Mark(Mark::Italic) => ("format-italic", "Italic"),
        Tool::Mark(Mark::Underline) => ("format-underline", "Underline"),
        Tool::Mark(Mark::Strike) => ("format-strike", "Strikethrough"),
        Tool::Mark(Mark::Code) => ("format-code", "Code"),
        Tool::Mark(_) => ("format-highlight", "Highlight"),
        Tool::Command(Command::SetBlock(BlockKind::TaskItem { .. })) => {
            ("format-checklist", "Checklist")
        }
        Tool::Command(Command::SetBlock(BlockKind::BulletItem)) => ("format-list", "Bullet list"),
        Tool::Command(Command::SetBlock(_)) => ("format-quote", "Quote"),
        Tool::Command(Command::Indent) => ("format-indent", "Indent"),
        Tool::Command(_) => ("format-table", "Insert table"),
        Tool::Link => ("format-link", "Link"),
        Tool::Image => ("format-image", "Insert image"),
        Tool::AddRow => ("format-add-row", "Add row"),
        Tool::AddColumn => ("format-add-column", "Add column"),
        Tool::DeleteRow => ("format-delete-row", "Delete row"),
        Tool::DeleteColumn => ("format-delete-column", "Delete column"),
    }
}

/// A small outlined box, the building block of the drawn icons.
fn outline(w: f32, h: f32, radius: f32, color: gpui::Hsla) -> Div {
    div()
        .w(px(w))
        .h(px(h))
        .rounded(px(radius))
        .border(px(1.5))
        .border_color(color)
}

fn bar(w: f32, color: gpui::Hsla) -> Div {
    div().w(px(w)).h(px(1.5)).rounded(px(1.)).bg(color)
}

/// The icon of a tool: letters for marks, small drawings for the rest
/// (the crate ships no icon font).
fn tool_icon(tool: &Tool, palette: &Palette) -> AnyElement {
    let fg = palette.fg;
    let letter = |text: &'static str, f: fn(Div) -> Div| {
        f(div().text_size(px(16.)).text_color(fg))
            .child(text)
            .into_any_element()
    };
    match tool {
        Tool::Styles => letter("Aa", |d| {
            d.font_weight(FontWeight::EXTRA_BOLD).text_size(px(15.))
        }),
        Tool::Mark(Mark::Bold) => letter("B", |d| d.font_weight(FontWeight::EXTRA_BOLD)),
        Tool::Mark(Mark::Italic) => letter("I", |d| d.font_weight(FontWeight::SEMIBOLD).italic()),
        Tool::Mark(Mark::Underline) => {
            letter("U", |d| d.font_weight(FontWeight::SEMIBOLD).underline())
        }
        Tool::Mark(Mark::Strike) => {
            letter("S", |d| d.font_weight(FontWeight::SEMIBOLD).line_through())
        }
        Tool::Mark(Mark::Code) => letter("</>", |d| d.text_size(px(13.)).font_family("monospace")),
        Tool::Mark(_) => div()
            .px(px(3.))
            .rounded(px(3.))
            .bg(palette.accent.opacity(0.35))
            .text_size(px(15.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(fg)
            .child("H")
            .into_any_element(),
        Tool::Command(Command::SetBlock(BlockKind::TaskItem { .. })) => div()
            .flex()
            .flex_col()
            .gap(px(3.))
            .children((0..2).map(|_| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(3.))
                    .child(outline(6., 6., 3., fg))
                    .child(bar(8., fg))
            }))
            .into_any_element(),
        Tool::Command(Command::SetBlock(BlockKind::BulletItem)) => div()
            .flex()
            .flex_col()
            .gap(px(3.))
            .children((0..3).map(|_| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(3.))
                    .child(div().size(px(3.)).rounded(px(2.)).bg(fg))
                    .child(bar(10., fg))
            }))
            .into_any_element(),
        Tool::Command(Command::SetBlock(_)) => letter("\u{201c}", |d| {
            d.text_size(px(24.))
                .font_weight(FontWeight::BOLD)
                .mt(px(8.))
        }),
        Tool::Command(Command::Indent) => letter("\u{2192}", |d| d.text_size(px(16.))),
        Tool::Command(_) => outline(16., 13., 2.5, fg)
            .relative()
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(3.5))
                    .h(px(1.5))
                    .bg(fg),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(5.))
                    .w(px(1.5))
                    .bg(fg),
            )
            .into_any_element(),
        Tool::Image => outline(16., 13., 2.5, fg)
            .relative()
            .child(
                div()
                    .absolute()
                    .right(px(2.))
                    .top(px(2.))
                    .size(px(3.))
                    .rounded(px(2.))
                    .bg(fg),
            )
            .child(
                div()
                    .absolute()
                    .left(px(2.))
                    .right(px(2.))
                    .bottom(px(2.))
                    .h(px(1.5))
                    .bg(fg),
            )
            .into_any_element(),
        Tool::Link => div()
            .relative()
            .w(px(17.))
            .h(px(8.))
            .child(outline(10., 8., 4., fg).absolute().left_0().top_0())
            .child(outline(10., 8., 4., fg).absolute().left(px(7.)).top_0())
            .into_any_element(),
        Tool::AddRow => letter("+ Row", text_tool),
        Tool::AddColumn => letter("+ Col", text_tool),
        Tool::DeleteRow => letter("\u{2212} Row", text_tool),
        Tool::DeleteColumn => letter("\u{2212} Col", text_tool),
    }
}

fn text_tool(d: Div) -> Div {
    d.text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .whitespace_nowrap()
}
