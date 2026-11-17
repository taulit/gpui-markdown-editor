# Performance

Budgets from the plan (`docs/design/plan/05-quality-release.md` §1 and
`02-gpui-markdown-editor.md` §0), measured with the criterion benchmarks in
`benches/`.

## Results

Measured 2026-11-17 in release mode on an Intel Core i5-11400F (6 cores,
12 threads, 2.6 GHz base), 16 GB RAM, Linux 6.18, rustc 1.99. The machine
was shared with other work (load average 7 to 11 during the run), so the
numbers carry noise; an earlier run under heavier load (load average 10
to 17) is given as the upper value where it differed much.

| Benchmark | Budget | Median | Under load |
|---|---|---|---|
| Keystroke + frame, 10,000 blocks (`editor/keystroke_and_frame/10k_blocks`) | < 4 ms | 1.5 ms (0.9 ms in a quieter run) | 2.6 to 4.4 ms |
| Open 1 MB: `set_markdown` + first frame (`editor/open/1MB`) | < 50 ms | 31 ms | 57 to 66 ms |
| First layout of 1 MB, after `set_markdown` (`editor/first_layout/1MB`) | (part of open) | 0.49 ms | 1.1 ms |
| Keystroke in the editing core, 10,000 blocks (`core/keystroke/10k_blocks`) | (part of keystroke) | 1.4 µs | 1.8 µs |
| Find over 1 MB (`editor/find/1MB`) | none | 2.2 ms | 5.1 ms |
| Parse 10 KB (`markdown/parse/10KB`) | none | 0.25 ms | 0.6 ms |
| Parse 1 MB (`markdown/parse/1MB`) | none | 37 ms | 36 ms |
| Serialize 10 KB, unchanged (`markdown/serialize_unchanged/10KB`) | none | 20 µs | 32 µs |
| Serialize 1 MB, unchanged (`markdown/serialize_unchanged/1MB`) | none | 3.1 ms | 3.7 ms |
| Serialize 10 KB, every block rewritten (`markdown/serialize_normalized/10KB`) | none | 0.82 ms | 0.85 ms |
| Serialize 1 MB, every block rewritten (`markdown/serialize_normalized/1MB`) | none | 41 ms | 60 ms |
| `html_to_markdown`, 100 KB (`html/html_to_markdown/100KB`) | none | 5.9 ms | 7.6 ms |

Both budgets are met on this machine when it is not saturated. Under heavy
load from other processes the keystroke stayed at or just above 4 ms and
opening 1 MB went over 50 ms; parsing (about 35 ms, of which
pulldown-cmark itself is about 15 ms) dominates opening. Opening skips the
fold and column-width walks when the host has no stored `DocMeta`, which
took about 3 ms of the 1 MB case.

The 1 MB parse in the table ran later than the open benchmark, at a higher
load, which is why it is not below the open time that includes it.

## What is measured

- **Documents** are generated (`benches/common/mod.rs`): sections with a
  heading, a paragraph with marks, a link, a wiki link and a tag, a nested
  list, a checklist, a quote, a code block and a table, repeated to the
  size. The 10,000-block document alternates headings, paragraphs and
  three-item lists (10,000 top-level blocks, about 25,000 lines). The
  extensions `WikiLink`, `Tag` and `Highlight` are on.
- **Keystroke + frame:** one character typed in the middle of the document
  through GPUI's input path (key dispatch, input handler, editing core,
  input rules, history, events), then one frame: layout of the changed
  block and the visible ones with the platform's real text system (font
  shaping included) and recording the scene. The character is deleted
  again outside the timing so the paragraph keeps its length.
- **Open:** `set_markdown` (parse, editing state with its path index) and
  the first frame in an 800x600 window, replacing the previous 1 MB
  document (so freeing it is included).
- **Editing core:** the same keystroke without the view: transaction,
  input rules, history and change set.
- **Find:** `MarkdownEditor::find` for a word that occurs once per section (about 2,300 matches).

**Not included:** GPU work. The benchmarks run in GPUI's headless test
window, which records the scene but does not rasterize it, and there is
no compositor or vsync. Image decoding (off the UI thread) and window
system input latency are not measured either.

## Reproduce

```sh
cargo bench --bench markdown        # parser, serializer, HTML, editing core
cargo bench --bench editor          # keystroke + frame, open, find
cargo bench --bench editor -- open  # one benchmark by name
```

Close other heavy programs first: the numbers above show how much a busy
machine moves them. The older end-to-end timing tests also still exist:
`cargo test --release --lib timing -- --ignored --nocapture`.
