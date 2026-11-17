//! Keyboard: actions, their default bindings and what they do.

use std::time::Instant;

use gpui::{
    App, ClipboardItem, Context, Div, Global, InteractiveElement as _, KeyBinding,
    KeyBindingContextPredicate, OperatingSystem, Stateful, Window, actions,
};

use super::MarkdownEditor;
use crate::command::Command;
use crate::edit::{Selection, Unit};
use crate::model::{BlockKind, Mark, TextPos};

/// The key context of the editor, for host key bindings.
pub(crate) const CONTEXT: &str = "MarkdownEditor";

/// The key context of the find bar.
pub(crate) const FIND_CONTEXT: &str = "MarkdownEditorFind";

/// The key context of the link popover.
pub(crate) const PROMPT_CONTEXT: &str = "MarkdownEditorPrompt";

/// The key context entry added while the caret is in a table cell.
pub(crate) const TABLE_CONTEXT: &str = "table";

actions!(
    markdown_editor,
    [
        ToggleBold,
        ToggleItalic,
        ToggleUnderline,
        ToggleStrike,
        ToggleCode,
        ToggleHighlight,
        ToggleLink,
        ToggleChecklist,
        Heading1,
        Heading2,
        Heading3,
        Paragraph,
        ToggleCheckbox,
        MoveBlockUp,
        MoveBlockDown,
        Undo,
        Redo,
        SelectAll,
        Newline,
        SoftBreak,
        Indent,
        Outdent,
        Backspace,
        Delete,
        DeleteWordLeft,
        DeleteWordRight,
        Left,
        Right,
        Up,
        Down,
        WordLeft,
        WordRight,
        LineStart,
        LineEnd,
        DocStart,
        DocEnd,
        PageUp,
        PageDown,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectWordLeft,
        SelectWordRight,
        SelectLineStart,
        SelectLineEnd,
        SelectDocStart,
        SelectDocEnd,
        SelectPageUp,
        SelectPageDown,
        CopySelection,
        CutSelection,
        PasteClipboard,
        Cancel,
        OpenFind,
        FindNext,
        FindPrevious,
        CloseFind,
        ToggleFindCase,
        ClosePrompt,
        ApplyPrompt,
        FoldHeading,
        TableInsertRowAbove,
        TableInsertRowBelow,
        TableInsertColumnLeft,
        TableInsertColumnRight,
        TableDeleteRow,
        TableDeleteColumn,
        TableMoveColumnLeft,
        TableMoveColumnRight,
        TableMenu,
    ]
);

/// Marks that the key bindings are registered.
struct Bound;

impl Global for Bound {}

/// Registers the editor's key bindings (plan §6: Ctrl+B/I/U, Ctrl+Z, arrows,
/// ...; Cmd instead of Ctrl on macOS). Call it once at startup, after
/// `gpui_component::init` and before binding your own keys, so that your
/// bindings take precedence. [`MarkdownEditor::new`] calls it if you did
/// not; calling it again does nothing.
///
/// The bindings apply in the key context `MarkdownEditor`.
pub fn init(cx: &mut App) {
    if cx.has_global::<Bound>() {
        return;
    }
    cx.set_global(Bound);
    let bindings = bindings(cx);
    cx.bind_keys(bindings);
}

