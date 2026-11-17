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

## Done

- E0: repo, toolchain, CI, versions pinned, assumptions verified (`docs/verified.md`).
- E1: public API skeleton, document model, lossless parser and serializer, round-trip corpus (1354 spec examples and 33 notes byte-identical), offscreen screenshot harness (`scripts/screenshot.sh`).

- E2: editing core, rendering element, keyboard, mouse, IME, clipboard, input rules, shortcuts, interaction tests.
  Not done: manual IME check on a desktop (dead keys, Compose, ibus-pinyin); simulated only.

## Open

- E3: tables, images, paste, drag, slash menu, format bar, find, folding, extensions.
- E4: benchmarks, docs, README GIF, reviews, tag v0.1.0.

## Versions for downstream

- gpui-pre =0.3.12
- gpui-component =0.10.0
- gpui-pre-platform =0.3.12 for the application itself (`gpui_platform::application()`)
- Rust edition 2024, MSRV 1.95
