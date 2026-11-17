# Changelog

All notable changes are documented here, following [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
API changes between tags are listed here because downstream (Sheaf) pins tags.

## [Unreleased]

Table editing and images (phase E3, part 1).

API changes:

- Escape is bound in the key context `MarkdownEditor` (action `Cancel`). It
  closes an open table menu and is passed on to the host otherwise.
- Tables are as wide as their content (each column between 48 and 320 px)
  instead of filling the content column.
- `EditorEvent::MetaChanged` is emitted when the user resizes a table
  column, and when tables with stored widths are added, removed or have
  columns inserted, moved or removed. `set_meta` restores the widths.
- `EditorOptions::resolve_src` and `EditorOptions::on_image` are called.

What works:

- Cell editing: clicks and Up/Down land in the cell under the pointer;
  typing, marks and selection in cells as in paragraphs.
- Cell rectangles: dragging across cells selects a rectangle; copy gives a
  Markdown table of it, Delete and Backspace empty its cells in one undo
  step, typing or pasting replaces the text of the anchor cell, arrows
  collapse it.
- Table structure: "+" bars below and right of the table append a row or
  column; handles at the focused row and column open a menu (insert
  before/after, move, delete, and alignment for columns). Every change is
  one undo step.
- Column widths: drag a column border; the width is kept in
  `DocMeta::column_widths`, not in the Markdown.
- Images: loaded asynchronously (local files through `resolve_src`,
  absolute paths without a resolver, http(s) URLs), decoded and scaled down
  to at most 1600 px off the UI thread, only near the viewport. Shown with
  radius 12, scaled to fit the content width; the placeholder with the alt
  text stays while loading and on failure. A selected image shows resize
  handles; dragging one stores the width as `![alt|width](src)`.
- Pasting image data calls `on_image` and inserts the returned source as an
  image block.

Fixed:

- Clicking the left half of the last character of a line placed the caret
  after it.

## [0.0.3]

The editor edits the rendered document (WYSIWYG) instead of the Markdown
source.

API changes:

- New `gpui_markdown_editor::init(cx)` registers the key bindings (key
  context `MarkdownEditor`). Call it at startup after `gpui_component::init`;
  `MarkdownEditor::new` calls it if you forget.
- `is_dirty` follows the undo history: undoing back to the saved state makes
  the editor clean again.
- `Command::SelectAll` selects the whole document; Ctrl+A selects the block
  first.
- Ctrl+click (Cmd+click on macOS) on a link, wiki link or tag emits
  `LinkClicked`, `WikiLinkClicked` or `TagClicked`; a plain click places the
  caret.
- `Command::InsertRule` no longer adds a blank line after the rule to the
  Markdown: an empty paragraph at the start or end of the document is not
  written.
- Removed the hidden preview switches `__set_rich_preview` and
  `__set_rich_selection` (`__set_selection` stays hidden, for screenshots).

What works:

- Typing with the Markdown input rules (headings, lists, checklists, quotes,
  code blocks, rules, bold, italic, strike, code, highlight); Backspace right
  after a rule undoes it.
- Enter, Shift+Enter, Tab and Shift+Tab in paragraphs, lists, checklists,
  code blocks and table cells.
- The shortcuts of the plan: marks, Ctrl+K link with the clipboard URL,
  checklist, headings, checkbox, moving blocks, undo and redo.
- Caret movement by characters, words, visual lines, pages and the document,
  with Shift to select across blocks; a blinking caret kept in view.
- Mouse selection by click, Shift+click, drag with scrolling at the edges,
  double and triple click; checkbox clicks; node selection of rules and images.
- IME composition with underlined marked text and the candidate window at the
  caret; stale ranges are clamped.
- Copy and cut as Markdown text; paste of Markdown, plain text, and URLs over a
  selection as links.
- All commands except `FoldHeading` and `OpenFind`; `highlight` is painted,
  `scroll_to` scrolls.
- Fixed: Ctrl+Alt+2 on a level-1 heading made it a paragraph instead of a
  level-2 heading.

Markdown output:

- An empty paragraph between two blocks is written as `&#32;` on its own
  line and read back as an empty paragraph, because Markdown has no empty
  paragraph. Files show this after pressing Enter twice between blocks.
- Bold, italic and strike leave whitespace at the edges of the selection
  unmarked, so the result reads back with the same marks.
- Copying part of one block copies inline Markdown without the block marker;
  spaces at the edges of copied text stay plain.

## [0.0.2]

No API changes since `v0.0.1-api`.

- `Document::parse` is a real parser now (CommonMark with GFM tables, task lists and strikethrough). Constructs the editor does not model stay `Raw` blocks or `Raw` marks.
- `Document::to_markdown` copies unedited blocks from the source and writes edited blocks in a fixed, normalized style.
- The `WikiLink`, `Tag` and `Highlight` extensions parse and write their syntax.

## [0.0.1-api]

- Public API skeleton (`MarkdownEditor`, `EditorOptions`, `Command`, `EditorEvent`, `EditorStyle`, `DocMeta`, `Extension`) and the document model. The parser is a placeholder that keeps the source as one raw block. Editing is plain-text for now.
