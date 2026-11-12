#!/usr/bin/env bash
# Native selection is part of product import tests, including the Swift shim.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
host=$(rustc ${BRIDGEVM_CHECK_TOOLCHAIN:+"$BRIDGEVM_CHECK_TOOLCHAIN"} -vV | sed -n 's/^host: //p')
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
cargo ${BRIDGEVM_CHECK_TOOLCHAIN:+"$BRIDGEVM_CHECK_TOOLCHAIN"} build \
    -p bridgevm-hvf --example snapshot_pair_cli --locked --target "$host" --target-dir "$TARGET_DIR" >&2
printf '%s\n' "$TARGET_DIR/$host/debug/examples/snapshot_pair_cli"
