# Changelog

All notable changes are documented here, following [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
API changes between tags are listed here because downstream (Sheaf) pins tags.

## [Unreleased]

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

## [0.0.2]

No API changes since `v0.0.1-api`.

- `Document::parse` is a real parser now (CommonMark with GFM tables, task lists and strikethrough). Constructs the editor does not model stay `Raw` blocks or `Raw` marks.
- `Document::to_markdown` copies unedited blocks from the source and writes edited blocks in a fixed, normalized style.
- The `WikiLink`, `Tag` and `Highlight` extensions parse and write their syntax.

## [0.0.1-api]

- Public API skeleton (`MarkdownEditor`, `EditorOptions`, `Command`, `EditorEvent`, `EditorStyle`, `DocMeta`, `Extension`) and the document model. The parser is a placeholder that keeps the source as one raw block. Editing is plain-text for now.