fn bindings(cx: &App) -> Vec<KeyBinding> {
    let mac = cx.platform_info().os == OperatingSystem::Mac;
    let mut keys: Vec<(&str, Box<dyn gpui::Action>)> = vec![
        ("secondary-b", Box::new(ToggleBold)),
        ("secondary-i", Box::new(ToggleItalic)),
        ("secondary-u", Box::new(ToggleUnderline)),
        ("secondary-shift-x", Box::new(ToggleStrike)),
        ("secondary-e", Box::new(ToggleCode)),
        ("secondary-shift-h", Box::new(ToggleHighlight)),
        ("secondary-k", Box::new(ToggleLink)),
        ("secondary-shift-l", Box::new(ToggleChecklist)),
        ("secondary-alt-1", Box::new(Heading1)),
        ("secondary-alt-2", Box::new(Heading2)),
        ("secondary-alt-3", Box::new(Heading3)),
        ("secondary-alt-0", Box::new(Paragraph)),
        ("secondary-enter", Box::new(ToggleCheckbox)),
        ("alt-up", Box::new(MoveBlockUp)),
        ("alt-down", Box::new(MoveBlockDown)),
        ("secondary-z", Box::new(Undo)),
        ("secondary-shift-z", Box::new(Redo)),
        ("secondary-y", Box::new(Redo)),
        ("secondary-a", Box::new(SelectAll)),
        ("enter", Box::new(Newline)),
        ("shift-enter", Box::new(SoftBreak)),
        ("tab", Box::new(Indent)),
        ("shift-tab", Box::new(Outdent)),
        ("backspace", Box::new(Backspace)),
        ("shift-backspace", Box::new(Backspace)),
        ("delete", Box::new(Delete)),
        ("left", Box::new(Left)),
        ("right", Box::new(Right)),
        ("up", Box::new(Up)),
        ("down", Box::new(Down)),
        ("home", Box::new(LineStart)),
        ("end", Box::new(LineEnd)),
        ("pageup", Box::new(PageUp)),
        ("pagedown", Box::new(PageDown)),
        ("shift-left", Box::new(SelectLeft)),
        ("shift-right", Box::new(SelectRight)),
        ("shift-up", Box::new(SelectUp)),
        ("shift-down", Box::new(SelectDown)),
        ("shift-home", Box::new(SelectLineStart)),
        ("shift-end", Box::new(SelectLineEnd)),
        ("shift-pageup", Box::new(SelectPageUp)),
        ("shift-pagedown", Box::new(SelectPageDown)),
        ("secondary-c", Box::new(CopySelection)),
        ("secondary-x", Box::new(CutSelection)),
        ("secondary-v", Box::new(PasteClipboard)),
        ("escape", Box::new(Cancel)),
        ("secondary-f", Box::new(OpenFind)),
        ("f3", Box::new(FindNext)),
        ("shift-f3", Box::new(FindPrevious)),
        ("secondary-.", Box::new(FoldHeading)),
    ];
    if mac {
        keys.extend([
            ("alt-left", Box::new(WordLeft) as Box<dyn gpui::Action>),
            ("alt-right", Box::new(WordRight)),
            ("alt-shift-left", Box::new(SelectWordLeft)),
            ("alt-shift-right", Box::new(SelectWordRight)),
            ("alt-backspace", Box::new(DeleteWordLeft)),
            ("alt-delete", Box::new(DeleteWordRight)),
            ("cmd-left", Box::new(LineStart)),
            ("cmd-right", Box::new(LineEnd)),
            ("cmd-shift-left", Box::new(SelectLineStart)),
            ("cmd-shift-right", Box::new(SelectLineEnd)),
            ("cmd-up", Box::new(DocStart)),
            ("cmd-down", Box::new(DocEnd)),
            ("cmd-shift-up", Box::new(SelectDocStart)),
            ("cmd-shift-down", Box::new(SelectDocEnd)),
        ]);
    } else {
        keys.extend([
            ("ctrl-left", Box::new(WordLeft) as Box<dyn gpui::Action>),
            ("ctrl-right", Box::new(WordRight)),
            ("ctrl-shift-left", Box::new(SelectWordLeft)),
            ("ctrl-shift-right", Box::new(SelectWordRight)),
            ("ctrl-backspace", Box::new(DeleteWordLeft)),
            ("ctrl-delete", Box::new(DeleteWordRight)),
            ("ctrl-home", Box::new(DocStart)),
            ("ctrl-end", Box::new(DocEnd)),
            ("ctrl-shift-home", Box::new(SelectDocStart)),
            ("ctrl-shift-end", Box::new(SelectDocEnd)),
        ]);
    }
    // Enter is bound here (after the field's own binding, which lets it
    // through) so that it is not typed into the field as well.
    let find: Vec<(&str, Box<dyn gpui::Action>)> = vec![
        ("escape", Box::new(CloseFind)),
        ("enter", Box::new(FindNext)),
        ("shift-enter", Box::new(FindPrevious)),
        ("secondary-f", Box::new(OpenFind)),
        ("f3", Box::new(FindNext)),
        ("shift-f3", Box::new(FindPrevious)),
        ("alt-c", Box::new(ToggleFindCase)),
    ];
    let load = |context: &str, keys: Vec<(&str, Box<dyn gpui::Action>)>| {
        let context = KeyBindingContextPredicate::parse(context)
            .ok()
            .map(std::rc::Rc::new);
        keys.into_iter()
            .map(move |(keystrokes, action)| {
                KeyBinding::load(keystrokes, action, context.clone(), false, None, cx)
                    .expect("valid key binding")
            })
            .collect::<Vec<_>>()
    };
    // Table structure, only with the caret in a table. Alt+Up/Down (move
    // block) moves the row there.
    let table: Vec<(&str, Box<dyn gpui::Action>)> = vec![
        ("secondary-alt-up", Box::new(TableInsertRowAbove)),
        ("secondary-alt-down", Box::new(TableInsertRowBelow)),
        ("secondary-alt-left", Box::new(TableInsertColumnLeft)),
        ("secondary-alt-right", Box::new(TableInsertColumnRight)),
        ("secondary-alt-backspace", Box::new(TableDeleteRow)),
        ("secondary-alt-shift-backspace", Box::new(TableDeleteColumn)),
        ("secondary-alt-shift-left", Box::new(TableMoveColumnLeft)),
        ("secondary-alt-shift-right", Box::new(TableMoveColumnRight)),
        ("shift-f10", Box::new(TableMenu)),
    ];
    let mut all = load(CONTEXT, keys);
    all.extend(load(&format!("{CONTEXT} && {TABLE_CONTEXT}"), table));
    all.extend(load(FIND_CONTEXT, find));
    all.extend(load(
        PROMPT_CONTEXT,
        vec![
            ("escape", Box::new(ClosePrompt)),
            ("enter", Box::new(ApplyPrompt)),
        ],
    ));
    all
}

