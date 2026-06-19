#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if [[ "${BRIDGEVM_HVF_ALLOW_MMIO_SERIAL_DEVICE:-}" != "1" ]]; then
  echo "SKIP: set BRIDGEVM_HVF_ALLOW_MMIO_SERIAL_DEVICE=1 to emulate one serial data write, one status read, and one HVC continuation"
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

output="$("$runner" --mmio-serial-device-probe --allow-device 2>&1)" \
  || fail "hvf-runner --mmio-serial-device-probe --allow-device failed: $output"

assert_contains "$output" "HVF MMIO serial device probe" "HVF MMIO serial device live output"
assert_contains "$output" "Device model: PL011 UART skeleton" "HVF MMIO serial device live output"
assert_contains "$output" "Allowed: true" "HVF MMIO serial device live output"
assert_contains "$output" "Attempted: true" "HVF MMIO serial device live output"
assert_contains "$output" "Write value register set: true" "HVF MMIO serial device live output"
assert_contains "$output" "Data address register set: true" "HVF MMIO serial device live output"
assert_contains "$output" "Status address register set: true" "HVF MMIO serial device live output"
assert_contains "$output" "Device bus created: true" "HVF MMIO serial device live output"
assert_contains "$output" "Device bus device count: 1" "HVF MMIO serial device live output"
assert_contains "$output" "Write exit observed: true" "HVF MMIO serial device live output"
assert_contains "$output" "Write handled by device: true" "HVF MMIO serial device live output"
assert_contains "$output" "Write value captured: true" "HVF MMIO serial device live output"
assert_contains "$output" "PC advanced after write: true" "HVF MMIO serial device live output"
assert_contains "$output" "Status exit observed: true" "HVF MMIO serial device live output"
assert_contains "$output" "Status handled by device: true" "HVF MMIO serial device live output"
assert_contains "$output" "Status value injected: true" "HVF MMIO serial device live output"
assert_contains "$output" "PC advanced after status: true" "HVF MMIO serial device live output"
assert_contains "$output" "Continuation exit observed: true" "HVF MMIO serial device live output"
assert_contains "$output" "Status value preserved: true" "HVF MMIO serial device live output"
assert_contains "$output" "Write exit syndrome: 0x93c08046" "HVF MMIO serial device live output"
assert_contains "$output" "Status exit syndrome: 0x93c08006" "HVF MMIO serial device live output"
assert_contains "$output" "Continuation exit syndrome: 0x5a000000" "HVF MMIO serial device live output"
assert_contains "$output" "Captured write value: 0x41" "HVF MMIO serial device live output"
assert_contains "$output" "Captured byte: 0x41" "HVF MMIO serial device live output"
assert_contains "$output" "Status value after continue: 0x90" "HVF MMIO serial device live output"
assert_contains "$output" "Blockers: none" "HVF MMIO serial device live output"

echo "PASS: HVF MMIO serial device live opt-in smoke"
