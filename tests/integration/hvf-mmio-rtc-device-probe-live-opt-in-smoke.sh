#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if [[ "${BRIDGEVM_HVF_ALLOW_MMIO_RTC_DEVICE:-}" != "1" ]]; then
  echo "SKIP: set BRIDGEVM_HVF_ALLOW_MMIO_RTC_DEVICE=1 to emulate a PL031 RTC read through the multi-device MMIO bus"
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

output="$("$runner" --mmio-rtc-device-probe --allow-device 2>&1)" \
  || fail "hvf-runner --mmio-rtc-device-probe --allow-device failed: $output"

assert_contains "$output" "HVF MMIO RTC device probe" "HVF MMIO RTC device live output"
assert_contains "$output" "Device models: PL011 UART skeleton; PL031 RTC skeleton" "HVF MMIO RTC device live output"
assert_contains "$output" "Allowed: true" "HVF MMIO RTC device live output"
assert_contains "$output" "Attempted: true" "HVF MMIO RTC device live output"
assert_contains "$output" "RTC address register set: true" "HVF MMIO RTC device live output"
assert_contains "$output" "Device bus created: true" "HVF MMIO RTC device live output"
assert_contains "$output" "Device bus device count: 2" "HVF MMIO RTC device live output"
assert_contains "$output" "RTC exit observed: true" "HVF MMIO RTC device live output"
assert_contains "$output" "RTC handled by device: true" "HVF MMIO RTC device live output"
assert_contains "$output" "RTC value injected: true" "HVF MMIO RTC device live output"
assert_contains "$output" "PC advanced: true" "HVF MMIO RTC device live output"
assert_contains "$output" "Continuation exit observed: true" "HVF MMIO RTC device live output"
assert_contains "$output" "RTC value preserved: true" "HVF MMIO RTC device live output"
assert_contains "$output" "RTC exit syndrome: 0x93c08006" "HVF MMIO RTC device live output"
assert_contains "$output" "RTC exit virtual address: 0x50001000" "HVF MMIO RTC device live output"
assert_contains "$output" "Continuation exit syndrome: 0x5a000000" "HVF MMIO RTC device live output"
assert_contains "$output" "RTC value after continue: 0x20260618" "HVF MMIO RTC device live output"
assert_contains "$output" "Blockers: none" "HVF MMIO RTC device live output"

echo "PASS: HVF MMIO RTC device live opt-in smoke"
