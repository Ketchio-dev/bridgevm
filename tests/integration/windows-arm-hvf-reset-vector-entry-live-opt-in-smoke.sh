#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if [[ "${BRIDGEVM_HVF_ALLOW_UEFI_RESET_VECTOR_ENTRY:-}" != "1" ]]; then
  echo "SKIP: set BRIDGEVM_HVF_ALLOW_UEFI_RESET_VECTOR_ENTRY=1 to create an HVF VM, map Windows UEFI pflash, create one vCPU, and enter the reset vector"
  exit 0
fi

STORE="$(mktemp -d "/tmp/bridgevm-winarm-hvf-reset-vector-entry-live.XXXXXX")"
FIRMWARE="$STORE/AAVMF_CODE.fd"
VARS_TEMPLATE="$STORE/AAVMF_VARS.fd"
VARS="$STORE/win11-arm-vars.fd"

# shellcheck source=/dev/null
source "$ROOT/tests/integration/windows-arm-hvf-pflash-fixtures.sh"

write_uefi_fv_fixture "$FIRMWARE" 131072
write_uefi_fv_fixture "$VARS_TEMPLATE" 65536

if [[ -n "${BRIDGEVM_LIVE_HVF_RUNNER:-}" ]]; then
  runner="$(apps/macos/scripts/build-sign-hvf-runner.sh --verify-only "$BRIDGEVM_LIVE_HVF_RUNNER")" \
    || fail "configured HVF runner is not signed with the hypervisor entitlement"
else
  runner="$(apps/macos/scripts/build-sign-hvf-runner.sh)" \
    || fail "could not build/sign hvf-runner with the hypervisor entitlement"
fi

output="$("$runner" --windows-reset-vector-entry-probe --firmware "$FIRMWARE" --vars-template "$VARS_TEMPLATE" --vars "$VARS" --create-vars --allow-entry 2>&1)" \
  || fail "hvf-runner --windows-reset-vector-entry-probe --allow-entry failed: $output"

assert_contains "$output" "Windows 11 Arm HVF UEFI reset-vector entry probe" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "QEMU: not used" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Apple VZ: not used" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Guest execution: UEFI reset vector entered under watchdog" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Windows boot: not claimed" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Allowed: true" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Attempted: true" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "VM created: true" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Firmware memory allocated: true" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Vars memory allocated: true" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Firmware memory populated: true" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Vars memory populated: true" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Firmware memory mapped: true" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Vars memory mapped: true" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "vCPU created: true" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "PC set: true" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "CPSR set: true" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Run attempted: true" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Reset-vector entry observed: true" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Firmware progress observed:" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "vCPU destroyed: true" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Firmware memory unmapped: true" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Vars memory unmapped: true" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Firmware memory deallocated: true" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Vars memory deallocated: true" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "VM destroyed: true" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Pflash map verified: true" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Reset vector IPA: 0x8000000" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Firmware slot IPA: 0x8000000" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Vars slot IPA: 0xc000000" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Firmware source bytes: 0x20000" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Vars source bytes: 0x10000" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Firmware map flags: read|exec" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Vars map flags: read|write" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "VM create status name: HV_SUCCESS" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Firmware allocate status name: HV_SUCCESS" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Vars allocate status name: HV_SUCCESS" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Firmware map status name: HV_SUCCESS" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Vars map status name: HV_SUCCESS" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "vCPU create status name: HV_SUCCESS" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "PC set status name: HV_SUCCESS" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "CPSR set status name: HV_SUCCESS" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Run status name: HV_SUCCESS" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Exit exception class name:" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Firmware unmap status name: HV_SUCCESS" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Vars unmap status name: HV_SUCCESS" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Firmware deallocate status name: HV_SUCCESS" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Vars deallocate status name: HV_SUCCESS" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "VM destroy status name: HV_SUCCESS" "Windows Arm reset-vector entry live probe output"
assert_contains "$output" "Blockers: none" "Windows Arm reset-vector entry live probe output"

echo "PASS: Windows 11 Arm no-QEMU HVF reset-vector entry live opt-in smoke ($STORE)"
