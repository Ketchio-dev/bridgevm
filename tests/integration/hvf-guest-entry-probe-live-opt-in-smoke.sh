#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if [[ "${BRIDGEVM_HVF_ALLOW_GUEST_ENTRY:-}" != "1" ]]; then
  echo "SKIP: set BRIDGEVM_HVF_ALLOW_GUEST_ENTRY=1 to enter one mapped HVC instruction with a watchdog"
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

output="$("$runner" --guest-entry-probe --allow-entry 2>&1)" \
  || fail "hvf-runner --guest-entry-probe --allow-entry failed: $output"

assert_contains "$output" "HVF guest entry probe" "HVF guest entry live probe output"
assert_contains "$output" "Allowed: true" "HVF guest entry live probe output"
assert_contains "$output" "Attempted: true" "HVF guest entry live probe output"
assert_contains "$output" "QEMU: not used" "HVF guest entry live probe output"
assert_contains "$output" "Apple VZ: not used" "HVF guest entry live probe output"
assert_contains "$output" "Guest execution: one HVC instruction with watchdog" "HVF guest entry live probe output"
assert_contains "$output" "VM created: true" "HVF guest entry live probe output"
assert_contains "$output" "Memory allocated: true" "HVF guest entry live probe output"
assert_contains "$output" "Memory mapped: true" "HVF guest entry live probe output"
assert_contains "$output" "vCPU created: true" "HVF guest entry live probe output"
assert_contains "$output" "PC set: true" "HVF guest entry live probe output"
assert_contains "$output" "CPSR set: true" "HVF guest entry live probe output"
assert_contains "$output" "Run attempted: true" "HVF guest entry live probe output"
assert_contains "$output" "Entry boundary observed: true" "HVF guest entry live probe output"
assert_contains "$output" "Run status name: HV_SUCCESS" "HVF guest entry live probe output"
assert_contains "$output" "Exit reason name: HV_EXIT_REASON_EXCEPTION" "HVF guest entry live probe output"
assert_contains "$output" "vCPU destroyed: true" "HVF guest entry live probe output"
assert_contains "$output" "Memory unmapped: true" "HVF guest entry live probe output"
assert_contains "$output" "Memory deallocated: true" "HVF guest entry live probe output"
assert_contains "$output" "VM destroyed: true" "HVF guest entry live probe output"
assert_contains "$output" "Blockers: none" "HVF guest entry live probe output"

echo "PASS: HVF guest entry live opt-in smoke"
