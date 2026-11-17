# Release checklist for 0.1.0

Steps for the maintainer. Sessions prepare the branch; tagging, pushing and
publishing are done by hand.

## 1. Decide the open questions

From `docs/STATUS.md`, answer before the release, because each one changes
public behaviour:

- [ ] `EditorOptions::input_rules`: the plan says input rules can be switched
      off; no option exists yet. Add it (non-breaking, a new field on a
      struct with `Default`) or drop the requirement.
- [ ] `EditorEvent::Changed` payload: plan §3 describes a payload, §1a a
      bare event. 0.0.x ships the bare event.
- [ ] Custom inline nodes for extensions: currently marks only.
- [ ] Empty paragraphs between blocks are saved as `&#32;`. Keep, or drop
      empty paragraphs on save.

## 2. Verify by hand on a real desktop

Automated tests cover these only in simulation or offscreen.

- [ ] IME: dead keys (`´` + `e`), Compose (`Compose o "`), ibus or fcitx with
      Pinyin or Japanese: preedit is underlined, the candidate window follows
      the caret, committing replaces the preedit. Linux Wayland and X11.
- [ ] Drag and drop: drop a PNG from the file manager (goes through
      `on_image`), drop a PDF (attachment row), drag a block by its handle.
- [ ] HiDPI: 200 % and a fractional scale (125 %, 150 %): text is sharp,
      checkboxes and the caret are aligned, images are not blurry.
- [ ] Clipboard: copy from the editor into a plain text editor (Markdown),
      paste a URL over a selection (links it).
- [ ] `cargo run --example notes`: walk through the list at the top of the
      file. Popups fade in; with the desktop's reduced-motion setting passed
      to `App::set_reduce_motion` they appear at once.
- [ ] Screen reader (Orca): headings and checkboxes are announced with their
      role and state.

## 3. CI

- [ ] The `check` and `deny` jobs are green on the release commit.
- [ ] The `cross-build` job (macOS and Windows, `cargo check --all-targets`)
      is green. It runs on pushes to `main` and on manual dispatch only, so
      start it with "Run workflow" if needed.

## 4. Version and changelog

- [ ] `Cargo.toml`: `version = "0.1.0"`; `cargo build` to update
      `Cargo.lock`.
- [ ] `CHANGELOG.md`: rename `## [Unreleased]` to `## [0.1.0]`, with the
      date.
- [ ] `README.md`: change the install snippet from `tag = "v0.0.4"` to
      `tag = "v0.1.0"`.
- [ ] `docs/STATUS.md`: add the `v0.1.0` tag line for downstream sessions.
- [ ] Commit `chore(release): 0.1.0`.

## 5. Tag

- [ ] `git tag -a v0.1.0 -m "v0.1.0"` on the merged `main` commit and push the
      tag.
- [ ] GitHub release with the notes from `git cliff --tag v0.1.0
      --unreleased` (config in `cliff.toml`) or the changelog section.

## 6. Publishing to crates.io (the maintainer's own step)

- [ ] Remove `publish = false` from `Cargo.toml`.
- [ ] `cargo publish --dry-run` and check the packaged file list
      (`cargo package --list`): no `docs/design/`, `tests/corpus/` or
      `.github/`.
- [ ] `cargo publish`.
- [ ] Update the README install snippet to the crates.io version and the
      "not on crates.io yet" note.
- [ ] From then on, `cargo semver-checks` in CI (plan 05).

## 7. After the release

- [ ] GitHub topics: `gpui`, `zed`, `markdown-editor`, `wysiwyg`, `rust-gui`,
      `rich-text-editor`.
- [ ] Proposal at longbridge/gpui-component (issue or discussion) to take the
      crate into GPUI Kit (plan §13).

## Open TODOs

- [ ] README GIF: an animated recording of typing with input rules. It needs
      a real window; record it on a desktop (for example with Peek or
      `wf-recorder` + `gifski`) and add it at the top of the README.
