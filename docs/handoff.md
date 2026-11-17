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

E3 PR: https://github.com/taulit/gpui-markdown-editor/pull/5 (tag `v0.0.4`), waiting for the user.
E4 part 1 runs on `phase/e4-release` (stacked on E3): a sub-agent does benchmarks, `docs/performance.md`, motion, README and docs. Rebase onto `main` once PR 5 is merged.

## Next

1. After the E3 PR is merged: branch `phase/e4-release` from `main`.
2. E4: criterion benchmarks and `docs/performance.md` (keystroke + frame in a 10,000-line document measured
   at 0.62 ms median, budget 4 ms), README with GIF, shortcut table and comparison (plan §13), popup motion
   (250 ms ease-out) and checkbox press spring (plan 04), docs pass, final bug/spec/simplicity reviews,
   `docs/release-checklist.md`, tag `v0.1.0`. No crates.io publishing (the user does that).
3. Apply the user's answers to the pending decisions listed in `docs/STATUS.md`.

## Open, known and accepted for now

- gpui-pre 0.3.12 exposes no HTML clipboard format; `markdown::html_to_markdown` exists but paste never reaches it.
- AccessKit: caret and selection are not reported (gpui's synthetic nodes are leaves).
- Folding and block drag work on top-level blocks only. Extensions can add marks, not inline nodes or block renderers.
- Attachments are a paragraph that is only a link to a local non-image file (the plan does not define the syntax).

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
