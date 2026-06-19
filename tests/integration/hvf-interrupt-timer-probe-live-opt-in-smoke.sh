#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if [[ "${BRIDGEVM_HVF_ALLOW_INTERRUPT_TIMER:-}" != "1" ]]; then
  echo "SKIP: set BRIDGEVM_HVF_ALLOW_INTERRUPT_TIMER=1 to verify pending IRQ and virtual timer controls on an empty HVF vCPU"
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

output="$("$runner" --interrupt-timer-probe --allow-interrupt-timer 2>&1)" \
  || fail "hvf-runner --interrupt-timer-probe --allow-interrupt-timer failed: $output"

assert_contains "$output" "HVF interrupt/timer probe" "HVF interrupt/timer live output"
assert_contains "$output" "QEMU: not used" "HVF interrupt/timer live output"
assert_contains "$output" "Apple VZ: not used" "HVF interrupt/timer live output"
assert_contains "$output" "Guest execution: not entered" "HVF interrupt/timer live output"
assert_contains "$output" "Allowed: true" "HVF interrupt/timer live output"
assert_contains "$output" "Attempted: true" "HVF interrupt/timer live output"
assert_contains "$output" "VM created: true" "HVF interrupt/timer live output"
assert_contains "$output" "vCPU created: true" "HVF interrupt/timer live output"
assert_contains "$output" "Pending IRQ set: true" "HVF interrupt/timer live output"
assert_contains "$output" "Pending IRQ after set: true" "HVF interrupt/timer live output"
assert_contains "$output" "Pending IRQ cleared: true" "HVF interrupt/timer live output"
assert_contains "$output" "Pending IRQ after clear: false" "HVF interrupt/timer live output"
assert_contains "$output" "VTimer masked: true" "HVF interrupt/timer live output"
assert_contains "$output" "VTimer mask after set: true" "HVF interrupt/timer live output"
assert_contains "$output" "VTimer unmasked: true" "HVF interrupt/timer live output"
assert_contains "$output" "VTimer mask after clear: false" "HVF interrupt/timer live output"
assert_contains "$output" "VTimer offset set: true" "HVF interrupt/timer live output"
assert_contains "$output" "VTimer offset after set: 0x1000" "HVF interrupt/timer live output"
assert_contains "$output" "Interrupt/timer boundary observed: true" "HVF interrupt/timer live output"
assert_contains "$output" "vCPU destroyed: true" "HVF interrupt/timer live output"
assert_contains "$output" "VM destroyed: true" "HVF interrupt/timer live output"
assert_contains "$output" "Blockers: none" "HVF interrupt/timer live output"

echo "PASS: HVF interrupt/timer live opt-in smoke"
