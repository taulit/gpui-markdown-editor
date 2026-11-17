//! The editor entity.

use std::fmt;
use std::ops::Range;

use gpui::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent,
    ParentElement as _, Render, SharedString, Styled as _, Subscription, Window, div,
    prelude::FluentBuilder as _, relative,
};
use gpui_component::input::{InputEvent, Redo, Textarea, TextareaState, Undo};
use unicode_segmentation::UnicodeSegmentation as _;

use crate::command::Command;
use crate::edit::Selection;
use crate::event::EditorEvent;
use crate::extension::Extensions;
use crate::meta::DocMeta;
use crate::model::{Block, BlockId, BlockKind, Document, TextPos, TextRange};
use crate::options::EditorOptions;
use crate::style::EditorStyle;
use crate::view::{DocumentElement, DocumentView, LayoutInput, ViewInput, ViewSource};

/// A Markdown editor view.
///
/// Create it as a GPUI entity, put the entity into your element tree and
/// subscribe to its [`EditorEvent`]s:
///
/// ```no_run
/// use gpui::{AppContext as _, Context, Entity, Window};
/// use gpui_markdown_editor::{EditorEvent, EditorOptions, MarkdownEditor};
///
/// struct NoteView {
///     editor: Entity<MarkdownEditor>,
/// }
///
/// impl NoteView {
///     fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
///         let editor = cx.new(|cx| MarkdownEditor::new(window, cx, EditorOptions::default()));
///         editor.update(cx, |editor, cx| editor.set_markdown("# Hello\n\nWorld\n", cx));
///
///         cx.subscribe(&editor, |_, editor, event: &EditorEvent, cx| {
///             if let EditorEvent::Changed = event {
///                 let markdown = editor.read(cx).markdown();
///                 // Write `markdown` to disk (debounced), then:
///                 editor.update(cx, |editor, _| editor.mark_saved());
///             }
///         })
///         .detach();
///
///         Self { editor }
///     }
/// }
/// ```
///
/// The editor fills the space its parent gives it and scrolls its content
/// itself. gpui-component must be initialized (`gpui_component::init`) before
/// the first editor is created.
pub struct MarkdownEditor {
    input: Entity<TextareaState>,
    /// The options without [`EditorOptions::extensions`], which live on in
    /// `extensions`.
    options: EditorOptions,
    extensions: Extensions,
    /// Always parsed from the text the user sees.
    document: Document,
    /// Text set with `set_markdown` that the input has not received yet,
    /// because updating the input needs a window.
    pending_text: Option<SharedString>,
    dirty: bool,
    selection: Range<usize>,
    style: EditorStyle,
    meta: DocMeta,
    highlights: Vec<TextRange>,
    scroll_target: Option<TextRange>,
    /// Layout and scroll state of the rendered document.
    view: DocumentView,
    /// Temporary: render the document instead of the source, see
    /// [`MarkdownEditor::__set_rich_preview`].
    rich: bool,
    rich_selection: Option<Selection>,
    _subscriptions: [Subscription; 2],
}

impl EventEmitter<EditorEvent> for MarkdownEditor {}

