# Changelog

All notable changes are documented here, following [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
API changes between tags are listed here because downstream (Sheaf) pins tags.

## [Unreleased]

## [0.0.2]

No API changes since `v0.0.1-api`.

- `Document::parse` is a real parser now (CommonMark with GFM tables, task lists and strikethrough). Constructs the editor does not model stay `Raw` blocks or `Raw` marks.
- `Document::to_markdown` copies unedited blocks from the source and writes edited blocks in a fixed, normalized style.
- The `WikiLink`, `Tag` and `Highlight` extensions parse and write their syntax.

## [0.0.1-api]

- Public API skeleton (`MarkdownEditor`, `EditorOptions`, `Command`, `EditorEvent`, `EditorStyle`, `DocMeta`, `Extension`) and the document model. The parser is a placeholder that keeps the source as one raw block. Editing is plain-text for now.
