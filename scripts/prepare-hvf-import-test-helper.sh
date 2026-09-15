#!/usr/bin/env bash
# Native selection is part of product import tests, including the Swift shim.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
host=$(rustc ${BRIDGEVM_CHECK_TOOLCHAIN:+"$BRIDGEVM_CHECK_TOOLCHAIN"} -vV | sed -n 's/^host: //p')
cargo ${BRIDGEVM_CHECK_TOOLCHAIN:+"$BRIDGEVM_CHECK_TOOLCHAIN"} build \
    -p bridgevm-hvf --example snapshot_pair_cli --locked --target "$host" --target-dir "$ROOT/target" >&2
printf '%s\n' "$ROOT/target/$host/debug/examples/snapshot_pair_cli"
