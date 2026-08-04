#!/usr/bin/env bash
# Run the loom interleaving models.
#
# These are cfg-gated on `loom` so an ordinary `cargo test` does not pay for
# permutation search. Release mode because loom explores many interleavings
# and a debug build makes that slow enough to discourage running it.
set -euo pipefail

cd "$(dirname "$0")/.."
TOOLCHAIN="${BRIDGEVM_RUST_TOOLCHAIN:-1.97.0}"

total=0
for model in loom_psci loom_reset_generation; do
    output="$(RUSTFLAGS="--cfg loom" cargo "+$TOOLCHAIN" test \
        -p bridgevm-hvf --test "$model" --release 2>&1)" || {
        echo "$output"
        echo "FAIL: $model" >&2
        exit 1
    }
    passed="$(printf '%s' "$output" | awk '/^test result/ {sum += $4} END {print sum+0}')"
    echo "  $model: $passed tests"
    total=$((total + passed))
done

if [ "$total" -eq 0 ]; then
    # A cfg typo would silently compile every model to nothing and "pass".
    echo "FAIL: no loom tests ran; check the --cfg loom gate" >&2
    exit 1
fi
echo "PASS: loom ($total tests)"
