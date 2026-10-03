#!/usr/bin/env bash
# Deterministic Rust workspace plus real-qcow2 generic import contracts.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export BRIDGEVM_CHECK_TOOLCHAIN="${1:-+1.97.0}"
cargo "$BRIDGEVM_CHECK_TOOLCHAIN" test --workspace --locked
scripts/check-bundle-import-contracts.sh
