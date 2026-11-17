# Handoff (session 2, editor)

Written 2026-11-17 so that work can resume without the session's context. Plan: `docs/design/plan/`
(README.md "Sessions", "Arbeitsweise", "Pull Requests"; 02 is the spec). Status for other sessions: `docs/STATUS.md`.

## Done

- **E0** merged to `main` (PR #1): scaffold, CI, `docs/verified.md`.
- **E1** merged to `main` (PR #3; PR #2 went into `phase/e0-setup` by mistake and is superseded): public API,
  lossless parser/serializer, extensions, round-trip corpus. Tags `v0.0.1-api`, `v0.0.2`.
- **E2** on `phase/e2-editing` (rebased on `main`, pushed): editing core `src/edit/`, rendering `src/view/`,
  wiring `src/editor/` (keyboard, mouse, IME, clipboard), 30 interaction tests, `examples/notes.rs`,
  screenshots `docs/screenshots/e2-*.png`. Gate checks were green before the review fixes
  (fmt, clippy, 180 lib tests + integration tests, rustdoc, cargo-deny).

## Running

- E2 PR: https://github.com/taulit/gpui-markdown-editor/pull/4 (tag `v0.0.3`), waiting for the user to merge.
- E3 on `phase/e3-blocks-ui` (stacked on `phase/e2-editing`). Part 1, a sub-agent: table editing and images
  (commits `feat(table): …`, `feat(image): …`). Part 2, not started: slash menu, format bar, link popover,
  find bar, folding, HTML/image paste, drag, extension hooks, accessibility roles. Run the parts one after the
  other; they touch the same files.

## Next

0. Steps 1 and 2 are done; continue at 3 once the E2 PR is merged (rebase E3 onto `main` then).
1. Verify the fixes: `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings`,
   `cargo test --all-features`, `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features`, `cargo deny check`,
   `scripts/screenshot.sh` (look at the PNGs).
2. Open the E2 PR against `main` (template in `.github/`, screenshots light and dark, measured keystroke time:
   0.35 ms median in a 10,000-line document, budget 4 ms, reported by the wiring agent, not re-measured).
   Bump the version, add a CHANGELOG entry and tag (e.g. `v0.0.3`) so Sheaf gets the WYSIWYG editor; update `docs/STATUS.md`.
3. E3 on `phase/e3-…` branched from E2: tables (editing, handles, column widths → `DocMeta`), images (loading,
   resize, `resolve_src`, `on_image`), HTML/image paste, drag, slash menu, format bar, link popover, find bar,
   folding, extension hooks, accessibility roles (AccessKit is available). Gate: all features in `examples/notes.rs`.
   One bug-review, plus simplicity-review and spec-review.
4. E4: criterion benchmarks, `docs/performance.md`, README with GIF, docs, reviews, `docs/release-checklist.md`, tag `v0.1.0`.

## Open, known and accepted for now

- An empty paragraph between blocks is saved as `&#32;` (Markdown has no empty paragraph). Open question for the
  user: keep, or drop empty paragraphs on save.

- Enter in a quote starts a second quote instead of a new paragraph in the same quote (plan does not specify).
- An edited paragraph that was hard-wrapped in the source is written on one line.
- Up/Down skip rules and images (Left/Right select them).
- Manual IME check on a real desktop (dead keys, Compose, ibus-pinyin) not done; only simulated.
- gpui-pre 0.3.12 on Linux loads variable fonts at normal weight only (screenshots use Noto Sans); affects Sheaf.
- Inline code is drawn at body size, no letter-spacing for the title (gpui text limits).
- Screenshots in PRs are embedded from the repo, not via an external image host.

## Working rules agreed with the user

- Be frugal: implementation sub-agents on Opus, mechanical work on Sonnet/Haiku, at most two in parallel,
  agents commit after each milestone, the orchestrator pushes.
- Only the user merges. Nothing under "Was keine Session selbst tut" (crates.io publishing etc.).
