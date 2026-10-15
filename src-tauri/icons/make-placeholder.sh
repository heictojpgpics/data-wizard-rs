#!/usr/bin/env bash
# Minimal placeholder PNG (1x1 transparent) — replaced by real icons later.
# This file is intentionally NOT a real icon — it's a stub so `tauri build`
# doesn't fail with a missing-file error during early scaffolding.
#
# Real icons are produced by `tauri icon path/to/source.png` from a 1024x1024
# source PNG. We'll generate them in a follow-up.

set -euo pipefail
OUT="$1"
# 1x1 transparent PNG
printf '\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15\xc4\x89\x00\x00\x00\nIDATx\x9cc\xfa\xcf\x00\x00\x00\x02\x00\x01\xe5\xa7\x07\xa1\x00\x00\x00\x00IEND\xaeB`\x82' > "$OUT"
