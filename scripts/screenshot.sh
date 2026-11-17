#!/usr/bin/env bash
# Renders the editor offscreen and writes the PNGs to docs/screenshots/.
# No window is opened. Needs a GPU or a software Vulkan driver (lavapipe).
#
#   scripts/screenshot.sh             all screenshots
#   scripts/screenshot.sh e1_basic    only the tests matching the filter
set -euo pipefail
cd "$(dirname "$0")/.."
exec cargo test --test screenshots -- --ignored --nocapture --test-threads=1 "$@"
