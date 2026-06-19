#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if [[ "${BRIDGEVM_HVF_ALLOW_MEMORY_MAP:-}" != "1" ]]; then
  echo "SKIP: set BRIDGEVM_HVF_ALLOW_MEMORY_MAP=1 to create an empty HVF VM and map/unmap one guest RAM page"
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

output="$("$runner" --memory-map-probe --allow-map 2>&1)" \
  || fail "hvf-runner --memory-map-probe --allow-map failed: $output"

assert_contains "$output" "HVF memory map/unmap probe" "HVF memory map live probe output"
assert_contains "$output" "Allowed: true" "HVF memory map live probe output"
assert_contains "$output" "Attempted: true" "HVF memory map live probe output"
assert_contains "$output" "QEMU: not used" "HVF memory map live probe output"
assert_contains "$output" "Apple VZ: not used" "HVF memory map live probe output"
assert_contains "$output" "Guest execution: not entered" "HVF memory map live probe output"
assert_contains "$output" "Guest IPA start: 0x40000000" "HVF memory map live probe output"
assert_contains "$output" "Bytes: 16384" "HVF memory map live probe output"
assert_contains "$output" "VM created: true" "HVF memory map live probe output"
assert_contains "$output" "Memory allocated: true" "HVF memory map live probe output"
assert_contains "$output" "Memory mapped: true" "HVF memory map live probe output"
assert_contains "$output" "Memory unmapped: true" "HVF memory map live probe output"
assert_contains "$output" "Memory deallocated: true" "HVF memory map live probe output"
assert_contains "$output" "VM destroyed: true" "HVF memory map live probe output"

echo "PASS: HVF memory map/unmap live opt-in smoke"
