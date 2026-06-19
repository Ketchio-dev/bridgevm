#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if [[ "${BRIDGEVM_HVF_ALLOW_EXIT_LOOP:-}" != "1" ]]; then
  echo "SKIP: set BRIDGEVM_HVF_ALLOW_EXIT_LOOP=1 to run two mapped HVC exits with an explicit PC advance"
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

output="$("$runner" --guest-exit-loop-probe --allow-loop 2>&1)" \
  || fail "hvf-runner --guest-exit-loop-probe --allow-loop failed: $output"

assert_contains "$output" "HVF guest exit loop probe" "HVF guest exit loop live probe output"
assert_contains "$output" "Allowed: true" "HVF guest exit loop live probe output"
assert_contains "$output" "Attempted: true" "HVF guest exit loop live probe output"
assert_contains "$output" "QEMU: not used" "HVF guest exit loop live probe output"
assert_contains "$output" "Apple VZ: not used" "HVF guest exit loop live probe output"
assert_contains "$output" "Guest execution: two HVC instructions with PC advance watchdog" "HVF guest exit loop live probe output"
assert_contains "$output" "VM created: true" "HVF guest exit loop live probe output"
assert_contains "$output" "Memory mapped: true" "HVF guest exit loop live probe output"
assert_contains "$output" "vCPU created: true" "HVF guest exit loop live probe output"
assert_contains "$output" "Initial PC set: true" "HVF guest exit loop live probe output"
assert_contains "$output" "CPSR set: true" "HVF guest exit loop live probe output"
assert_contains "$output" "First run attempted: true" "HVF guest exit loop live probe output"
assert_contains "$output" "First exit observed: true" "HVF guest exit loop live probe output"
assert_contains "$output" "PC read after first exit: true" "HVF guest exit loop live probe output"
assert_contains "$output" "PC advanced: true" "HVF guest exit loop live probe output"
assert_contains "$output" "Second run attempted: true" "HVF guest exit loop live probe output"
assert_contains "$output" "Second exit observed: true" "HVF guest exit loop live probe output"
assert_contains "$output" "Exit loop observed: true" "HVF guest exit loop live probe output"
assert_contains "$output" "First exit syndrome: 0x5a000000" "HVF guest exit loop live probe output"
assert_contains "$output" "Second exit syndrome: 0x5a000001" "HVF guest exit loop live probe output"
assert_contains "$output" "Blockers: none" "HVF guest exit loop live probe output"

echo "PASS: HVF guest exit loop live opt-in smoke"