impl MarkdownEditor {
    /// Creates an empty editor.
    ///
    /// The style starts as [`EditorStyle::from_theme`].
    pub fn new(window: &mut Window, cx: &mut Context<Self>, mut options: EditorOptions) -> Self {
        let extensions = Extensions::new(std::mem::take(&mut options.extensions));
        let find_bar = cfg!(feature = "ui") && options.ui.find_bar;
        let input = cx.new(|cx| TextareaState::new(window, cx).searchable(find_bar));
        let subscriptions = [
            cx.subscribe(&input, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.sync_document(cx);
                }
            }),
            cx.observe(&input, |this, _, cx| this.sync_selection(cx)),
        ];

        Self {
            input,
            options,
            document: Document::parse_with("", extensions.clone()),
            extensions,
            pending_text: None,
            dirty: false,
            selection: 0..0,
            style: EditorStyle::from_theme(cx),
            meta: DocMeta::default(),
            highlights: Vec::new(),
            scroll_target: None,
            view: DocumentView::new(),
            rich: false,
            rich_selection: None,
            _subscriptions: subscriptions,
        }
    }

    /// Replaces the content with `md`.
    ///
    /// Clears the undo history, the cursor position and the ranges set with
    /// [`highlight`](Self::highlight). Afterwards the editor is not
    /// [dirty](Self::is_dirty). Does not emit [`EditorEvent::Changed`].
    ///
    /// [`meta`](Self::meta) is kept. When loading a different document, set
    /// its metadata with [`set_meta`](Self::set_meta) as well.
    pub fn set_markdown(&mut self, md: &str, cx: &mut Context<Self>) {
        self.document = Document::parse_with(md, self.extensions.clone());
        self.pending_text = Some(SharedString::new(md));
        self.dirty = false;
        self.highlights.clear();
        self.scroll_target = None;
        self.rich_selection = None;
        self.view.reset();
        cx.notify();
    }

    /// Returns the content as Markdown.
    ///
    /// Lossless: text that was loaded with [`set_markdown`](Self::set_markdown)
    /// and not edited is returned byte for byte, including line endings and
    /// trailing whitespace.
    pub fn markdown(&self) -> String {
        self.document.source().to_owned()
    }

    /// Returns `true` if the content was edited since the last
    /// [`set_markdown`](Self::set_markdown) or [`mark_saved`](Self::mark_saved).
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Declares the current content as saved, see [`is_dirty`](Self::is_dirty).
    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }

    /// Runs an editing command.
    ///
    /// Changes made by a command are part of the undo history and emit
    /// [`EditorEvent::Changed`] like edits made by the user.
    pub fn command(&mut self, cmd: Command, window: &mut Window, cx: &mut Context<Self>) {
        self.flush_pending_text(window, cx);
        match cmd {
            Command::Undo => self.dispatch_to_input(&Undo, window, cx),
            Command::Redo => self.dispatch_to_input(&Redo, window, cx),
            Command::SelectAll => self
                .input
                .update(cx, |input, cx| input.select_all(window, cx)),
            Command::InsertText(text) | Command::InsertMarkdown(text) => self
                .input
                .update(cx, |input, cx| input.replace(text, window, cx)),
            Command::OpenFind => self
                .input
                .update(cx, |input, cx| input.open_search(false, cx)),
            _ => {}
        }
        self.sync_document(cx);
        self.sync_selection(cx);
    }

    /// Finds all occurrences of `query` in the text of the document, ignoring
    /// case, in document order.
    ///
    /// Searches the text as displayed (without Markdown syntax) of all blocks,
    /// nested blocks and table cells. Matches do not overlap and never span
    /// two blocks. An empty query finds nothing.
    pub fn find(&self, query: &str) -> Vec<TextRange> {
        let mut ranges = Vec::new();
        if query.is_empty() {
            return ranges;
        }
        let needle = query.to_lowercase();
        visit_texts(self.document.blocks(), &mut |block, text| {
            ranges.extend(
                find_ignore_case(text, &needle)
                    .into_iter()
                    .map(|range| TextRange {
                        start: TextPos {
                            block,
                            offset: range.start,
                        },
                        end: TextPos {
                            block,
                            offset: range.end,
                        },
                    }),
            );
        });
        ranges
    }

    /// Highlights the given ranges, for example the hits of a search run by
    /// the host. Replaces the ranges set before. Pass an empty list to remove
    /// the highlights.
    ///
    /// Ranges belong to the content they were found in. They are dropped by
    /// [`set_markdown`](Self::set_markdown).
    pub fn highlight(&mut self, ranges: Vec<TextRange>, cx: &mut Context<Self>) {
        self.highlights = ranges;
        cx.notify();
    }

    /// The ranges set with [`highlight`](Self::highlight).
    pub fn highlights(&self) -> &[TextRange] {
        &self.highlights
    }

    /// Scrolls so that `range` is visible. Does not move the cursor.
    pub fn scroll_to(&mut self, range: TextRange, cx: &mut Context<Self>) {
        self.scroll_target = Some(range);
        self.view.request_reveal(range);
        cx.notify();
    }

    /// Replaces the visual style.
    pub fn set_style(&mut self, style: EditorStyle, cx: &mut Context<Self>) {
        self.style = style;
        cx.notify();
    }

    /// The current visual style.
    pub fn style(&self) -> &EditorStyle {
        &self.style
    }

    /// Counts the words of the document as displayed, without Markdown
    /// syntax. Words are split by Unicode rules (UAX #29).
    pub fn word_count(&self) -> usize {
        let mut count = 0;
        visit_texts(self.document.blocks(), &mut |_, text| {
            count += text.unicode_words().count();
        });
        count
    }

    /// The presentation state that is not part of the Markdown, see
    /// [`DocMeta`].
    pub fn meta(&self) -> &DocMeta {
        &self.meta
    }

    /// Replaces the presentation state, usually right after
    /// [`set_markdown`](Self::set_markdown). Does not emit
    /// [`EditorEvent::MetaChanged`].
    pub fn set_meta(&mut self, meta: DocMeta, cx: &mut Context<Self>) {
        self.meta = meta;
        cx.notify();
    }

    /// The document model of the current content, for read access: outlines,
    /// task counts, previews.
    pub fn document(&self) -> &Document {
        &self.document
    }

    fn flush_pending_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.pending_text.take() {
            self.selection = self.input.update(cx, |input, cx| {
                input.set_value(text, window, cx);
                input.selected_range()
            });
        }
    }

    fn dispatch_to_input(&self, action: &dyn gpui::Action, window: &mut Window, cx: &mut App) {
        self.input
            .focus_handle(cx)
            .dispatch_action(action, window, cx);
    }

    /// Re-parses the document if the text of the input changed.
    fn sync_document(&mut self, cx: &mut Context<Self>) {
        // The input still holds the text that `set_markdown` replaced.
        if self.pending_text.is_some() {
            return;
        }
        let text = self.input.read(cx).value();
        if text.as_ref() == self.document.source() {
            return;
        }
        self.document = Document::parse_with(&text, self.extensions.clone());
        // Parsing again reuses ids for different blocks.
        self.view.invalidate_all();
        self.rich_selection = None;
        self.dirty = true;
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    fn sync_selection(&mut self, cx: &mut Context<Self>) {
        let selection = self.input.read(cx).selected_range();
        if selection != self.selection {
            self.selection = selection;
            cx.emit(EditorEvent::SelectionChanged);
        }
    }
}

