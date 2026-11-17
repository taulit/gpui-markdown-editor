#!/usr/bin/env bash
# Renders the typing frames offscreen and combines them into
# docs/screenshots/typing.gif. Needs a GPU or lavapipe, and ffmpeg.
set -euo pipefail
cd "$(dirname "$0")/.."
rm -rf target/readme-typing
cargo test --test screenshots readme_typing_frames -- --ignored --test-threads=1
ffmpeg -loglevel error -y -framerate 8 -i target/readme-typing/frame-%03d.png \
  -vf "scale=720:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=64[p];[b][p]paletteuse=dither=none" \
  -loop 0 docs/screenshots/typing.gif
echo "wrote docs/screenshots/typing.gif"
