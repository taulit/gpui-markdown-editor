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

- A sub-agent is fixing the E2 bug-review findings on `phase/e2-editing` (commits as `fix(edit): …`).
  Items, each with a regression test in `src/edit/tests/`:
  1. blocking: pending marks (Ctrl+E at a caret) only apply to the first typed character
  2. Tab with a selection in a code block deletes the selection
  3. copying part of one heading/list item keeps the block kind
  4. multi-line plain-text paste into a list item breaks the list
  5. select-all misses rules/images/tables at the document edges; cut leaves an empty table
  6. IME preedit states become undo steps
  7. Enter on an empty item in the middle of a list does not round-trip (tight list turns loose)
  8. inline HTML is escaped after copy/paste
  9. leading/trailing spaces are written as `&#32;` (also on the clipboard)
  10. `InsertRule` leaves an empty trailing paragraph that changes the saved file
- Check `git log origin/main..phase/e2-editing` for which of these landed; anything missing is still open.

## Next

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
