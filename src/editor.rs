//! The editor entity.

mod drag;
mod input;
mod keys;
mod mouse;
mod table;
#[cfg(test)]
mod tests;
mod ui;

use std::fmt;
use std::ops::Range;
use std::time::{Duration, Instant};

use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, StatefulInteractiveElement as _, Styled as _, Subscription,
    Task, Window, div,
};
use gpui_component::scroll::Scrollbar;
use unicode_segmentation::UnicodeSegmentation as _;

pub use keys::init;

use crate::command::Command;
use crate::edit::{EditorState, Selection};
use crate::event::EditorEvent;
use crate::extension::Extensions;
use crate::meta::DocMeta;
use crate::model::{Block, BlockId, BlockKind, Document, TextPos, TextRange};
use crate::options::EditorOptions;
use crate::style::EditorStyle;
use crate::view::{ColumnWidths, DocumentElement, DocumentView, LayoutInput};

/// How long the caret stays on and off while blinking.
const BLINK_INTERVAL: Duration = Duration::from_millis(530);

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
/// the first editor is created; call [`init`] at startup as
/// well to register the key bindings.
///
/// # Interaction
///
/// The document is edited in place (WYSIWYG). A click places the caret; a
/// click on a link, wiki link or tag with the secondary modifier held (Ctrl,
/// Cmd on macOS) emits [`EditorEvent::LinkClicked`],
/// [`EditorEvent::WikiLinkClicked`] or [`EditorEvent::TagClicked`], as in
/// Notion. The mouse cursor turns into a pointer over links while the
/// modifier is held, and over checkboxes, which toggle on click.
pub struct MarkdownEditor {
    state: EditorState,
    /// The options without [`EditorOptions::extensions`], which live on in
    /// the document.
    options: EditorOptions,
    focus: FocusHandle,
    focused: bool,
    /// The history position of the saved content, see `is_dirty`.
    saved: u64,
    style: EditorStyle,
    meta: DocMeta,
    /// The column widths of `meta`, by table id, so that they stay with
    /// their table when tables before it are added or removed.
    column_widths: ColumnWidths,
    highlights: Vec<TextRange>,
    /// Layout and scroll state of the rendered document.
    view: DocumentView,
    /// The window x that vertical caret movement aims for.
    goal_x: Option<Pixels>,
    caret_on: bool,
    blink_epoch: usize,
    blink: Option<Task<()>>,
    drag: Option<mouse::Drag>,
    /// The mouse position while it is over the document.
    hover: Option<gpui::Point<Pixels>>,
    table_menu: Option<table::TableMenu>,
    column_drag: Option<table::ColumnDrag>,
    image_drag: Option<mouse::ImageDrag>,
    find: Option<ui::find::FindBar>,
    slash: Option<ui::slash::SlashMenu>,
    completion: Option<ui::complete::CompletionMenu>,
    prompt: Option<ui::link::LinkPrompt>,
    /// The style popover of the format bar is open.
    style_menu: bool,
    /// Folded top-level headings, and the top-level blocks they hide.
    folded: std::collections::HashSet<BlockId>,
    hidden: std::collections::HashSet<BlockId>,
    /// The selection head after the last change, to tell the direction of
    /// a caret move into a folded section.
    last_head: Option<TextPos>,
    block_drag: Option<drag::BlockDrag>,
    /// The query of the find bar when it was closed.
    last_query: String,
    _subscriptions: [Subscription; 2],
}

impl EventEmitter<EditorEvent> for MarkdownEditor {}