/// What a key does.
#[derive(Clone, Debug)]
pub(super) enum Op {
    Command(Command),
    Move(Motion, bool),
    DeleteBackward(Unit),
    DeleteForward(Unit),
    Enter,
    SoftBreak,
    Tab,
    ShiftTab,
    Copy,
    Cut,
    Paste,
    Link,
    /// Escape: closes the open menu or bar.
    Cancel,
    OpenFind,
    /// F3, Shift+F3: the next or previous match of the open find bar.
    FindStep(bool),
    /// A table structure key.
    Table(TableKey),
}

/// A table structure key, acting on the caret's cell.
#[derive(Clone, Copy, Debug)]
pub(super) enum TableKey {
    /// Insert a row below (`true`) or above.
    InsertRow(bool),
    /// Insert a column right (`true`) or left.
    InsertColumn(bool),
    DeleteRow,
    DeleteColumn,
    /// Move the column right (`true`) or left.
    MoveColumn(bool),
    /// Open the row menu at the caret's cell, or switch an open row menu
    /// to the column menu.
    Menu,
}

/// A caret movement.
#[derive(Clone, Copy, Debug)]
pub(super) enum Motion {
    Left(Unit),
    Right(Unit),
    Up,
    Down,
    LineStart,
    LineEnd,
    DocStart,
    DocEnd,
    PageUp,
    PageDown,
}

