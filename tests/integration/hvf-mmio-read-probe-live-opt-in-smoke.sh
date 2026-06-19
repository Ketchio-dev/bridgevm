#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if [[ "${BRIDGEVM_HVF_ALLOW_MMIO_READ:-}" != "1" ]]; then
  echo "SKIP: set BRIDGEVM_HVF_ALLOW_MMIO_READ=1 to run one unmapped LDR read and observe an MMIO/data-abort exit"
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

output="$("$runner" --mmio-read-probe --allow-mmio 2>&1)" \
  || fail "hvf-runner --mmio-read-probe --allow-mmio failed: $output"

assert_contains "$output" "HVF MMIO read exit probe" "HVF MMIO read live probe output"
assert_contains "$output" "Allowed: true" "HVF MMIO read live probe output"
assert_contains "$output" "Attempted: true" "HVF MMIO read live probe output"
assert_contains "$output" "QEMU: not used" "HVF MMIO read live probe output"
assert_contains "$output" "Apple VZ: not used" "HVF MMIO read live probe output"
assert_contains "$output" "Guest execution: one unmapped LDR read with watchdog" "HVF MMIO read live probe output"
assert_contains "$output" "Address register set: true" "HVF MMIO read live probe output"
assert_contains "$output" "Run attempted: true" "HVF MMIO read live probe output"
assert_contains "$output" "MMIO exit observed: true" "HVF MMIO read live probe output"
assert_contains "$output" "MMIO IPA: 0x50000000" "HVF MMIO read live probe output"
assert_contains "$output" "Run status name: HV_SUCCESS" "HVF MMIO read live probe output"
assert_contains "$output" "Exit reason name: HV_EXIT_REASON_EXCEPTION" "HVF MMIO read live probe output"
assert_contains "$output" "Blockers: none" "HVF MMIO read live probe output"

echo "PASS: HVF MMIO read live opt-in smoke"
