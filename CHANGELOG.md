# Changelog

All notable changes are documented here, following [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
API changes between tags are listed here because downstream (Sheaf) pins tags.

## [0.3.0]

- **Behaviour change:** single line breaks inside a paragraph are kept.
  New option `EditorOptions::line_breaks: LineBreaks` with the default
  `LineBreaks::Keep`: a line break is shown as one and an edited
  paragraph writes it back as a plain line break, as Typora and Obsidian
  do. Before, it read as a space (CommonMark) and editing a paragraph
  joined its lines. `LineBreaks::Join` restores that.
  `Document::parse_with_line_breaks` and `Document::line_breaks` are new;
  `Document::parse` and `parse_with` still read CommonMark (`Join`).
  Copied Markdown always marks its breaks (`\` at the end of the line).
- New `EditorStyle::bionic_reading: bool` (default `false`): a reading
  aid that draws the first part of every word bold. Display only; code
  is left alone.
- Roomier defaults of `EditorStyle::from_theme`: `paragraph_spacing` 6 →
  12 px, `heading_spacing_above` 22 → 28 px, `heading_spacing_below`
  6 → 8 px.
- Like `strings` in 0.2.0, struct literals of `EditorOptions` and
  `EditorStyle` without `..` no longer compile.

## [0.2.0] - 2026-11-22

- The built-in UI is translated: English, German, Spanish, French,
  Italian, Japanese, Polish, Brazilian Portuguese, Russian and simplified
  Chinese. New option `EditorOptions::strings: EditorStrings` (default
  English) with `EditorStrings::from_env`, `for_locale`, `set` (one text
  by key, like Tiptap's `configure`), `translate` (fill from the host's
  catalogs), `keys` and `locales`; `MarkdownEditor::set_strings` and
  `strings` switch and read them at runtime. Slash entries are keyed
  `slash.<id>`, including those of extensions and `slash_items`, and keep
  their English label as a keyword. Like `input_rules` in 0.1.0, a struct
  literal of `EditorOptions` without `..Default::default()` no longer
  compiles.

## [0.1.0]

Release preparation (phase E5).

- New option `EditorOptions::input_rules: bool` (default `true`). With
  `false`, typed Markdown such as `# ` or `**x**` stays literal text,
  including the input rules of extensions. The slash menu and the `[[`/`#`
  completions are not affected; they keep their own switches
  (`UiParts::slash_menu`, `EditorOptions::completions`).
  `EditorOptions` is not `#[non_exhaustive]`, so a struct literal that
  lists every field without `..Default::default()` no longer compiles;
  literals ending in `..Default::default()` (as documented) are
  unaffected.
- **File format change** relative to 0.0.3 and 0.0.4: empty paragraphs
  are no longer written. They exist only as places to type; the
  serializer leaves them out everywhere (between blocks, inside quotes
  and list items), where 0.0.3/0.0.4 wrote `&#32;` on its own line. The
  blocks around a dropped paragraph are written as neighbours: `- a`,
  an empty paragraph, `- c` (Enter in an empty middle item) is saved as
  `- a\n- c` and reads back as one list. The parser no longer treats a
  lone `&#32;` specially: in files written by 0.0.3/0.0.4 it reads as a
  paragraph containing one space, as CommonMark defines. Unedited files
  still round-trip byte for byte, and undoing every edit restores them.
  Adding only an empty paragraph makes the editor dirty (`is_dirty`)
  although `markdown()` is unchanged.
- Extensions contribute marks only (parser, serializer, `mark_style`,
  `input_rule`, `click`, `slash_items`); their own inline nodes and block
  renderers are not supported. Documented on `Extension`, in the crate
  guide and the README.

Benchmarks, motion and documentation (phase E4, part 1).

- Popups (slash menu, completions, link popover and bubble, style
  popover, find bar, table menu) fade in and slide 6 px into place over
  250 ms ease-out. A checkbox pressed with the mouse springs to 85 % of
  its size and back (stiffness 300, damping 26). Both follow GPUI's
  `App::set_reduce_motion` and request no frames once settled.
- Criterion benchmarks (`cargo bench`): parse and serialize of 10 KB and
  1 MB, `html_to_markdown` on 100 KB, a keystroke in the editing core and
  with a frame in a 10,000-block document, opening and the first layout
  of 1 MB, find over 1 MB. Results and budgets in `docs/performance.md`.
  CI compiles the benchmarks.
- Crate documentation rewritten as a guide (loading and events, options,
  commands, a complete extension, `DocMeta` persistence, images and
  attachments, keys and rebinding, round-trip rules, motion, known
  limits); README with screenshots, shortcuts and a comparison with
  gpui-component; `docs/release-checklist.md`; `examples/headless.rs`.
- API: no changes. `gpui_markdown_editor::bench` is `#[doc(hidden)]`, for
  the benchmarks only and not covered by semver.

## [0.0.4]

Tables, images and the editor UI (phase E3).

After review (part 3):

- New key bindings in the context `MarkdownEditor && table` (active with
  the caret in a table cell; Cmd instead of Ctrl on macOS):
  Ctrl+Alt+Up/Down insert a row above/below, Ctrl+Alt+Left/Right insert a
  column left/right, Ctrl+Alt+Shift+Left/Right move the column,
  Ctrl+Alt+Backspace delete the row, Ctrl+Alt+Shift+Backspace delete the
  column, Shift+F10 open the row menu at the caret's cell (again: the
  column menu). Alt+Up/Down move the row instead of the table. Actions
  `TableInsertRowAbove`, `TableInsertRowBelow`, `TableInsertColumnLeft`,
  `TableInsertColumnRight`, `TableDeleteRow`, `TableDeleteColumn`,
  `TableMoveColumnLeft`, `TableMoveColumnRight`, `TableMenu`.
- Attachments (plan §9): a paragraph that is only a link to a local file
  that is not an image is drawn as an attachment row (file icon, link text
  as name, file size when `resolve_src` resolves it). A click on the row
  outside the name, or Ctrl/Cmd+click on the name, emits
  `EditorEvent::AttachmentClicked` with the link target. Dropped files that
  are not images are inserted as such paragraphs linking to the dropped
  path (§1a defines no host hook for them). Before, other dropped files
  were ignored.
- Folding: the caret and node selections never end up in a folded
  section (movement skips hidden rules, images and tables too). An edit
  that would hide visible content unfolds the section instead: Enter at
  the end of a folded heading, a block dropped after a folded section, a
  heading that ended a folded section turned into content of it.
- `html_to_markdown`: nesting deeper than 64 elements is flattened and
  input beyond 8 MiB ignored (no stack overflow); HTML's implied end tags
  (`<li>a<li>b`, `<td>1<td>2`, `<p>a<p>b`, `dt`/`dd`, `tr`, table sections,
  `option`) give siblings; text that would read as Markdown syntax (`- x`,
  `1. x`, `: x`, `==x==`, ...) is escaped.
- Images: local files are read only if they are regular files of at most
  64 MB (a FIFO or `/dev/zero` fails into the placeholder at once);
  decoders refuse images over 16384 px a side or 256 MB of memory. http(s)
  images load through the editor's own loader with the same caps (the
  body is read up to 64 MB) and show their first frame.