/// Adds the action handlers to the editor's root element.
pub(super) fn bind_actions(root: Stateful<Div>, cx: &mut Context<MarkdownEditor>) -> Stateful<Div> {
    macro_rules! ops {
        ($root:expr; $($action:ident => $op:expr),* $(,)?) => {
            $root $(.on_action(cx.listener(
                |this: &mut MarkdownEditor, _: &$action, window: &mut Window, cx: &mut Context<MarkdownEditor>| {
                    this.run($op, window, cx)
                },
            )))*
        };
    }
    use Unit::{Grapheme, Word};
    ops!(root;
        ToggleBold => mark(Mark::Bold),
        ToggleItalic => mark(Mark::Italic),
        ToggleUnderline => mark(Mark::Underline),
        ToggleStrike => mark(Mark::Strike),
        ToggleCode => mark(Mark::Code),
        ToggleHighlight => mark(Mark::Highlight),
        ToggleLink => Op::Link,
        ToggleChecklist => block(BlockKind::TaskItem { checked: false }),
        Heading1 => block(BlockKind::Heading(1)),
        Heading2 => block(BlockKind::Heading(2)),
        Heading3 => block(BlockKind::Heading(3)),
        Paragraph => block(BlockKind::Paragraph),
        ToggleCheckbox => Op::Command(Command::ToggleTask),
        MoveBlockUp => Op::Command(Command::MoveBlockUp),
        MoveBlockDown => Op::Command(Command::MoveBlockDown),
        Undo => Op::Command(Command::Undo),
        Redo => Op::Command(Command::Redo),
        SelectAll => Op::Command(Command::SelectAll),
        Newline => Op::Enter,
        SoftBreak => Op::SoftBreak,
        Indent => Op::Tab,
        Outdent => Op::ShiftTab,
        Backspace => Op::DeleteBackward(Grapheme),
        Delete => Op::DeleteForward(Grapheme),
        DeleteWordLeft => Op::DeleteBackward(Word),
        DeleteWordRight => Op::DeleteForward(Word),
        Left => Op::Move(Motion::Left(Grapheme), false),
        Right => Op::Move(Motion::Right(Grapheme), false),
        Up => Op::Move(Motion::Up, false),
        Down => Op::Move(Motion::Down, false),
        WordLeft => Op::Move(Motion::Left(Word), false),
        WordRight => Op::Move(Motion::Right(Word), false),
        LineStart => Op::Move(Motion::LineStart, false),
        LineEnd => Op::Move(Motion::LineEnd, false),
        DocStart => Op::Move(Motion::DocStart, false),
        DocEnd => Op::Move(Motion::DocEnd, false),
        PageUp => Op::Move(Motion::PageUp, false),
        PageDown => Op::Move(Motion::PageDown, false),
        SelectLeft => Op::Move(Motion::Left(Grapheme), true),
        SelectRight => Op::Move(Motion::Right(Grapheme), true),
        SelectUp => Op::Move(Motion::Up, true),
        SelectDown => Op::Move(Motion::Down, true),
        SelectWordLeft => Op::Move(Motion::Left(Word), true),
        SelectWordRight => Op::Move(Motion::Right(Word), true),
        SelectLineStart => Op::Move(Motion::LineStart, true),
        SelectLineEnd => Op::Move(Motion::LineEnd, true),
        SelectDocStart => Op::Move(Motion::DocStart, true),
        SelectDocEnd => Op::Move(Motion::DocEnd, true),
        SelectPageUp => Op::Move(Motion::PageUp, true),
        SelectPageDown => Op::Move(Motion::PageDown, true),
        CopySelection => Op::Copy,
        CutSelection => Op::Cut,
        PasteClipboard => Op::Paste,
        Cancel => Op::Cancel,
        OpenFind => Op::OpenFind,
        FindNext => Op::FindStep(true),
        FindPrevious => Op::FindStep(false),
        FoldHeading => Op::Command(Command::FoldHeading),
        TableInsertRowAbove => Op::Table(TableKey::InsertRow(false)),
        TableInsertRowBelow => Op::Table(TableKey::InsertRow(true)),
        TableInsertColumnLeft => Op::Table(TableKey::InsertColumn(false)),
        TableInsertColumnRight => Op::Table(TableKey::InsertColumn(true)),
        TableDeleteRow => Op::Table(TableKey::DeleteRow),
        TableDeleteColumn => Op::Table(TableKey::DeleteColumn),
        TableMoveColumnLeft => Op::Table(TableKey::MoveColumn(false)),
        TableMoveColumnRight => Op::Table(TableKey::MoveColumn(true)),
        TableMenu => Op::Table(TableKey::Menu),
    )
}

