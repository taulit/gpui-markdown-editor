# Status

Read by other sessions. Keep terse.

## Tags

- `v0.0.1-api` (branch `phase/e1-api-model`): the public API compiles and is documented. Depend on it with
  `gpui-markdown-editor = { git = "https://github.com/taulit/gpui-markdown-editor", tag = "v0.0.1-api" }`.
  The view edits the Markdown source as plain text for now; `markdown()`, `set_markdown()`, `is_dirty()`,
  `mark_saved()`, `find()`, `word_count()` and `EditorEvent::Changed` work. Everything else is accepted and
  stored without effect (see "Status" in the crate docs). Call `gpui_component::init(cx)` before creating an editor.

- `v0.0.2` (branch `phase/e1-api-model`): same API, real lossless parser and serializer, `WikiLink`/`Tag`/`Highlight`
  extensions work. Prefer this tag if you use `Document` headless (outline, open tasks, tags).

- `v0.0.3` (branch `phase/e2-editing`): WYSIWYG editing. Same API plus `gpui_markdown_editor::init(cx)` (key
  bindings; call after `gpui_component::init`, `MarkdownEditor::new` calls it if forgotten). Keyboard, mouse, IME,
  clipboard (Markdown text), input rules, shortcuts, undo/redo. `is_dirty` follows the undo history. Tables render
  read-only, images are placeholders, `FoldHeading`/`OpenFind` do nothing yet. See `CHANGELOG.md`.

- `v0.0.4` (branch `phase/e3-blocks-ui`): table editing (cells, rectangle selection, row/column menus and keys,
  column widths in `DocMeta`), images (`resolve_src`, `on_image`, resize), attachments (`AttachmentClicked`),
  find bar, slash menu, format bar, link popover, heading folding (`DocMeta`), block drag, extension hooks
  (`mark_style`, `input_rule`, `click`, `slash_items`), `completions`, accessibility roles. `MetaChanged` is emitted.
  Signatures unchanged; additions are listed in `CHANGELOG.md`. Prefer this tag.

- `v0.1.0` (branch `phase/e5-release-0.1.0`): first complete release. Against `v0.0.4`: new
  `EditorOptions::input_rules` (breaks struct literals without `..Default::default()`), empty paragraphs are no
  longer written (`&#32;` is gone), popup and checkbox motion, benchmarks, crate guide.

- `v0.2.0` (branch `feat/ui-translation`): the built-in UI is translated. New `EditorOptions::strings`
  (`EditorStrings`: `from_env`, `for_locale`, `set`, `translate`; breaks struct literals without
  `..Default::default()`), `MarkdownEditor::set_strings`. Ten languages built in. Prefer this tag.

## Done

- E0: repo, toolchain, CI, versions pinned, assumptions verified (`docs/verified.md`).
- E1: public API skeleton, document model, lossless parser and serializer, round-trip corpus (1354 spec examples and 33 notes byte-identical), offscreen screenshot harness (`scripts/screenshot.sh`).

- E2: editing core, rendering element, keyboard, mouse, IME, clipboard, input rules, shortcuts, interaction tests.
  Not done: manual IME check on a desktop (dead keys, Compose, ibus-pinyin); simulated only.

- E3: tables, images, attachments, find, slash menu, format bar, link popover, folding, block drag, extension
  hooks, accessibility roles. Bug, spec and simplicity reviews done and their findings fixed.

## Open

- E4 (branch `phase/e4-release`, PR open): benchmarks (`docs/performance.md`), motion, crate guide, README,
  release checklist. No API changes since `v0.0.4`. Tag `v0.1.0` follows after the decisions below.
- E5 (branch `phase/e5-release-0.1.0`): the former open decisions are settled: `EditorOptions::input_rules` added,
  `Changed` stays without payload (plan §3 fixed), extensions add marks only (documented), empty paragraphs are not
  saved (`&#32;` dropped).

## Versions for downstream

- gpui-pre =0.3.12
- gpui-component =0.10.0
- gpui-pre-platform =0.3.12 for the application itself (`gpui_platform::application()`)
- Rust edition 2024, MSRV 1.95
