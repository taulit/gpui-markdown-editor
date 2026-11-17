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

Nothing. E0–E4 are merged to `main` (PRs 1, 3, 4, 5, 6). The release branch `phase/e5-release-0.1.0` applies the
user's four decisions (`EditorOptions::input_rules`, plain `Changed` in the plan, extension limit documented,
empty paragraphs not written) and bumps to 0.1.0; tag `v0.1.0`, PR open against `main`, waiting for the user.

## Next

1. After the merge: the manual checks in `docs/release-checklist.md` on a real desktop (IME, drag and drop,
   HiDPI, screen reader). They were never done; everything was verified with tests and offscreen screenshots.
2. crates.io publishing is the user's own step.
3. Known performance edge: opening 1 MB takes about 31 ms on a quiet machine (budget 50 ms) but 57–66 ms under
   heavy load; parsing is about 35 ms of it.

## Open, known and accepted for now

- gpui-pre 0.3.12 exposes no HTML clipboard format; `markdown::html_to_markdown` exists but paste never reaches it.
- AccessKit: caret and selection are not reported (gpui's synthetic nodes are leaves).
- Folding and block drag work on top-level blocks only. Extensions can add marks, not inline nodes or block renderers.
- Attachments are a paragraph that is only a link to a local non-image file (the plan does not define the syntax).

- Empty paragraphs are not saved (decided for 0.1.0): they exist only as places to type. Two lists around one
  join into one list on reload. Files from 0.0.3/0.0.4 with a lone `&#32;` line read it as a paragraph with a space.

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
