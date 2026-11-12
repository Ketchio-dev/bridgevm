#!/usr/bin/env bash
# Build the actual host runner and CLI for headless app/channel fixtures.
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"
host=$(rustc ${BRIDGEVM_CHECK_TOOLCHAIN:+"$BRIDGEVM_CHECK_TOOLCHAIN"} -vV | sed -n 's/^host: //p')
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
cargo ${BRIDGEVM_CHECK_TOOLCHAIN:+"$BRIDGEVM_CHECK_TOOLCHAIN"} build \
    -p hvf-runner -p bridgevm-cli --locked --target "$host" --target-dir "$TARGET_DIR" >&2
printf '%s\n' "$TARGET_DIR/$host/debug/hvf-runner"
