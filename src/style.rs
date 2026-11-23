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
    /// Space between a list marker (bullet or number) and the item text.
    pub list_marker_gap: Pixels,

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
    /// Space above headings (not above the first block). Level 3 and deeper
    /// get 70 % of it.
    pub heading_spacing_above: Pixels,
    /// Space below level 2 and deeper headings.
    pub heading_spacing_below: Pixels,
    /// Space below the title.
    pub title_spacing_below: Pixels,

    /// Font family of inline code and code blocks.
    pub code_font_family: SharedString,
    /// Font size of inline code and code blocks.
    pub code_size: Pixels,
    /// Line height of code blocks.
    pub code_line_height: f32,
    /// Background of inline code and code blocks.
    pub code_background: Hsla,
    /// Corner radius of code blocks and inline code.
    pub code_radius: Pixels,

    /// Color of the bar on the left of a quote.
    pub quote_bar_color: Hsla,
    /// Width of the bar on the left of a quote.
    pub quote_bar_width: Pixels,
    /// Space between the quote bar and the quoted text.
    pub quote_indent: Pixels,

    /// Diameter of the round checkbox of a checklist item.
    pub checkbox_size: Pixels,
    /// Outline of an open checkbox.
    pub checkbox_border_color: Hsla,
    /// Fill of a ticked checkbox.
    pub checkbox_checked_color: Hsla,
    /// Color of the tick inside a ticked checkbox.
    pub checkbox_check_color: Hsla,
    /// Space between the checkbox and the item text.
    pub checkbox_gap: Pixels,

    /// Color of the hairlines between table cells.
    pub table_border_color: Hsla,
    /// Background of the header row of a table.
    pub table_header_background: Hsla,
    /// Font size of table cells.
    pub table_text_size: Pixels,
    /// Line height of table cells.
    pub table_line_height: f32,
    /// Space between the hairlines and the text of a table cell.
    pub table_cell_padding: Edges<Pixels>,

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
    /// Width of the text cursor.
    pub caret_width: Pixels,
    /// Color of the ring around a selected image, rule or table.
    pub node_selection_color: Hsla,

    /// Corner radius of images.
    pub image_radius: Pixels,
    /// Height of the box shown while an image is not loaded.
    pub image_placeholder_height: Pixels,

    /// Reading aid in the manner of "bionic reading": the first part of
    /// every word is drawn bold, which gives the eye fixed points to jump
    /// between. Display only: the Markdown does not change. Not applied to
    /// code. Default `false`.
    pub bionic_reading: bool,
}

impl EditorStyle {
    /// Builds the default style from the current gpui-component
    /// [`Theme`](gpui_component::Theme).
    ///
    /// The accent is the theme's `primary` color: it fills ticked checkboxes,
    /// draws the quote bar, tints highlights and colors links, wiki links
    /// and tags.
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
            paragraph_spacing: px(12.),
            list_marker_gap: px(10.),

            title_size: px(28.),
            title_weight: FontWeight::EXTRA_BOLD,
            h2_size: px(19.),
            h2_weight: FontWeight::BOLD,
            h3_size: px(16.),
            h3_weight: FontWeight::BOLD,
            heading_line_height: 1.2,
            heading_spacing_above: px(28.),
            heading_spacing_below: px(8.),
            title_spacing_below: px(10.),

            code_font_family: theme.mono_font_family.clone(),
            code_size: px(14.),
            code_line_height: 1.5,
            code_background: theme.muted,
            code_radius: px(8.),

            quote_bar_color: accent,
            quote_bar_width: px(3.),
            quote_indent: px(14.),

            checkbox_size: px(21.),
            checkbox_border_color: theme.foreground.opacity(0.4),
            checkbox_checked_color: accent,
            checkbox_check_color: white(),
            checkbox_gap: px(12.),

            table_border_color: theme.border,
            table_header_background: theme.muted,
            table_text_size: px(14.5),
            table_line_height: 1.4,
            table_cell_padding: Edges {
                top: px(8.),
                right: px(12.),
                bottom: px(8.),
                left: px(12.),
            },

            link_color: accent,
            tag_color: accent,
            highlight_color: accent.opacity(0.25),
            selection_color: accent.opacity(0.28),
            caret_color: theme.caret,
            caret_width: px(2.),
            node_selection_color: accent,

            image_radius: px(12.),
            image_placeholder_height: px(220.),

            bionic_reading: false,
        }
    }
}
