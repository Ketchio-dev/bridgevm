#!/usr/bin/env bash
# Build the actual host runner and CLI for headless app/channel fixtures.
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"
host=$(rustc ${BRIDGEVM_CHECK_TOOLCHAIN:+"$BRIDGEVM_CHECK_TOOLCHAIN"} -vV | sed -n 's/^host: //p')
cargo ${BRIDGEVM_CHECK_TOOLCHAIN:+"$BRIDGEVM_CHECK_TOOLCHAIN"} build \
    -p hvf-runner -p bridgevm-cli --locked --target "$host" --target-dir "$ROOT/target" >&2
printf '%s\n' "$ROOT/target/$host/debug/hvf-runner"