fn mark(mark: Mark) -> Op {
    Op::Command(Command::ToggleMark(mark))
}

fn block(kind: BlockKind) -> Op {
    Op::Command(Command::SetBlock(kind))
}

impl MarkdownEditor {
    pub(super) fn run(&mut self, op: Op, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .completion
            .as_ref()
            .is_some_and(|c| !c.items.is_empty())
            && self.completion_key(&op, window, cx)
        {
            return;
        }
        if self.slash.is_some() && self.slash_key(&op, window, cx) {
            return;
        }
        if self.table_menu.is_some() {
            let delta = match op {
                Op::Move(Motion::Up, false) => Some(Some(-1)),
                Op::Move(Motion::Down, false) => Some(Some(1)),
                Op::Enter => Some(None),
                _ => None,
            };
            if let Some(delta) = delta
                && self.table_menu_key(delta, window, cx)
            {
                return;
            }
        }
        let now = Instant::now();
        let mut keep_goal = false;
        match op {
            Op::Command(Command::FoldHeading) => {
                self.fold_at_caret(window, cx);
                return;
            }
            // In a table, moving the block moves the caret's row.
            Op::Command(Command::MoveBlockUp | Command::MoveBlockDown)
                if self.state.current_cell().is_some() =>
            {
                let down = matches!(op, Op::Command(Command::MoveBlockDown));
                self.state.table_move_row(down);
            }
            Op::Table(key) => {
                if !self.table_key(key, window, cx) {
                    cx.propagate();
                }
                return;
            }
            Op::Command(command) => {
                self.state.exec(&command, now);
            }
            Op::Move(motion, extend) => keep_goal = self.motion(motion, extend, window),
            Op::DeleteBackward(unit) => {
                self.state.delete_backward(unit, now);
            }
            Op::DeleteForward(unit) => {
                self.state.delete_forward(unit, now);
            }
            Op::Enter => {
                self.state.enter(now);
            }
            Op::SoftBreak => {
                self.state.soft_break(now);
            }
            Op::Tab => {
                self.state.tab(now);
            }
            Op::ShiftTab => {
                self.state.shift_tab();
            }
            Op::Copy => {
                self.copy(cx);
            }
            Op::Cut => {
                if self.copy(cx) {
                    self.state.delete_selection();
                }
            }
            Op::Paste => self.paste(cx),
            Op::Link if self.options.ui.link_popover => {
                self.open_link_prompt(window, cx);
                return;
            }
            Op::Link => self.toggle_link(cx),
            Op::Cancel => {
                // Without a menu Escape belongs to the host.
                let style_menu = std::mem::take(&mut self.style_menu);
                if self.table_menu.take().is_none() && self.find.take().is_none() && !style_menu {
                    cx.propagate();
                    return;
                }
            }
            Op::OpenFind => {
                if self.options.ui.find_bar {
                    self.open_find(window, cx);
                } else {
                    cx.propagate();
                }
                return;
            }
            Op::FindStep(forward) => {
                if self.find.is_some() {
                    self.find_step(forward, cx);
                } else {
                    cx.propagate();
                }
                return;
            }
        }
        self.after_change(keep_goal, window, cx);
    }

    /// Keys of the open completion list. `true` if the key was used.
    fn completion_key(&mut self, op: &Op, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match op {
            Op::Move(Motion::Up, false) => self.completion_move(-1),
            Op::Move(Motion::Down, false) => self.completion_move(1),
            Op::Enter | Op::Tab => {
                let selected = self.completion.as_ref().map_or(0, |m| m.selected);
                self.completion_choose(selected, window, cx);
            }
            Op::Cancel => self.completion = None,
            _ => return false,
        }
        cx.notify();
        true
    }

    /// Keys of the open slash menu. `true` if the key was used.
    fn slash_key(&mut self, op: &Op, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match op {
            Op::Move(Motion::Up, false) => self.slash_move(-1),
            Op::Move(Motion::Down, false) => self.slash_move(1),
            Op::Enter | Op::Tab => match &self.slash {
                Some(menu) if !menu.entries.is_empty() => {
                    let selected = menu.selected;
                    self.slash_choose(selected, window, cx);
                }
                _ => {
                    self.slash = None;
                    return false;
                }
            },
            Op::Cancel => self.slash = None,
            _ => return false,
        }
        cx.notify();
        true
    }