impl MarkdownEditor {
    /// Creates an empty editor.
    ///
    /// The style starts as [`EditorStyle::from_theme`]. Registers the key
    /// bindings if [`init`] was not called yet.
    pub fn new(window: &mut Window, cx: &mut Context<Self>, mut options: EditorOptions) -> Self {
        init(cx);
        // The parts are compiled in, but only offered with the `ui` feature.
        if cfg!(not(feature = "ui")) {
            options.ui = crate::options::UiParts::NONE;
        }
        let extensions = Extensions::new(std::mem::take(&mut options.extensions));
        let focus = cx.focus_handle();
        let subscriptions = [
            cx.on_focus(&focus, window, |this, _, cx| {
                this.focused = true;
                this.restart_blink(cx);
                cx.notify();
            }),
            cx.on_blur(&focus, window, |this, _, cx| {
                this.focused = false;
                this.blink = None;
                this.drag = None;
                cx.notify();
            }),
        ];
        let mut state = EditorState::from_markdown("", extensions);
        state.take_changes();
        let saved = state.history.position();

        Self {
            state,
            options,
            focus,
            focused: false,
            saved,
            style: EditorStyle::from_theme(cx),
            meta: DocMeta::default(),
            column_widths: ColumnWidths::default(),
            highlights: Vec::new(),
            view: DocumentView::new(),
            goal_x: None,
            caret_on: true,
            blink_epoch: 0,
            blink: None,
            drag: None,
            hover: None,
            table_menu: None,
            column_drag: None,
            image_drag: None,
            find: None,
            slash: None,
            completion: None,
            prompt: None,
            style_menu: false,
            folded: Default::default(),
            hidden: Default::default(),
            last_head: None,
            block_drag: None,
            last_query: String::new(),
            _subscriptions: subscriptions,
        }
    }

    /// Replaces the content with `md`.
    ///
    /// Clears the undo history and the ranges set with
    /// [`highlight`](Self::highlight), puts the caret at the start and
    /// scrolls to the top. Afterwards the editor is not
    /// [dirty](Self::is_dirty). Does not emit [`EditorEvent::Changed`].
    ///
    /// [`meta`](Self::meta) is kept. When loading a different document, set
    /// its metadata with [`set_meta`](Self::set_meta) as well.
    pub fn set_markdown(&mut self, md: &str, cx: &mut Context<Self>) {
        self.state.set_markdown(md);
        self.state.take_changes();
        self.saved = self.state.history.position();
        self.highlights.clear();
        self.goal_x = None;
        self.drag = None;
        self.table_menu = None;
        self.column_widths = widths_by_id(&self.state.doc, &self.meta);
        self.folded.clear();
        self.hidden.clear();
        self.last_head = None;
        self.view.reset();
        self.load_folds(cx);
        cx.notify();
    }

    /// Returns the content as Markdown.
    ///
    /// Lossless: text that was loaded with [`set_markdown`](Self::set_markdown)
    /// and not edited is returned byte for byte, including line endings and
    /// trailing whitespace. Edited blocks are written in a normalized style;
    /// undoing every edit gives back the original text.
    pub fn markdown(&self) -> String {
        self.state.markdown()
    }

    /// Returns `true` if the content differs from the last
    /// [`set_markdown`](Self::set_markdown) or [`mark_saved`](Self::mark_saved):
    /// undoing back to that point makes the editor clean again.
    pub fn is_dirty(&self) -> bool {
        self.state.history.position() != self.saved
    }

    /// Declares the current content as saved, see [`is_dirty`](Self::is_dirty).
    pub fn mark_saved(&mut self) {
        self.saved = self.state.history.position();
        // Typing on must not merge into the saved entry.
        self.state.history.seal();
    }

    /// Runs an editing command.
    ///
    /// Changes made by a command are part of the undo history and emit
    /// [`EditorEvent::Changed`] like edits made by the user.
    pub fn command(&mut self, cmd: Command, window: &mut Window, cx: &mut Context<Self>) {
        match cmd {
            Command::OpenFind => {
                self.open_find(window, cx);
                return;
            }
            Command::FoldHeading => {
                self.fold_at_caret(window, cx);
                return;
            }
            // The command selects the whole document; Ctrl+A grows the
            // selection from the block to the document.
            Command::SelectAll => {
                self.state.set_selection(Selection::All);
            }
            cmd => {
                self.state.exec(&cmd, Instant::now());
            }
        }
        self.after_change(false, window, cx);
    }

