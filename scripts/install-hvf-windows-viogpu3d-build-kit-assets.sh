#!/usr/bin/env bash
# Install the static builders, notices and patches used by the Windows kit.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-}"
[[ -d "$OUT_DIR" ]] || { echo "usage: $0 OUTPUT_DIRECTORY" >&2; exit 2; }

"$ROOT/scripts/install-windows-graphics-notice-tools.sh" "$OUT_DIR"
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
