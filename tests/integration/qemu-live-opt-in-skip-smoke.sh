#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

LIVE_SMOKE="tests/integration/qemu-live-boot-opt-in-smoke.sh"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  case "$haystack" in
    *"$needle"*) ;;
    *) fail "$label missing '$needle'; got: $haystack" ;;
  esac
}

output="$(
  env \
    -u BRIDGEVM_LIVE_QEMU_ALLOW_REAL_START \
    -u BRIDGEVM_LIVE_QEMU_STORE \
    -u BRIDGEVM_LIVE_QEMU_VM \
    -u BRIDGEVM_LIVE_QEMU_QCOW2_DISK \
    -u BRIDGEVM_LIVE_QEMU_SERIAL_EXPECTED \
    -u BRIDGEVM_LIVE_QEMU_TIMEOUT_SECONDS \
    -u BRIDGEVM_LIVE_QEMU_EVIDENCE_DIR \
    "$LIVE_SMOKE" 2>&1
)" || fail "$LIVE_SMOKE did not exit successfully without live opt-in; got: $output"

assert_contains "$output" "SKIP:" "QEMU live opt-in default output"
assert_contains "$output" "BRIDGEVM_LIVE_QEMU_ALLOW_REAL_START=1" "QEMU live opt-in default output"

missing_sentinel_output="$(
  env \
    -u BRIDGEVM_LIVE_QEMU_SERIAL_EXPECTED \
    BRIDGEVM_LIVE_QEMU_ALLOW_REAL_START=1 \
    "$LIVE_SMOKE" 2>&1
)" || fail "$LIVE_SMOKE did not exit successfully without serial sentinel; got: $missing_sentinel_output"

assert_contains "$missing_sentinel_output" "SKIP:" "QEMU live missing sentinel output"
assert_contains "$missing_sentinel_output" "BRIDGEVM_LIVE_QEMU_SERIAL_EXPECTED" "QEMU live missing sentinel output"

echo "PASS: QEMU live opt-in smoke skips successfully by default"
