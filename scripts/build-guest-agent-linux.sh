#!/usr/bin/env bash
set -euo pipefail

# Cross-compile the Linux guest agent (bridgevm-tools-linux) that runs INSIDE a
# guest VM and speaks the guest-tools protocol over the virtio-serial channel.
#
# From an Apple Silicon (arm64 macOS) host we cross-compile to Linux arm64 using
# zig as the cross-linker via cargo-zigbuild. One-time toolchain setup:
#   rustup target add aarch64-unknown-linux-gnu
#   brew install zig
#   cargo install cargo-zigbuild
#
# Usage:
#   scripts/build-guest-agent-linux.sh [--target <triple>] [--debug]
# Default target: aarch64-unknown-linux-gnu (release).

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

TARGET="aarch64-unknown-linux-gnu"
PROFILE_FLAG="--release"
PROFILE_DIR="release"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target) TARGET="$2"; shift 2 ;;
    --debug) PROFILE_FLAG=""; PROFILE_DIR="debug"; shift ;;
    -h|--help) sed -n '3,16p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

require() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "error: missing '$1'. Toolchain setup:" >&2
    echo "  rustup target add $TARGET && brew install zig && cargo install cargo-zigbuild" >&2
    exit 1
  }
}
require cargo
require zig
require cargo-zigbuild

if ! rustup target list --installed 2>/dev/null | grep -q "^$TARGET$"; then
  echo "Adding rust target $TARGET ..."
  rustup target add "$TARGET"
fi

echo "Cross-compiling bridgevm-tools-linux for $TARGET ($PROFILE_DIR) ..."
cargo zigbuild -p bridgevm-tools-linux --target "$TARGET" $PROFILE_FLAG

ARTIFACT="target/$TARGET/$PROFILE_DIR/bridgevm-tools-linux"
[[ -f "$ARTIFACT" ]] || { echo "error: artifact not found: $ARTIFACT" >&2; exit 1; }
echo "Built: $ARTIFACT"
file "$ARTIFACT" | head -1
printf '%s\n' "$ARTIFACT"