impl Focusable for MarkdownEditor {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}

impl MarkdownEditor {
    /// Temporary switch between the source view and the rendered document
    /// (read-only apart from placing the caret and selecting with the mouse).
    #[doc(hidden)]
    pub fn __set_rich_preview(&mut self, on: bool, cx: &mut Context<Self>) {
        self.rich = on;
        cx.notify();
    }

    /// Temporary: sets the selection shown by the rich preview, for tests and
    /// screenshots. Offsets index the text of the given block or cell.
    #[doc(hidden)]
    pub fn __set_rich_selection(&mut self, anchor: TextPos, head: TextPos, cx: &mut Context<Self>) {
        self.rich_selection = Some(Selection::Text { anchor, head });
        self.view.request_reveal(TextRange {
            start: head,
            end: head,
        });
        cx.notify();
    }

    fn layout_input(&self) -> LayoutInput<'_> {
        LayoutInput {
            document: &self.document,
            style: &self.style,
            first_line_is_title: self.options.first_line_is_title,
            max_width: self.options.max_width,
        }
    }

    fn set_rich_selection(&mut self, selection: Option<Selection>, cx: &mut Context<Self>) {
        if self.rich_selection != selection {
            self.rich_selection = selection;
            cx.emit(EditorEvent::SelectionChanged);
            cx.notify();
        }
    }

    fn rich_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ts = window.text_system().clone();
        let view = self.view.clone();
        let mut q = view.query(self.layout_input(), &ts);
        // Rules and images are selected as a whole, table cells hold text.
        let node = q.node_at(event.position).filter(|id| {
            self.document
                .block(*id)
                .is_some_and(|b| !matches!(b.kind(), BlockKind::Table(_)))
        });
        let selection = if let Some(node) = node {
            Some(Selection::Node(node))
        } else {
            let Some(pos) = q.position_at(event.position) else {
                return;
            };
            let range = match event.click_count {
                1 => None,
                2 => q.word_range(pos),
                _ => q.block_range(pos.block),
            };
            match (range, &self.rich_selection) {
                (Some(r), _) => Some(Selection::Text {
                    anchor: r.start,
                    head: r.end,
                }),
                (None, Some(Selection::Text { anchor, .. })) if event.modifiers.shift => {
                    Some(Selection::Text {
                        anchor: *anchor,
                        head: pos,
                    })
                }
                (None, _) => Some(Selection::caret(pos)),
            }
        };
        drop(q);
        self.set_rich_selection(selection, cx);
    }

    fn rich_mouse_drag(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !event.dragging() {
            return;
        }
        let Some(Selection::Text { anchor, .. }) = self.rich_selection.clone() else {
            return;
        };
        let ts = window.text_system().clone();
        let view = self.view.clone();
        let head = view
            .query(self.layout_input(), &ts)
            .position_at(event.position);
        if let Some(head) = head {
            self.set_rich_selection(Some(Selection::Text { anchor, head }), cx);
        }
    }
}

