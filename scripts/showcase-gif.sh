#!/usr/bin/env bash
# Renders the showcase typing frames offscreen and combines them into
# docs/screenshots/showcase.gif (used by the docs site and the launch post).
# Needs a GPU or lavapipe, and ffmpeg.
set -euo pipefail
cd "$(dirname "$0")/.."
rm -rf target/showcase-typing
# The headless renderer can fail after many windows; each run resumes.
for attempt in 1 2 3 4 5 6; do
  cargo test --test screenshots showcase_typing_frames -- --ignored --test-threads=1 && break
  echo "attempt $attempt failed, resuming"
done
# The GIF opens on the finished note (it is the first frame in previews and
# thumbnails), then types it from the start and pauses on the result.
frames=(target/showcase-typing/frame-*.png)
last=${frames[-1]}
seq=target/showcase-typing/seq
mkdir -p "$seq"
n=0
for f in $(for _ in $(seq 12); do echo "$last"; done) "${frames[@]}"; do
  ln -sf "$PWD/$f" "$seq/$(printf '%04d' "$n").png"
  n=$((n + 1))
done
ffmpeg -loglevel error -y -framerate 10 -i "$seq/%04d.png" \
  -vf "crop=840:880:190:0,scale=720:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=96[p];[b][p]paletteuse=dither=none" \
  -loop 0 docs/screenshots/showcase.gif
echo "wrote docs/screenshots/showcase.gif"
