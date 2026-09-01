#!/usr/bin/env bash
# Install the static builders, notices and patches used by the Windows kit.
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
install -m 755 \
  "$ROOT/scripts/package-windows-graphics-notices.py" \
  "$OUT_DIR"
install -m 644 "$ROOT/scripts/finalize-hvf-windows-viogpu3d-package.ps1" \
  "$OUT_DIR/finalize-viogpu3d-package.ps1"
install -m 644 "$ROOT/scripts/finalize-hvf-windows-viogpu3d-test-package.ps1" \
  "$OUT_DIR/finalize-viogpu3d-test-package.ps1"
install -m 644 \
  "$ROOT/scripts/win-assets/viogpu3d-arehnman-arm64-minimal.inf" \
  "$ROOT/scripts/win-assets/build-mesa-arm64.ps1" \
  "$ROOT/scripts/win-assets/mesa-cross-arm64.ini" \
  "$ROOT/scripts/win-assets/run-submit-trace-build.cmd" \
  "$OUT_DIR"
