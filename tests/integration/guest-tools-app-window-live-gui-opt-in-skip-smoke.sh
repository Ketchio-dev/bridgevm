#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LIVE_SMOKE="tests/integration/guest-tools-app-window-live-gui-opt-in-smoke.sh"

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

cd "$ROOT"

grep -Fq "WINDOW_PAYLOAD" "$LIVE_SMOKE" \
  || fail "$LIVE_SMOKE no longer preserves the live wmctrl window payload"
grep -Fq "real-guest-window-proxy-crop-synthetic-framebuffer" "$LIVE_SMOKE" \
  || fail "$LIVE_SMOKE no longer records the live-window crop proof kind"
grep -Fq "displayd materialized a proxy crop from the live wmctrl bounds" "$LIVE_SMOKE" \
  || fail "$LIVE_SMOKE no longer reports the live-window crop artifact"

output="$(
  env \
    -u BRIDGEVM_LIVE_GUEST_TOOLS_ALLOW_REAL_START \
    -u BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK \
    -u BRIDGEVM_LIVE_GUEST_TOOLS_STORE \
    -u BRIDGEVM_LIVE_GUEST_TOOLS_VM \
    -u BRIDGEVM_LIVE_GUEST_TOOLS_TIMEOUT_SECONDS \
    "$LIVE_SMOKE" 2>&1
)" || fail "$LIVE_SMOKE did not exit successfully without live opt-in; got: $output"

assert_contains "$output" "SKIP:" "guest-tools app/window live GUI opt-in default output"
assert_contains "$output" "BRIDGEVM_LIVE_GUEST_TOOLS_ALLOW_REAL_START=1" \
  "guest-tools app/window live GUI opt-in default output"

missing_disk_output="$(
  env \
    -u BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK \
    BRIDGEVM_LIVE_GUEST_TOOLS_ALLOW_REAL_START=1 \
    "$LIVE_SMOKE" 2>&1
)" || fail "$LIVE_SMOKE did not exit successfully without qcow2 disk; got: $missing_disk_output"

assert_contains "$missing_disk_output" "SKIP:" "guest-tools app/window live GUI missing disk output"
assert_contains "$missing_disk_output" "BRIDGEVM_LIVE_GUEST_TOOLS_QCOW2_DISK" \
  "guest-tools app/window live GUI missing disk output"

echo "PASS: guest-tools app/window live GUI opt-in smoke skips safely by default"
