//! The slash menu: `/` typed at the start of a line or after a space opens
//! a list of block types and the host's [`SlashItem`]s, filtered by the
//! text typed after the slash.
//!
//! The document keeps the focus: Up and Down move the selection in the
//! list, Enter or Tab choose, Escape closes the menu and keeps the text.
//! Choosing removes the typed `/query` first, so that undo brings it back.

use gpui::{
    Context, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _, Pixels,
    SharedString, Styled as _, Window, div, prelude::FluentBuilder as _, px, size,
};

use super::super::MarkdownEditor;
use super::{Palette, appear, menu_panel, menu_row, overlay, place, wrap};
use crate::command::Command;
use crate::edit::Selection;
use crate::edit::order;
use crate::model::{BlockId, BlockKind, TextPos};
use crate::options::SlashHandler;

/// Width of the menu (the style popover of the prototype).
const WIDTH: Pixels = px(250.);
/// Rows shown at once.
const VISIBLE: usize = 8;
const ROW_HEIGHT: Pixels = px(34.);
const ROW_HEIGHT_DESCRIBED: Pixels = px(48.);

/// What an entry does.
#[derive(Clone)]
pub(in crate::editor) enum SlashAction {
    Block(BlockKind),
    Command(Command),
    /// Asks for the source of an image.
    Image,
    /// Types text at the caret (entries of extensions).
    Text(&'static str),
    Host(SlashHandler),
}

/// One entry of the menu.
#[derive(Clone)]
pub(in crate::editor) struct SlashEntry {
    pub id: SharedString,
    pub label: SharedString,
    pub description: Option<SharedString>,
    keywords: Vec<SharedString>,
    pub action: SlashAction,
}

/// The open menu.
pub(in crate::editor) struct SlashMenu {
    /// The block with the slash.
    pub block: BlockId,
    /// Byte offset of the slash.
    pub start: usize,
    /// The entries matching the query, best first.
    pub entries: Vec<SlashEntry>,
    pub selected: usize,
    /// The first entry shown.
    scroll: usize,
}

fn entry(id: &str, label: &str, keywords: &[&str], action: SlashAction) -> SlashEntry {
    SlashEntry {
        id: SharedString::from(id.to_string()),
        label: SharedString::from(label.to_string()),
        description: None,
        keywords: keywords
            .iter()
            .map(|k| SharedString::from(k.to_string()))
            .collect(),
        action,
    }
}

/// The built-in entries, in menu order.
fn builtin() -> Vec<SlashEntry> {
    use SlashAction::{Block, Command as Cmd};
    vec![
        entry(
            "text",
            "Text",
            &["paragraph", "plain"],
            Block(BlockKind::Paragraph),
        ),
        entry(
            "h1",
            "Heading 1",
            &["title", "h1"],
            Block(BlockKind::Heading(1)),
        ),
        entry(
            "h2",
            "Heading 2",
            &["subtitle", "h2"],
            Block(BlockKind::Heading(2)),
        ),
        entry("h3", "Heading 3", &["h3"], Block(BlockKind::Heading(3))),
        entry(
            "bullet",
            "Bullet list",
            &["unordered", "ul"],
            Block(BlockKind::BulletItem),
        ),
        entry(
            "numbered",
            "Numbered list",
            &["ordered", "ol"],
            Block(BlockKind::OrderedItem { number: 1 }),
        ),
        entry(
            "checklist",
            "Checklist",
            &["todo", "task", "checkbox"],
            Block(BlockKind::TaskItem { checked: false }),
        ),
        entry(
            "quote",
            "Quote",
            &["blockquote", "citation"],
            Block(BlockKind::Quote),
        ),
        entry(
            "code",
            "Code block",
            &["monospace", "pre", "snippet"],
            Block(BlockKind::CodeBlock { lang: None }),
        ),
        entry(
            "divider",
            "Divider",
            &["rule", "line", "hr", "separator"],
            Cmd(Command::InsertRule),
        ),
        entry(
            "table",
            "Table",
            &["grid", "columns"],
            Cmd(Command::InsertTable { rows: 3, cols: 3 }),
        ),
        entry("image", "Image", &["picture", "photo"], SlashAction::Image),
    ]
}

/// How well `entry` matches `query` (lowercase): lower is better, `None`
/// for no match.
fn score(entry: &SlashEntry, query: &str) -> Option<u8> {
    if query.is_empty() {
        return Some(0);
    }
    let label = entry.label.to_lowercase();
    if label.starts_with(query) {
        return Some(0);
    }
    if label.split_whitespace().any(|w| w.starts_with(query)) {
        return Some(1);
    }
    let keyword = |k: &SharedString| k.to_lowercase().starts_with(query);
    if entry.keywords.iter().any(keyword) || entry.id.to_lowercase().starts_with(query) {
        return Some(2);
    }
    (query.len() >= 3 && label.contains(query)).then_some(3)
}

/// The entries matching `query`, best first, in menu order within a score.
fn filter(entries: Vec<SlashEntry>, query: &str) -> Vec<SlashEntry> {
    let query = query.trim().to_lowercase();
    let mut scored: Vec<(u8, usize, SlashEntry)> = entries
        .into_iter()
        .enumerate()
        .filter_map(|(i, e)| score(&e, &query).map(|s| (s, i, e)))
        .collect();
    scored.sort_by_key(|(s, i, _)| (*s, *i));
    scored.into_iter().map(|(_, _, e)| e).collect()
}

impl MarkdownEditor {
    fn slash_entries(&self) -> Vec<SlashEntry> {
        let mut entries = builtin();
        for extension in self.state.doc.extensions.iter() {
            entries.extend(extension.slash_items().into_iter().map(|item| {
                SlashEntry {
                    id: item.id.into(),
                    label: item.label.into(),
                    description: None,
                    keywords: item
                        .keywords
                        .iter()
                        .map(|k| SharedString::from(*k))
                        .collect(),
                    action: SlashAction::Text(item.insert),
                }
            }));
        }
        entries.extend(self.options.slash_items.iter().map(|item| SlashEntry {
            id: item.id.clone(),
            label: item.label.clone(),
            description: item.description.clone(),
            keywords: item.keywords.clone(),
            action: SlashAction::Host(item.on_select.clone()),
        }));
        for entry in &mut entries {
            self.localize(entry);
        }
        entries
    }

