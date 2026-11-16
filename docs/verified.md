# Verified assumptions

Assumptions marked "(prüfen)" in the plan, verified on 2026-11-16.

| Assumption | Result | Evidence | Consequence |
| --- | --- | --- | --- |
| Which gpui version to target | `gpui-component 0.10.0` (crates.io, repo longbridge/gpui-kit) requires `gpui-pre =0.3.12` exactly. A newer `gpui-pre 0.3.13` exists but is not what gpui-component 0.10.0 pins. | Its Cargo.toml: `[dependencies.gpui] version = "=0.3.12", package = "gpui-pre"`. | This crate pins `gpui-pre =0.3.12` and `gpui-component =0.10.0`. Apps additionally need `gpui-pre-platform =0.3.12` (lib `gpui_platform`, `application()`); this crate uses it only as a dev-dependency. |
| AccessKit is available | Present. | gpui-pre 0.3.12 depends on `accesskit`, ships `src/_accessibility.rs` (guide) and `examples/a11y.rs`. | Plan section 11 accessibility roles are implemented in phase E3/E4 against this API, not deferred. |
| IME panic bug ("panic when typing again after deleting IME text") | No matching upstream issue found by web search on 2026-11-16. Related but different fixes: zed #12299 (IBus/compose state on Linux), #28429 (preedit cursor position on Wayland), #12702 (IME action buffering). | Web search. The hazard class is a stale marked (preedit) range after the marked text was deleted, leading to out-of-range slicing in the `EntityInputHandler` implementation. | In our handler: clamp every incoming UTF-16 range to the current text and to char boundaries, clear the marked range whenever text is deleted or the selection moves, and cover "mark text, delete, type again" with an interaction test in phase E2. Manual check with dead keys (´ + e), Compose and ibus-pinyin stays on the E2 gate list. |
| Offscreen screenshots are possible | Yes. gpui-pre 0.3.12 provides `HeadlessAppContext::capture_screenshot` with `gpui_platform::current_headless_renderer()` (wgpu on Linux, feature `test-support`). | gpui-pre 0.3.12 source. | PR screenshots are rendered offscreen by `scripts/screenshot.sh`, no window on the developer's desktop. |

## Model deviation from the plan sketch

Not a "(prüfen)" item; recorded for downstream.

- Headings are modeled for levels 1-6 (plan sketch: 1-3) so that existing files with deeper headings stay WYSIWYG; the UI offers 1-3.
- `OrderedItem` stores `number` per item.
- Image blocks carry an optional Obsidian-style `width`.
- Positions are `TextPos { block: BlockId, offset }`; table cells have their own ids.
