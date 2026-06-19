#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if [[ "${BRIDGEVM_HVF_ALLOW_MMIO_WRITE_EMULATION:-}" != "1" ]]; then
  echo "SKIP: set BRIDGEVM_HVF_ALLOW_MMIO_WRITE_EMULATION=1 to emulate one unmapped STR write, capture X0, advance PC, and continue to HVC"
  exit 0
fi

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

if [[ -n "${BRIDGEVM_LIVE_HVF_RUNNER:-}" ]]; then
  runner="$(apps/macos/scripts/build-sign-hvf-runner.sh --verify-only "$BRIDGEVM_LIVE_HVF_RUNNER")" \
    || fail "configured HVF runner is not signed with the hypervisor entitlement"
else
  runner="$(apps/macos/scripts/build-sign-hvf-runner.sh)" \
    || fail "could not build/sign hvf-runner with the hypervisor entitlement"
fi

output="$("$runner" --mmio-write-emulation-probe --allow-emulate 2>&1)" \
  || fail "hvf-runner --mmio-write-emulation-probe --allow-emulate failed: $output"

assert_contains "$output" "HVF MMIO write emulation probe" "HVF MMIO write emulation live output"
assert_contains "$output" "Allowed: true" "HVF MMIO write emulation live output"
assert_contains "$output" "Attempted: true" "HVF MMIO write emulation live output"
assert_contains "$output" "Write value register set: true" "HVF MMIO write emulation live output"
assert_contains "$output" "MMIO exit observed: true" "HVF MMIO write emulation live output"
assert_contains "$output" "Write value captured: true" "HVF MMIO write emulation live output"
assert_contains "$output" "PC advanced: true" "HVF MMIO write emulation live output"
assert_contains "$output" "Continuation exit observed: true" "HVF MMIO write emulation live output"
assert_contains "$output" "Write value preserved: true" "HVF MMIO write emulation live output"
assert_contains "$output" "MMIO exit syndrome: 0x93c08046" "HVF MMIO write emulation live output"
assert_contains "$output" "Continuation exit syndrome: 0x5a000000" "HVF MMIO write emulation live output"
assert_contains "$output" "Captured write value: 0xfedcba987654321" "HVF MMIO write emulation live output"
assert_contains "$output" "Write value after continue: 0xfedcba987654321" "HVF MMIO write emulation live output"
assert_contains "$output" "Blockers: none" "HVF MMIO write emulation live output"

echo "PASS: HVF MMIO write emulation live opt-in smoke"
