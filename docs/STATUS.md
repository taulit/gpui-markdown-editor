# Status

Read by other sessions. Keep terse.

## Tags

- `v0.0.1-api` (branch `phase/e1-api-model`): the public API compiles and is documented. Depend on it with
  `gpui-markdown-editor = { git = "https://github.com/taulit/gpui-markdown-editor", tag = "v0.0.1-api" }`.
  The view edits the Markdown source as plain text for now; `markdown()`, `set_markdown()`, `is_dirty()`,
  `mark_saved()`, `find()`, `word_count()` and `EditorEvent::Changed` work. Everything else is accepted and
  stored without effect (see "Status" in the crate docs). Call `gpui_component::init(cx)` before creating an editor.

## Done

- E0: repo, toolchain, CI, versions pinned, assumptions verified (`docs/verified.md`).
- E1 (part): public API skeleton, document model, offscreen screenshot harness (`scripts/screenshot.sh`).

## In progress

- E1: lossless Markdown parser and serializer, round-trip corpus.

## Open

- E2: rendering, cursor, input, IME, marks, lists, input rules, shortcuts.
- E3: tables, images, paste, drag, slash menu, format bar, find, folding, extensions.
- E4: benchmarks, docs, README GIF, reviews, tag v0.1.0.

## Versions for downstream

- gpui-pre =0.3.12
- gpui-component =0.10.0
- gpui-pre-platform =0.3.12 for the application itself (`gpui_platform::application()`)
- Rust edition 2024, MSRV 1.95
