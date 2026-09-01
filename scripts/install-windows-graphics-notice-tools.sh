#!/usr/bin/env bash
# Install the self-contained Windows graphics notice verifier toolset.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-}"
[[ -d "$OUT_DIR" ]] || { echo "usage: $0 OUTPUT_DIRECTORY" >&2; exit 2; }

install -m 644 \
  "$ROOT/THIRD-PARTY-PATCHES.tsv" \
  "$ROOT/docs/licenses/Mesa-patched-files-MIT.txt" \
  "$ROOT/docs/licenses/virtio-win-BSD-3-Clause.txt" \
  "$ROOT/scripts/patches/virtio-win-mesa-unbound-clear.patch" \
  "$ROOT/scripts/patches/virtio-win-mesa-submit-trace.patch" \
  "$OUT_DIR"
install -m 755 "$ROOT/scripts/package-windows-graphics-notices.py" \
  "$ROOT/scripts/windows_graphics_notice_git.py" "$OUT_DIR"
