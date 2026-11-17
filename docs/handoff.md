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

Nothing. E0–E3 are merged to `main` (PRs 1, 3, 4, 5; tag `v0.0.4`). E4 is complete on `phase/e4-release`
(rebased on `main`, final bug review done and its two findings fixed), PR open against `main`, waiting for the user.

## Next

1. Apply the user's answers to the four pending decisions in `docs/STATUS.md`.
2. Then bump to 0.1.0, changelog, tag `v0.1.0` (see `docs/release-checklist.md`). No crates.io publishing
   (the user does that).
3. Known performance edge: opening 1 MB takes about 31 ms on a quiet machine (budget 50 ms) but 57–66 ms under
   heavy load; parsing is about 35 ms of it.

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