- Dropped image files are read on the background executor, with the same
  cap.
- Column widths follow column inserts, deletes and moves from every place
  (format bar, menu, keys) and their undo and redo, also undoing the
  deletion of a whole table.
- Fixed a panic when an extension's input rule rewrote a typed `/` or `#`.
- Tables: a column without an alignment whose body cells are all numbers
  (digits with separators, sign, currency or percent) is right-aligned;
  table text uses tabular figures (`tnum`).
- `highlight` scrolls the first range into view (unfolding a folded
  section that hides it) without moving the caret.
- The popup shadow has alpha 0.5 as in the design.
- `EditorStyle::from_theme`: links, wiki links and tags use the accent
  (the theme's `primary` color) instead of the theme's `link` color.

API changes (part 2, editor UI):

- `Extension` has four new default methods: `mark_style` (new types
  `MarkStyle` and `MarkColor`), `input_rule`, `click` and `slash_items`
  (new type `extension::SlashEntry`). The renderer and Ctrl/Cmd+click no
  longer special-case `WikiLink` and `Tag`; the bundled extensions
  implement the hooks.
- New `markdown::html_to_markdown` (feature `paste-html`). GPUI 0.3.12
  offers no HTML clipboard format, so the editor's paste never sees HTML;
  hosts that read HTML themselves insert the result with
  `Command::InsertMarkdown`.
- `Command::OpenFind` and `Command::FoldHeading` work.
  `EditorOptions::completions`, `EditorOptions::slash_items` and every
  switch of `UiParts` are honored. `DocMeta::folded_headings` is written
  (with `MetaChanged`) and restored by `set_meta` and `set_markdown`.
- New key bindings in the context `MarkdownEditor`: Ctrl/Cmd+F (find),
  F3 and Shift+F3 (next/previous match), Ctrl/Cmd+. (fold the heading at
  the caret). Ctrl/Cmd+K opens the link popover; only without
  `UiParts::link_popover` does it still link to the clipboard URL. The
  popups have their own key contexts `MarkdownEditorFind` and
  `MarkdownEditorPrompt`.
- Without the cargo feature `ui` all UI parts are off.

API changes (part 1, tables and images):

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

What works (part 2):

- Find bar (Ctrl/Cmd+F): query field, "3 of 12", previous/next with Enter,
  Shift+Enter, F3, Shift+F3 and buttons, case toggle (Alt+C). Matches are
  painted like host highlights, the current one stronger, and scrolled into
  view; a folded section with the current match is unfolded. Escape
  selects the current match and returns the focus. Text selected in one
  block becomes the query. No replace: the plan does not ask for it.
- Slash menu: `/` at the start of a line or after whitespace lists text,
  headings 1 to 3, bullet, numbered and check lists, quote, code block,
  divider, table and image, then extension and host entries, filtered by
  the typed text. Up/Down, Enter/Tab, Escape and click; the `/query` is
  removed first, so undo brings it back.
- Format bar: floats at the bottom center as in the Sheaf prototype and
  adapts to the selection (insert tools at a caret, marks and link for
  text, list/indent/quote over several blocks, row and column tools in a
  table); mark buttons reflect the selection; hidden while unfocused,
  while the mouse selects and in code blocks. "Aa" opens the style popover.
- Link popover (Ctrl/Cmd+K, the link button): URL field prefilled with
  the current link or a clipboard URL; Enter applies in one undo step,
  Escape cancels, Remove (or an empty field) unlinks. With the caret in a
  link a bubble offers Open, Edit and Remove. The same popover asks for
  the source of an image from the slash menu or the format bar.
- Completions: `[[` and `#` open the host's suggestions from
  `EditorOptions::completions`.
- Folding: a chevron left of a top-level heading (on hover and while
  folded) or Ctrl/Cmd+. folds everything up to the next heading of the
  same or a higher level. Folded blocks are not laid out and the caret
  skips them.
- Block drag: a handle left of the hovered block moves it (with its
  children, a folded heading with its section) to the insertion line, in
  one undo step. Image files dropped onto the editor go through
  `on_image`.
- Extensions: `[[Note]]` and `#tag` are marked while typing (one undo
  step), "Link to note" and "Tag" in the slash menu.
- Accessibility: the document is an AccessKit node with role Document;
  visible blocks are synthetic leaves with their role (heading level,
  checkbox state with the Click action) and text; popups carry roles and
  labels. The table menu works with the keyboard. GPUI's synthetic nodes
  are leaves, so caret and selection are not reported as text positions.

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
