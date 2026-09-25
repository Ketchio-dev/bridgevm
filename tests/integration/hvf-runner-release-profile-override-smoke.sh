#!/usr/bin/env bash
# A release Cargo profile with debug assertions must fail before it can package a runner.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
store="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-runner-profile.XXXXXX")"
trap 'rm -rf "$store"' EXIT
export CARGO_TARGET_DIR="$store/target"
export CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=true

fail() { echo "release runner profile override: FAIL ($*)" >&2; exit 1; }

# Cargo controls PROFILE even when the invoking environment tries to set it.
PROFILE=debug cargo +1.97.0 rustc --locked --release -p hvf-runner --bin hvf-runner --quiet \
  -- --print cfg > "$store/cfg" 2> "$store/cfg-error" || fail "cannot inspect overridden release cfg: $(cat "$store/cfg-error")"
rg -qx 'debug_assertions' "$store/cfg" || fail 'release debug-assertions override did not take effect'
rg -qx 'bridgevm_non_debug_profile' "$store/cfg" || fail 'Cargo profile was not classified as non-debug'

if PROFILE=debug cargo +1.97.0 build --locked --release -p hvf-runner --quiet > "$store/build" 2>&1; then
  fail 'overridden release profile produced a runnable hvf-runner'
fi
rg -Fq 'hvf-runner non-debug Cargo profile cannot enable debug assertions' "$store/build" ||
  fail "build failed without the release-policy diagnostic: $(cat "$store/build")"
[[ ! -e "$CARGO_TARGET_DIR/release/hvf-runner" ]] || fail 'overridden release profile left an executable'

echo 'release runner profile override: PASS (debug assertions enabled; build refused; no binary)'
