#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if [[ "${BRIDGEVM_HVF_ALLOW_REAL_EDK2_RESET_VECTOR_ENTRY:-}" != "1" ]]; then
  echo "SKIP: set BRIDGEVM_HVF_ALLOW_REAL_EDK2_RESET_VECTOR_ENTRY=1 to run the signed HVF reset-vector probe against a real AArch64 edk2 pflash image"
  exit 0
fi

STORE="$(mktemp -d "/tmp/bridgevm-winarm-hvf-real-edk2-reset-vector.XXXXXX")"
FIRMWARE="${BRIDGEVM_AARCH64_UEFI_CODE:-/opt/homebrew/Cellar/qemu/11.0.1/share/qemu/edk2-aarch64-code.fd}"
VARS_TEMPLATE="${BRIDGEVM_AARCH64_UEFI_VARS:-/opt/homebrew/Cellar/qemu/11.0.1/share/qemu/edk2-arm-vars.fd}"
VARS="$STORE/edk2-arm-vars.fd"

# shellcheck source=/dev/null
source "$ROOT/tests/integration/windows-arm-hvf-pflash-fixtures.sh"

if [[ ! -f "$FIRMWARE" || ! -f "$VARS_TEMPLATE" ]]; then
  echo "SKIP: real AArch64 edk2 firmware files were not found at '$FIRMWARE' and '$VARS_TEMPLATE'"
  exit 0
fi

if [[ -n "${BRIDGEVM_LIVE_HVF_RUNNER:-}" ]]; then
  runner_output="$(apps/macos/scripts/build-sign-hvf-runner.sh --verify-only "$BRIDGEVM_LIVE_HVF_RUNNER")" \
    || fail "configured HVF runner is not signed with the hypervisor entitlement"
else
  runner_output="$(apps/macos/scripts/build-sign-hvf-runner.sh)" \
    || fail "could not build/sign hvf-runner with the hypervisor entitlement"
fi
runner="$(printf '%s\n' "$runner_output" | tail -n 1)"
[[ -x "$runner" ]] || fail "signed hvf-runner path is not executable: $runner_output"

output="$("$runner" --windows-reset-vector-entry-probe --firmware "$FIRMWARE" --vars-template "$VARS_TEMPLATE" --vars "$VARS" --create-vars --allow-entry 2>&1)" \
  || fail "hvf-runner real edk2 reset-vector entry failed: $output"

assert_contains "$output" "Windows 11 Arm HVF UEFI reset-vector entry probe" "real edk2 reset-vector output"
assert_contains "$output" "QEMU: not used" "real edk2 reset-vector output"
assert_contains "$output" "Apple VZ: not used" "real edk2 reset-vector output"
assert_contains "$output" "Windows boot: not claimed" "real edk2 reset-vector output"
assert_contains "$output" "Allowed: true" "real edk2 reset-vector output"
assert_contains "$output" "Attempted: true" "real edk2 reset-vector output"
assert_contains "$output" "VM created: true" "real edk2 reset-vector output"
assert_contains "$output" "Firmware memory mapped: true" "real edk2 reset-vector output"
assert_contains "$output" "Vars memory mapped: true" "real edk2 reset-vector output"
assert_contains "$output" "vCPU created: true" "real edk2 reset-vector output"
assert_contains "$output" "PC set: true" "real edk2 reset-vector output"
assert_contains "$output" "CPSR set: true" "real edk2 reset-vector output"
assert_contains "$output" "Run attempted: true" "real edk2 reset-vector output"
assert_contains "$output" "Reset-vector entry observed: true" "real edk2 reset-vector output"
assert_contains "$output" "Firmware progress observed: true" "real edk2 reset-vector output"
assert_contains "$output" "Pflash map verified: true" "real edk2 reset-vector output"
assert_contains "$output" "Firmware source bytes: 0x4000000" "real edk2 reset-vector output"
assert_contains "$output" "Vars source bytes: 0x4000000" "real edk2 reset-vector output"
assert_contains "$output" "Run status name: HV_SUCCESS" "real edk2 reset-vector output"
assert_contains "$output" "Exit reason name: HV_EXIT_REASON_EXCEPTION" "real edk2 reset-vector output"
assert_not_contains "$output" "Exit exception class name: not observed" "real edk2 reset-vector output"
assert_not_contains "$output" "PC after run: 0x8000000" "real edk2 reset-vector output"
assert_contains "$output" "Blockers: none" "real edk2 reset-vector output"

if ! grep -Eq '^PC after run: 0x[0-9a-f]+' <<<"$output"; then
  fail "real edk2 reset-vector output did not report PC after run: $output"
fi

echo "PASS: Windows 11 Arm no-QEMU HVF real edk2 reset-vector live opt-in smoke ($STORE)"
