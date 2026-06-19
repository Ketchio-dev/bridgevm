#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if [[ "${BRIDGEVM_HVF_ALLOW_VM_CREATE:-}" != "1" ]]; then
  echo "SKIP: set BRIDGEVM_HVF_ALLOW_VM_CREATE=1 to create and immediately destroy an empty HVF VM and vCPU"
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

output="$("$runner" --vcpu-probe --allow-create 2>&1)" \
  || fail "hvf-runner --vcpu-probe --allow-create failed: $output"

assert_contains "$output" "HVF vCPU create/destroy probe" "HVF vCPU live probe output"
assert_contains "$output" "Allowed: true" "HVF vCPU live probe output"
assert_contains "$output" "Attempted: true" "HVF vCPU live probe output"
assert_contains "$output" "QEMU: not used" "HVF vCPU live probe output"
assert_contains "$output" "Apple VZ: not used" "HVF vCPU live probe output"

if [[ "$output" != *"VM created: true"* || "$output" != *"vCPU created: true"* || "$output" != *"vCPU destroyed: true"* || "$output" != *"VM destroyed: true"* ]]; then
  fail "empty HVF vCPU lifecycle was not created and destroyed cleanly: $output"
fi

echo "PASS: HVF vCPU create/destroy live opt-in smoke"