    /// Pastes the clipboard: image data through
    /// [`EditorOptions::on_image`](crate::EditorOptions::on_image) as an
    /// image block, else its text.
    fn paste(&mut self, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        let image = item.entries().iter().find_map(|entry| match entry {
            gpui::ClipboardEntry::Image(image) => Some(image.clone()),
            _ => None,
        });
        if let Some(image) = image
            && let Some(handler) = self.options.on_image.clone()
            && let Some(src) = handler(image.bytes, image.format.mime_type())
        {
            self.state.insert_image(&src, "");
            return;
        }
        if let Some(text) = item.text() {
            self.state.paste_text(&text);
        }
    }

    /// Writes the selection as Markdown to the clipboard. `false` if
    /// nothing is selected.
    fn copy(&mut self, cx: &mut Context<Self>) -> bool {
        let text = self.state.copy_text();
        if text.is_empty() {
            return false;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        true
    }

    /// Ctrl+K without the link popover: removes the link at the caret or in
    /// the selection, or links to the URL on the clipboard.
    fn toggle_link(&mut self, cx: &mut Context<Self>) {
        let link = Mark::Link {
            url: "".into(),
            title: None,
        };
        if self.state.has_mark(&link) {
            self.state.set_link(None);
            return;
        }
        let url = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .map(|text| text.trim().to_string())
            .filter(|text| crate::edit::is_url(text));
        if let Some(url) = url {
            self.state.set_link(Some(&url));
        }
    }

    /// Moves the caret. Returns `true` for vertical movement, which keeps
    /// the goal column.
    fn motion(&mut self, motion: Motion, extend: bool, window: &mut Window) -> bool {
        let state = &mut self.state;
        match motion {
            Motion::Left(unit) => {
                state.move_left(unit, extend);
                return false;
            }
            Motion::Right(unit) => {
                state.move_right(unit, extend);
                return false;
            }
            Motion::DocStart => {
                state.move_to_doc_start(extend);
                return false;
            }
            Motion::DocEnd => {
                state.move_to_doc_end(extend);
                return false;
            }
            _ => {}
        }
        let (anchor, head) = match state.selection.clone() {
            Selection::Text { anchor, head } => (anchor, head),
            Selection::Node(_) | Selection::All | Selection::Cells { .. } => {
                match motion {
                    Motion::Up | Motion::PageUp | Motion::LineStart => {
                        state.move_left(Unit::Grapheme, extend)
                    }
                    _ => state.move_right(Unit::Grapheme, extend),
                };
                return false;
            }
        };
        let ts = window.text_system().clone();
        let view = self.view.clone();
        let mut q = view.query(self.layout_input(), &ts);
        let vertical = matches!(
            motion,
            Motion::Up | Motion::Down | Motion::PageUp | Motion::PageDown
        );
        let goal = self
            .goal_x
            .or_else(|| q.caret_bounds(head).map(|b| b.origin.x));
        let target: Option<TextPos> = match (motion, goal) {
            (Motion::Up, Some(x)) => q.position_above(head, x).or_else(|| self.state.first_pos()),
            (Motion::Down, Some(x)) => q.position_below(head, x).or_else(|| self.state.last_pos()),
            (Motion::PageUp, Some(x)) => q.page_up(head, x),
            (Motion::PageDown, Some(x)) => q.page_down(head, x),
            (Motion::LineStart, _) => q.line_start(head),
            (Motion::LineEnd, _) => q.line_end(head),
            _ => None,
        };
        drop(q);
        if let Some(target) = target {
            let anchor = if extend { anchor } else { target };
            self.state.set_selection(Selection::Text {
                anchor,
                head: target,
            });
        }
        if vertical {
            self.goal_x = goal;
        }
        vertical
    }
}
