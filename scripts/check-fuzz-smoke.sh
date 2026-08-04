#!/usr/bin/env bash
# Run every parser fuzz target over its checked-in corpus.
#
# Uses the pinned stable toolchain, so this runs in ordinary CI. It is a smoke
# test: it proves the targets build, the corpus is present and nothing in it
# panics. A real campaign needs `cargo fuzz run <target>` on nightly.
set -euo pipefail

cd "$(dirname "$0")/../fuzz"
TOOLCHAIN="${BRIDGEVM_RUST_TOOLCHAIN:-1.97.0}"
exec cargo "+$TOOLCHAIN" run --release --bin smoke "$@"
