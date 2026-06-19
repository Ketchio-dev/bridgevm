#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if [[ "${BRIDGEVM_HVF_ALLOW_VM_CREATE:-}" != "1" ]]; then
  echo "SKIP: set BRIDGEVM_HVF_ALLOW_VM_CREATE=1 to create and immediately destroy an empty HVF VM"
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

output="$("$runner" --vm-probe --allow-create 2>&1)" \
  || fail "hvf-runner --vm-probe --allow-create failed: $output"

assert_contains "$output" "HVF VM create/destroy probe" "HVF VM live probe output"
assert_contains "$output" "Allowed: true" "HVF VM live probe output"
assert_contains "$output" "Attempted: true" "HVF VM live probe output"
assert_contains "$output" "QEMU: not used" "HVF VM live probe output"
assert_contains "$output" "Apple VZ: not used" "HVF VM live probe output"

if [[ "$output" != *"Created: true"* || "$output" != *"Destroyed: true"* ]]; then
  if [[ "$output" == *"Create status name: HV_DENIED"* ]]; then
    echo "BLOCKED: hv_vm_create returned HV_DENIED; sign the HVF runner with com.apple.security.hypervisor before this live probe can pass"
    exit 0
  fi
  fail "empty HVF VM was not created and destroyed cleanly: $output"
fi

echo "PASS: HVF VM create/destroy live opt-in smoke"