    /// Finds all occurrences of `query` in the text of the document, ignoring
    /// case, in document order.
    ///
    /// Searches the text as displayed (without Markdown syntax) of all blocks,
    /// nested blocks and table cells. Matches do not overlap and never span
    /// two blocks. An empty query finds nothing.
    pub fn find(&self, query: &str) -> Vec<TextRange> {
        self.find_matches(query, false)
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
        visit_texts(self.state.doc.blocks(), &mut |_, text| {
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
        self.column_widths = widths_by_id(&self.state.doc, &meta);
        self.meta = meta;
        self.load_folds(cx);
        self.leave_hidden(false);
        self.state.take_changes();
        cx.notify();
    }

    /// Brings [`DocMeta::column_widths`] in line with the widths by table
    /// id (tables may have moved, appeared or gone) and emits
    /// [`EditorEvent::MetaChanged`] if that changed it.
    pub(crate) fn sync_meta(&mut self, cx: &mut Context<Self>) {
        let mut ordinal = 0;
        let mut widths = std::collections::BTreeMap::new();
        visit_tables(self.state.doc.blocks(), &mut |id, _| {
            if let Some(w) = self.column_widths.get(&id) {
                widths.insert(ordinal, w.clone());
            }
            ordinal += 1;
        });
        self.column_widths
            .retain(|id, _| self.state.doc.block(*id).is_some());
        if widths != self.meta.column_widths {
            self.meta.column_widths = widths;
            cx.emit(EditorEvent::MetaChanged);
        }
    }

    /// The document model of the current content, for read access: outlines,
    /// task counts, previews.
    pub fn document(&self) -> &Document {
        &self.state.doc
    }

    /// Sets the selection, for screenshots and tests. Offsets index the text
    /// of the given block or cell.
    #[doc(hidden)]
    pub fn __set_selection(&mut self, anchor: TextPos, head: TextPos, cx: &mut Context<Self>) {
        self.state.set_selection(Selection::Text { anchor, head });
        self.state.take_changes();
        self.reveal_selection();
        cx.notify();
    }

    /// Selects the rectangle between two cells of a table, or the block
    /// `anchor` as a whole if `head` is `None`, for screenshots.
    #[doc(hidden)]
    pub fn __select_cells_or_node(
        &mut self,
        anchor: BlockId,
        head: Option<BlockId>,
        cx: &mut Context<Self>,
    ) {
        let selection = match head {
            Some(head) => Selection::Cells { anchor, head },
            None => Selection::Node(anchor),
        };
        self.state.set_selection(selection);
        self.state.take_changes();
        cx.notify();
    }

    fn layout_input(&self) -> LayoutInput<'_> {
        LayoutInput {
            document: &self.state.doc,
            style: &self.style,
            first_line_is_title: self.options.first_line_is_title,
            max_width: self.options.max_width,
            column_widths: &self.column_widths,
            image_preview: self.image_drag.as_ref().map(|d| (d.id, d.width)),
            hidden: &self.hidden,
        }
    }

    /// Scrolls the caret (or the selected node) into view at the next frame.
    fn reveal_selection(&self) {
        let pos = match &self.state.selection {
            Selection::Text { head, .. } => *head,
            Selection::All => return,
            Selection::Cells { head, .. } => TextPos {
                block: *head,
                offset: 0,
            },
            Selection::Node(id) => TextPos {
                block: *id,
                offset: 0,
            },
        };
        self.view.request_reveal(TextRange {
            start: pos,
            end: pos,
        });
    }

    /// Picks up what the last operation on the state changed: relayout,
    /// events, caret visibility. `keep_goal` keeps the goal column of
    /// vertical movement.
    fn after_change(&mut self, keep_goal: bool, window: &mut Window, cx: &mut Context<Self>) {
        let mut changes = self.state.take_changes();
        // Any edit can add or remove a heading, which shifts the ordinals:
        // a linear pass, only while something is folded.
        if changes.content && !self.folded.is_empty() {
            self.sync_folds(cx);
        }
        if !self.hidden.is_empty() {
            self.leave_hidden(true);
            changes.selection |= self.state.take_changes().selection;
        }
        self.last_head = self.state.selection.head();
        if !keep_goal {
            self.goal_x = None;
        }
        if self.slash.is_some() && (changes.content || changes.selection) {
            self.refresh_slash();
        }
        if self.completion.is_some() && (changes.content || changes.selection) {
            self.refresh_completion();
        }
        if changes.content || changes.selection {
            self.reveal_selection();
            self.restart_blink(cx);
        }
        if changes.structure && !self.column_widths.is_empty() {
            self.sync_meta(cx);
        }
        if changes.content {
            self.view.invalidate(changes.blocks);
            if self.find.is_some() {
                self.refresh_find(false, cx);
            }
            window.notify_text_input_changed(&self.focus);
            cx.emit(EditorEvent::Changed);
        }
        if changes.selection {
            window.notify_text_selection_changed(&self.focus);
            cx.emit(EditorEvent::SelectionChanged);
        }
        cx.notify();
    }

    /// Shows the caret and starts blinking after a full interval, so that
    /// it stays visible while the user types or moves it.
    fn restart_blink(&mut self, cx: &mut Context<Self>) {
        self.caret_on = true;
        self.blink_epoch += 1;
        if !self.focused {
            self.blink = None;
            return;
        }
        let epoch = self.blink_epoch;
        self.blink = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(BLINK_INTERVAL).await;
                let current = this.update(cx, |this, cx| {
                    if this.blink_epoch != epoch {
                        return false;
                    }
                    this.caret_on = !this.caret_on;
                    cx.notify();
                    true
                });
                if !matches!(current, Ok(true)) {
                    break;
                }
            }
        }));
    }
}

