//! The visual style of the editor.

use gpui::{App, Edges, FontWeight, Hsla, Pixels, SharedString, px, white};
use gpui_component::ActiveTheme as _;

/// Fonts, sizes and colors the editor draws with.
///
/// A plain value: build one with [`EditorStyle::from_theme`], change the
/// fields you care about and pass it to
/// [`MarkdownEditor::set_style`](crate::MarkdownEditor::set_style). The
/// editor does not follow theme changes on its own. Build a new style and set
/// it again when the theme changes.
///
/// ```no_run
/// # use gpui::{App, Entity, px};
/// # use gpui_markdown_editor::{EditorStyle, MarkdownEditor};
/// # fn restyle(editor: &Entity<MarkdownEditor>, cx: &mut App) {
/// let mut style = EditorStyle::from_theme(cx);
/// style.text_size = px(17.);
/// style.link_color = gpui::blue();
/// editor.update(cx, |editor, cx| editor.set_style(style, cx));
/// # }
/// ```
///
/// Sizes are in logical pixels. Line heights are multiples of the font size.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct EditorStyle {
    /// Font family of all text except code.
    pub font_family: SharedString,
    /// Font size of paragraphs, list items and quotes.
    pub text_size: Pixels,
    /// Line height of body text.
    pub line_height: f32,
    /// Color of body text and headings.
    pub text_color: Hsla,
    /// Color of secondary content such as list bullets and the text of quotes.
    pub muted_color: Hsla,
    /// Color of the quietest content: placeholders, the outline of an open
    /// checkbox, the text of ticked checklist items.
    pub faint_color: Hsla,
    /// Space around the content column. The bottom edge is the room left
    /// below the last block when scrolled to the end.
    pub padding: Edges<Pixels>,
    /// Space between two blocks.
    pub paragraph_spacing: Pixels,

    /// Font size of the title: level 1 headings, and the first block with
    /// [`first_line_is_title`](crate::EditorOptions::first_line_is_title).
    pub title_size: Pixels,
    /// Font weight of the title.
    pub title_weight: FontWeight,
    /// Font size of level 2 headings.
    pub h2_size: Pixels,
    /// Font weight of level 2 headings.
    pub h2_weight: FontWeight,
    /// Font size of level 3 headings and deeper.
    pub h3_size: Pixels,
    /// Font weight of level 3 headings and deeper.
    pub h3_weight: FontWeight,
    /// Line height of the title and of headings.
    pub heading_line_height: f32,

    /// Font family of inline code and code blocks.
    pub code_font_family: SharedString,
    /// Font size of inline code and code blocks.
    pub code_size: Pixels,
    /// Background of inline code and code blocks.
    pub code_background: Hsla,

    /// Color of the bar on the left of a quote.
    pub quote_bar_color: Hsla,

    /// Diameter of the round checkbox of a checklist item.
    pub checkbox_size: Pixels,
    /// Outline of an open checkbox.
    pub checkbox_border_color: Hsla,
    /// Fill of a ticked checkbox.
    pub checkbox_checked_color: Hsla,
    /// Color of the tick inside a ticked checkbox.
    pub checkbox_check_color: Hsla,

    /// Color of the hairlines between table cells.
    pub table_border_color: Hsla,
    /// Background of the header row of a table.
    pub table_header_background: Hsla,

    /// Color of links and wiki links.
    pub link_color: Hsla,
    /// Color of tags.
    pub tag_color: Hsla,
    /// Background of highlighted text (`==text==`) and of the ranges set with
    /// [`MarkdownEditor::highlight`](crate::MarkdownEditor::highlight).
    pub highlight_color: Hsla,
    /// Background of selected text.
    pub selection_color: Hsla,
    /// Color of the text cursor.
    pub caret_color: Hsla,

    /// Corner radius of images.
    pub image_radius: Pixels,
}

impl EditorStyle {
    /// Builds the default style from the current gpui-component
    /// [`Theme`](gpui_component::Theme).
    ///
    /// The accent is the theme's `primary` color: it fills ticked checkboxes,
    /// draws the quote bar and tints highlights. Links and tags use the
    /// theme's `link` color.
    pub fn from_theme(cx: &App) -> Self {
        let theme = cx.theme();
        let accent = theme.primary;
        Self {
            font_family: theme.font_family.clone(),
            text_size: px(15.5),
            line_height: 1.6,
            text_color: theme.foreground,
            muted_color: theme.muted_foreground,
            faint_color: theme.foreground.opacity(0.4),
            padding: Edges {
                top: px(18.),
                right: px(48.),
                bottom: px(120.),
                left: px(48.),
            },
            paragraph_spacing: px(6.),

            title_size: px(28.),
            title_weight: FontWeight::EXTRA_BOLD,
            h2_size: px(19.),
            h2_weight: FontWeight::BOLD,
            h3_size: px(16.),
            h3_weight: FontWeight::BOLD,
            heading_line_height: 1.2,

            code_font_family: theme.mono_font_family.clone(),
            code_size: px(14.),
            code_background: theme.muted,

            quote_bar_color: accent,

            checkbox_size: px(21.),
            checkbox_border_color: theme.foreground.opacity(0.4),
            checkbox_checked_color: accent,
            checkbox_check_color: white(),

            table_border_color: theme.border,
            table_header_background: theme.muted,

            link_color: theme.link,
            tag_color: theme.link,
            highlight_color: accent.opacity(0.25),
            selection_color: theme.selection,
            caret_color: theme.caret,

            image_radius: px(12.),
        }
    }
}
