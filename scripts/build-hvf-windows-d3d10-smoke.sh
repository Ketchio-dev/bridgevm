#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
OUT_DIR=${1:-"$ROOT_DIR/build/win-tests"}
ZIG=${ZIG:-zig}

mkdir -p "$OUT_DIR"

"$ZIG" cc -target aarch64-windows-gnu -O2 \
  -o "$OUT_DIR/bridgevm-d3d10-smoke.exe" \
  "$ROOT_DIR/scripts/win-tests/bridgevm-d3d10-smoke.c" \
  -ld3d10 -ldxgi

"$ZIG" cc -target aarch64-windows-gnu -O2 \
  -o "$OUT_DIR/bridgevm-debug-runner.exe" \
  "$ROOT_DIR/scripts/win-tests/bridgevm-debug-runner.c"

"$ZIG" cc -target aarch64-windows-gnu -O2 \
  -o "$OUT_DIR/bridgevm-d3d10-draw-smoke.exe" \
  "$ROOT_DIR/scripts/win-tests/bridgevm-d3d10-draw-smoke.c" \
  -ld3d10 -ldxgi

"$ZIG" cc -target aarch64-windows-gnu -O2 \
  -o "$OUT_DIR/bridgevm-d3d10-bench.exe" \
  "$ROOT_DIR/scripts/win-tests/bridgevm-d3d10-bench.c" \
  -ld3d10 -ldxgi

VULKAN_INCLUDE=${VULKAN_INCLUDE:-/opt/homebrew/include}
"$ZIG" cc -target aarch64-windows-gnu -O2 \
  -I"$VULKAN_INCLUDE" \
  -o "$OUT_DIR/bridgevm-vulkan-draw-smoke.exe" \
  "$ROOT_DIR/scripts/win-tests/bridgevm-vulkan-draw-smoke.c"

file "$OUT_DIR/bridgevm-d3d10-smoke.exe"
file "$OUT_DIR/bridgevm-debug-runner.exe"
file "$OUT_DIR/bridgevm-d3d10-draw-smoke.exe"
file "$OUT_DIR/bridgevm-d3d10-bench.exe"
file "$OUT_DIR/bridgevm-vulkan-draw-smoke.exe"