    /// Shows `entry` with the text of `slash.<id>` from the editor strings,
    /// if there is one. The English label stays a keyword, so that `/head`
    /// finds the headings in every language.
    fn localize(&self, entry: &mut SlashEntry) {
        let Some(label) = self.options.strings.lookup(&format!("slash.{}", entry.id)) else {
            return;
        };
        if label != entry.label {
            let english = std::mem::replace(&mut entry.label, label);
            entry.keywords.push(english);
        }
    }

    /// Opens the menu if a `/` was just typed at the start of a line or
    /// after whitespace, in a block that takes block commands.
    pub(in crate::editor) fn maybe_open_slash(&mut self, typed: &str) {
        if typed != "/" || !self.options.ui.slash_menu || self.state.marked.is_some() {
            return;
        }
        let Selection::Text { anchor, head } = self.state.selection.clone() else {
            return;
        };
        if anchor != head || head.offset == 0 {
            return;
        }
        let Some(order::Loc::Block(path)) = self.state.locate(head.block) else {
            return;
        };
        let literal = order::block_at(&self.state.doc.blocks, &path)
            .is_none_or(|b| order::is_literal(&b.kind));
        let Some(text) = self.state.text_of(head.block).map(|t| t.text.as_str()) else {
            return;
        };
        // An input rule may have rewritten the typed character: the slash
        // must still be right before the caret.
        let start = head.offset - 1;
        if literal || text.get(start..head.offset) != Some("/") {
            return;
        }
        let after_space = text[..start]
            .chars()
            .next_back()
            .is_none_or(char::is_whitespace);
        if !after_space {
            return;
        }
        self.slash = Some(SlashMenu {
            block: head.block,
            start,
            entries: self.slash_entries(),
            selected: 0,
            scroll: 0,
        });
    }

    /// The typed query, or `None` if the menu no longer applies: the caret
    /// left the `/query`, or the slash is gone.
    fn slash_query(&self) -> Option<String> {
        let menu = self.slash.as_ref()?;
        let Selection::Text { anchor, head } = &self.state.selection else {
            return None;
        };
        if anchor != head || head.block != menu.block || head.offset <= menu.start {
            return None;
        }
        let text = &self.state.text_of(menu.block)?.text;
        if text.get(menu.start..menu.start + 1) != Some("/") {
            return None;
        }
        let query = text.get(menu.start + 1..head.offset)?;
        (!query.contains('\n')).then(|| query.to_string())
    }

    /// Filters the menu after an edit or caret move, or closes it.
    pub(in crate::editor) fn refresh_slash(&mut self) {
        let Some(query) = self.slash_query() else {
            self.slash = None;
            return;
        };
        let entries = filter(self.slash_entries(), &query);
        // A space after a query without results ends the menu, as in Notion.
        if entries.is_empty() && query.ends_with(' ') {
            self.slash = None;
            return;
        }
        if let Some(menu) = &mut self.slash {
            menu.entries = entries;
            menu.selected = 0;
            menu.scroll = 0;
        }
    }

    /// Moves the selection in the menu by `delta`, wrapping around.
    pub(in crate::editor) fn slash_move(&mut self, delta: isize) {
        let Some(menu) = &mut self.slash else {
            return;
        };
        if menu.entries.is_empty() {
            return;
        }
        menu.selected = wrap(menu.selected, delta, menu.entries.len());
        if menu.selected < menu.scroll {
            menu.scroll = menu.selected;
        } else if menu.selected >= menu.scroll + VISIBLE {
            menu.scroll = menu.selected + 1 - VISIBLE;
        }
    }

