#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

env_truthy() {
  local value
  value="$(printf '%s' "${1:-}" | tr '[:upper:]' '[:lower:]')"
  case "$value" in
    1|true|yes|on) return 0 ;;
    *) return 1 ;;
  esac
}

if ! env_truthy "${BRIDGEVM_HVF_ALLOW_VTIMER_EXIT:-}"; then
  echo "SKIP: set BRIDGEVM_HVF_ALLOW_VTIMER_EXIT=1|true|yes|on to map a tiny WFI guest and observe one HV_EXIT_REASON_VTIMER_ACTIVATED boundary"
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

output="$("$runner" --vtimer-exit-probe --allow-vtimer-exit 2>&1)" \
  || fail "hvf-runner --vtimer-exit-probe --allow-vtimer-exit failed: $output"

assert_contains "$output" "HVF VTimer exit probe" "HVF VTimer exit live output"
assert_contains "$output" "QEMU: not used" "HVF VTimer exit live output"
assert_contains "$output" "Apple VZ: not used" "HVF VTimer exit live output"
assert_contains "$output" "Guest execution: WFI wait loop with host-programmed virtual timer" "HVF VTimer exit live output"
assert_contains "$output" "Allowed: true" "HVF VTimer exit live output"
assert_contains "$output" "Attempted: true" "HVF VTimer exit live output"
assert_contains "$output" "VM created: true" "HVF VTimer exit live output"
assert_contains "$output" "Memory allocated: true" "HVF VTimer exit live output"
assert_contains "$output" "Memory mapped: true" "HVF VTimer exit live output"
assert_contains "$output" "vCPU created: true" "HVF VTimer exit live output"
assert_contains "$output" "PC set: true" "HVF VTimer exit live output"
assert_contains "$output" "CPSR set: true" "HVF VTimer exit live output"
assert_contains "$output" "VTimer offset set: true" "HVF VTimer exit live output"
assert_contains "$output" "CNTV_CVAL_EL0 set: true" "HVF VTimer exit live output"
assert_contains "$output" "CNTV_CTL_EL0 set: true" "HVF VTimer exit live output"
assert_contains "$output" "VTimer unmasked: true" "HVF VTimer exit live output"
assert_contains "$output" "Run attempted: true" "HVF VTimer exit live output"
assert_contains "$output" "VTimer exit observed: true" "HVF VTimer exit live output"
assert_contains "$output" "Pending IRQ injected: true" "HVF VTimer exit live output"
assert_contains "$output" "VTimer mask observed after exit: true" "HVF VTimer exit live output"
assert_contains "$output" "VTimer unmasked after exit: true" "HVF VTimer exit live output"
assert_contains "$output" "Instructions: WFI; HVC #0" "HVF VTimer exit live output"
assert_contains "$output" "Exit reason name: HV_EXIT_REASON_VTIMER_ACTIVATED" "HVF VTimer exit live output"
assert_contains "$output" "Pending IRQ set status name: HV_SUCCESS" "HVF VTimer exit live output"
assert_contains "$output" "VTimer mask get after exit status name: HV_SUCCESS" "HVF VTimer exit live output"
assert_contains "$output" "VTimer unmask after exit status name: HV_SUCCESS" "HVF VTimer exit live output"
assert_contains "$output" "vCPU destroyed: true" "HVF VTimer exit live output"
assert_contains "$output" "Memory unmapped: true" "HVF VTimer exit live output"
assert_contains "$output" "VM destroyed: true" "HVF VTimer exit live output"
assert_contains "$output" "Memory deallocated: true" "HVF VTimer exit live output"
assert_contains "$output" "Blockers: none" "HVF VTimer exit live output"

echo "PASS: HVF VTimer exit live opt-in smoke"
