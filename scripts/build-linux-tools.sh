#!/usr/bin/env bash
set -euo pipefail

# D4: cross-build bridgevm-tools-linux for aarch64 Linux from macOS so the Apple
# VZ live-GUI guest-tools path has a real ELF to stage. Uses cargo-zigbuild
# (zig as the cross linker); falls back is not attempted here — if zigbuild is
# absent the script fails loudly with the install hint.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="aarch64-unknown-linux-gnu"
OUT="$ROOT/target/$TARGET/release/bridgevm-tools-linux"

command -v cargo-zigbuild >/dev/null 2>&1 || {
  echo "cargo-zigbuild not found; install with: cargo install cargo-zigbuild (needs zig)" >&2
  exit 1
}
command -v zig >/dev/null 2>&1 || command -v /opt/homebrew/opt/zig/bin/zig >/dev/null 2>&1 || {
  echo "zig not found; cargo-zigbuild needs zig as the cross linker" >&2
  exit 1
}

cd "$ROOT"
cargo zigbuild --release --target "$TARGET" -p bridgevm-tools-linux

[[ -f "$OUT" ]] || { echo "expected output missing: $OUT" >&2; exit 1; }
file "$OUT"
printf 'built %s\n' "$OUT"
