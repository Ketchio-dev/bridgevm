#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if [[ "${BRIDGEVM_HVF_ALLOW_VCPU_RUN:-}" != "1" ]]; then
  echo "SKIP: set BRIDGEVM_HVF_ALLOW_VCPU_RUN=1 to pre-cancel and observe one hv_vcpu_run boundary"
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

output="$("$runner" --vcpu-run-probe --allow-run 2>&1)" \
  || fail "hvf-runner --vcpu-run-probe --allow-run failed: $output"

assert_contains "$output" "HVF vCPU run/cancel probe" "HVF vCPU run live probe output"
assert_contains "$output" "Allowed: true" "HVF vCPU run live probe output"
assert_contains "$output" "Attempted: true" "HVF vCPU run live probe output"
assert_contains "$output" "QEMU: not used" "HVF vCPU run live probe output"
assert_contains "$output" "Apple VZ: not used" "HVF vCPU run live probe output"
assert_contains "$output" "Guest execution: pre-canceled before entry" "HVF vCPU run live probe output"
assert_contains "$output" "Cancel requested: true" "HVF vCPU run live probe output"
assert_contains "$output" "Run attempted: true" "HVF vCPU run live probe output"
assert_contains "$output" "Run boundary observed: true" "HVF vCPU run live probe output"
assert_contains "$output" "Exit reason name: HV_EXIT_REASON_CANCELED" "HVF vCPU run live probe output"

if [[ "$output" != *"VM created: true"* || "$output" != *"vCPU created: true"* || "$output" != *"vCPU destroyed: true"* || "$output" != *"VM destroyed: true"* ]]; then
  fail "empty HVF vCPU run boundary did not clean up VM/vCPU lifecycle: $output"
fi

echo "PASS: HVF vCPU run/cancel live opt-in smoke"