impl ViewSource for MarkdownEditor {
    fn view_input(&self) -> ViewInput<'_> {
        ViewInput {
            layout: self.layout_input(),
            selection: self.rich_selection.as_ref(),
            highlights: &self.highlights,
            caret_visible: true,
            marked: None,
        }
    }
}

impl Render for MarkdownEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.flush_pending_text(window, cx);

        if self.rich {
            return div()
                .size_full()
                .on_mouse_down(MouseButton::Left, cx.listener(Self::rich_mouse_down))
                .on_mouse_move(cx.listener(Self::rich_mouse_drag))
                .child(DocumentElement::new(self.view.clone(), cx.entity()))
                .into_any_element();
        }

        let style = &self.style;
        let padding = style.padding;
        div()
            .size_full()
            .flex()
            .justify_center()
            .child(
                div()
                    .size_full()
                    .when_some(self.options.max_width, |this, max_width| {
                        this.max_w(max_width + padding.left + padding.right)
                    })
                    .pt(padding.top)
                    .pl(padding.left)
                    .pr(padding.right)
                    .child(
                        Textarea::new(&self.input)
                            .appearance(false)
                            .h(relative(1.))
                            .font_family(style.font_family.clone())
                            .text_size(style.text_size)
                            .line_height(relative(style.line_height)),
                    ),
            )
            .into_any_element()
    }
}

impl fmt::Debug for MarkdownEditor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MarkdownEditor")
            .field("options", &self.options)
            .field("extensions", &self.extensions)
            .field("dirty", &self.dirty)
            .field("selection", &self.selection)
            .field("meta", &self.meta)
            .field("highlights", &self.highlights)
            .field("scroll_target", &self.scroll_target)
            .finish_non_exhaustive()
    }
}

/// Calls `f` with every piece of text in document order: the text of each
/// block, then its table cells, then its children.
fn visit_texts(blocks: &[Block], f: &mut impl FnMut(BlockId, &str)) {
    for block in blocks {
        f(block.id(), block.text());
        if let BlockKind::Table(table) = block.kind() {
            for cell in table.rows.iter().flatten() {
                f(cell.id(), cell.content().text());
            }
        }
        visit_texts(block.children(), f);
    }
}

/// Finds the non-overlapping occurrences of `needle` in `text`, ignoring case.
/// `needle` must be lowercase and not empty.
fn find_ignore_case(text: &str, needle: &str) -> Vec<Range<usize>> {
    if text.is_ascii() {
        return text
            .to_ascii_lowercase()
            .match_indices(needle)
            .map(|(start, found)| start..start + found.len())
            .collect();
    }

    // Lowercasing can change the byte length of a char, so offsets in the
    // lowercased text are mapped back to the char they came from.
    let mut lowered = String::with_capacity(text.len());
    let mut origins: Vec<Range<usize>> = Vec::with_capacity(text.len());
    for (start, ch) in text.char_indices() {
        lowered.extend(ch.to_lowercase());
        origins.resize(lowered.len(), start..start + ch.len_utf8());
    }

    lowered
        .match_indices(needle)
        .filter(|(start, _)| {
            // A match that begins inside the expansion of one char has no
            // position in the original text.
            *start == 0 || origins[start - 1] != origins[*start]
        })
        .map(|(start, found)| origins[start].start..origins[start + found.len() - 1].end)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::find_ignore_case;

    #[test]
    fn finds_ascii_ignoring_case() {
        assert_eq!(
            find_ignore_case("Milk, MILK, milk", "milk"),
            [0..4, 6..10, 12..16]
        );
        assert_eq!(find_ignore_case("aaaaa", "aa"), [0..2, 2..4]);
        assert!(find_ignore_case("bread", "milk").is_empty());
    }

    #[test]
    fn maps_offsets_when_lowercasing_changes_lengths() {
        // "İ" (2 bytes) lowercases to "i̇" (3 bytes).
        let text = "İstanbul ÄPFEL äpfel";
        let found = find_ignore_case(text, "äpfel");
        assert_eq!(found.len(), 2);
        assert_eq!(&text[found[0].clone()], "ÄPFEL");
        assert_eq!(&text[found[1].clone()], "äpfel");

        let found = find_ignore_case(text, "stanbul");
        assert_eq!(&text[found[0].clone()], "stanbul");
    }
}
