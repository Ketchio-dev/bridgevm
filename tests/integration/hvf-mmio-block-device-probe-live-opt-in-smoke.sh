#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if [[ "${BRIDGEVM_HVF_ALLOW_MMIO_BLOCK_DEVICE:-}" != "1" ]]; then
  echo "SKIP: set BRIDGEVM_HVF_ALLOW_MMIO_BLOCK_DEVICE=1 to emulate VirtIO-MMIO block identity reads through the MMIO bus"
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

output="$("$runner" --mmio-block-device-probe --allow-device 2>&1)" \
  || fail "hvf-runner --mmio-block-device-probe --allow-device failed: $output"

assert_contains "$output" "HVF MMIO block device probe" "HVF MMIO block device live output"
assert_contains "$output" "Device models: PL011 UART skeleton; PL031 RTC skeleton; VirtIO-MMIO block identity skeleton" "HVF MMIO block device live output"
assert_contains "$output" "Allowed: true" "HVF MMIO block device live output"
assert_contains "$output" "Attempted: true" "HVF MMIO block device live output"
assert_contains "$output" "Register address registers set: true" "HVF MMIO block device live output"
assert_contains "$output" "Device bus created: true" "HVF MMIO block device live output"
assert_contains "$output" "Device bus device count: 3" "HVF MMIO block device live output"
assert_contains "$output" "magic at 0x50002000: expected 0x74726976, run=true, exit=true, handled=true, injected=true, pc_advanced=true" "HVF MMIO block device live output"
assert_contains "$output" "version at 0x50002004: expected 0x2, run=true, exit=true, handled=true, injected=true, pc_advanced=true" "HVF MMIO block device live output"
assert_contains "$output" "device_id at 0x50002008: expected 0x2, run=true, exit=true, handled=true, injected=true, pc_advanced=true" "HVF MMIO block device live output"
assert_contains "$output" "vendor_id at 0x5000200c: expected 0x4252564d, run=true, exit=true, handled=true, injected=true, pc_advanced=true" "HVF MMIO block device live output"
assert_contains "$output" "Continuation exit observed: true" "HVF MMIO block device live output"
assert_contains "$output" "Vendor value preserved: true" "HVF MMIO block device live output"
assert_contains "$output" "Continuation exit syndrome: 0x5a000000" "HVF MMIO block device live output"
assert_contains "$output" "Vendor value after continue: 0x4252564d" "HVF MMIO block device live output"
assert_contains "$output" "Blockers: none" "HVF MMIO block device live output"

echo "PASS: HVF MMIO block device live opt-in smoke"