    /// Runs entry `index` of the menu: removes the `/query`, then applies
    /// the entry.
    pub(in crate::editor) fn slash_choose(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(menu) = self.slash.take() else {
            return;
        };
        let Some(entry) = menu.entries.get(index).cloned() else {
            return;
        };
        if let Some(head) = self.state.selection.head() {
            let start = TextPos {
                block: menu.block,
                offset: menu.start,
            };
            self.state.set_selection(Selection::Text {
                anchor: start,
                head,
            });
            self.state.delete_selection();
        }
        // Each step is its own undo entry: undo first reverts the block
        // change, then brings the typed text back.
        self.state.history.seal();
        match entry.action {
            SlashAction::Block(kind) => {
                let current = self
                    .state
                    .current_block()
                    .and_then(|id| self.state.doc.block(id))
                    .map(|b| b.kind().clone());
                // `SetBlock` toggles back to a paragraph if the block
                // already has the kind.
                let same = current.is_some_and(|c| match (&c, &kind) {
                    (BlockKind::Heading(a), BlockKind::Heading(b)) => a == b,
                    _ => std::mem::discriminant(&c) == std::mem::discriminant(&kind),
                });
                if !same {
                    self.state
                        .exec(&Command::SetBlock(kind), std::time::Instant::now());
                }
            }
            SlashAction::Command(command) => {
                self.state.exec(&command, std::time::Instant::now());
            }
            SlashAction::Text(text) => {
                self.state.type_text(text, std::time::Instant::now());
            }
            SlashAction::Image => {
                self.after_change(false, window, cx);
                self.open_image_prompt(window, cx);
                return;
            }
            SlashAction::Host(handler) => {
                self.after_change(false, window, cx);
                handler(self, window, cx);
                return;
            }
        }
        self.after_change(false, window, cx);
    }

    /// The menu below (or above) the slash.
    pub(in crate::editor) fn render_slash(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let menu = self.slash.as_ref()?;
        let palette = Palette::new(cx);
        let ts = window.text_system().clone();
        let anchor = self
            .view
            .query(self.layout_input(), &ts)
            .bounds_for_range(menu.block, menu.start..menu.start + 1)?;
        let shown: Vec<(usize, &SlashEntry)> = menu
            .entries
            .iter()
            .enumerate()
            .skip(menu.scroll)
            .take(VISIBLE)
            .collect();
        let height = shown
            .iter()
            .map(|(_, e)| {
                if e.description.is_some() {
                    ROW_HEIGHT_DESCRIBED
                } else {
                    ROW_HEIGHT
                }
            })
            .fold(px(12.), |a, b| a + b)
            .max(px(46.));
        let position = place(anchor, size(WIDTH, height), self.view.bounds(), false);
        let count = menu.entries.len();
        let rows = shown.into_iter().map(|(index, entry)| {
            menu_row(
                SharedString::from(format!("slash-{}", entry.id)),
                &palette,
                gpui::Role::MenuItem,
                entry.label.clone(),
                (index, count),
                index == menu.selected,
            )
            .flex_col()
            .justify_center()
            .h(if entry.description.is_some() {
                ROW_HEIGHT_DESCRIBED
            } else {
                ROW_HEIGHT
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.slash_choose(index, window, cx);
                }),
            )
            .child(div().text_size(px(13.5)).child(entry.label.clone()))
            .children(
                entry
                    .description
                    .clone()
                    .map(|d| div().text_size(px(11.5)).text_color(palette.muted).child(d)),
            )
        });
        let list = menu_panel(
            &palette,
            "slash-menu",
            gpui::Role::Menu,
            self.tr("slash.menu"),
            WIDTH,
        )
        .children(rows)
        .when(menu.entries.is_empty(), |s| {
            s.child(
                div()
                    .px(px(12.))
                    .py(px(8.))
                    .text_size(px(13.))
                    .text_color(palette.muted)
                    .child(self.tr("slash.empty")),
            )
        });
        Some(overlay(position, appear(list, "slash-menu", false)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(query: &str) -> Vec<String> {
        filter(builtin(), query)
            .into_iter()
            .map(|e| e.label.to_string())
            .collect()
    }

    #[test]
    fn filters_by_label_words_and_keywords() {
        assert_eq!(labels("").len(), builtin().len());
        assert_eq!(labels("head"), ["Heading 1", "Heading 2", "Heading 3"]);
        assert_eq!(labels("list")[..2], ["Bullet list", "Numbered list"]);
        assert_eq!(labels("todo"), ["Checklist"]);
        assert_eq!(labels("H2"), ["Heading 2"]);
        assert_eq!(labels("TAB"), ["Table"]);
        assert!(labels("zzz").is_empty());
    }
}