impl Focusable for MarkdownEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for MarkdownEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Focus that arrived before the focus listener was active.
        let focused = self.focus.is_focused(window);
        if focused != self.focused {
            self.focused = focused;
            if focused {
                self.restart_blink(cx);
            } else {
                self.blink = None;
            }
        }
        let document = div()
            .id("markdown-editor")
            .role(gpui::Role::Group)
            .aria_label("Markdown editor")
            .key_context(keys::CONTEXT)
            .track_focus(&self.focus)
            .size_full()
            .on_drop(
                cx.listener(|this, paths: &gpui::ExternalPaths, window, cx| {
                    let position = window.mouse_position();
                    this.drop_paths(paths.paths(), position, window, cx);
                }),
            );
        let document = keys::bind_actions(document, cx)
            .child(DocumentElement::new(self.view.clone(), cx.entity()))
            .child(Scrollbar::vertical(&self.view))
            .children(self.render_table_menu(cx));
        div()
            .relative()
            .size_full()
            .child(document)
            .children(self.render_format_bar(cx))
            .children(self.render_find(cx))
            .children(self.render_slash(window, cx))
            .children(self.render_completion(window, cx))
            .children(self.render_prompt(window, cx))
            .children(self.render_link_bubble(window, cx))
    }
}

impl fmt::Debug for MarkdownEditor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MarkdownEditor")
            .field("options", &self.options)
            .field("extensions", &self.state.doc.extensions)
            .field("dirty", &self.is_dirty())
            .field("selection", &self.state.selection)
            .field("meta", &self.meta)
            .field("highlights", &self.highlights)
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

/// Calls `f` with every table in document order, with its column count.
fn visit_tables(blocks: &[Block], f: &mut impl FnMut(BlockId, usize)) {
    for block in blocks {
        if let BlockKind::Table(table) = block.kind() {
            f(
                block.id(),
                table.rows.iter().map(Vec::len).max().unwrap_or(0),
            );
        }
        visit_tables(block.children(), f);
    }
}

/// The column widths of `meta` keyed by the ids of the tables of `doc`.
fn widths_by_id(doc: &Document, meta: &DocMeta) -> ColumnWidths {
    let mut out = ColumnWidths::default();
    let mut ordinal = 0;
    visit_tables(doc.blocks(), &mut |id, _| {
        if let Some(w) = meta.column_widths.get(&ordinal) {
            out.insert(id, w.clone());
        }
        ordinal += 1;
    });
    out
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
mod find_tests {
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
