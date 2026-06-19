#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if [[ "${BRIDGEVM_HVF_ALLOW_UEFI_PFLASH_MAP:-}" != "1" ]]; then
  echo "SKIP: set BRIDGEVM_HVF_ALLOW_UEFI_PFLASH_MAP=1 to create an empty HVF VM and map/unmap Windows UEFI pflash slots"
  exit 0
fi

STORE="$(mktemp -d "/tmp/bridgevm-winarm-hvf-pflash-hvf-map-live.XXXXXX")"
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

output="$("$runner" --windows-pflash-hvf-map-probe --firmware "$FIRMWARE" --vars-template "$VARS_TEMPLATE" --vars "$VARS" --create-vars --allow-map 2>&1)" \
  || fail "hvf-runner --windows-pflash-hvf-map-probe --allow-map failed: $output"

assert_contains "$output" "Windows 11 Arm HVF UEFI pflash HVF map/unmap probe" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "QEMU: not used" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "Apple VZ: not used" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "Guest execution: not entered" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "Allowed: true" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "Attempted: true" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "VM created: true" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "Firmware memory allocated: true" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "Vars memory allocated: true" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "Firmware memory populated: true" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "Vars memory populated: true" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "Firmware memory mapped: true" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "Vars memory mapped: true" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "Firmware memory unmapped: true" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "Vars memory unmapped: true" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "Firmware memory deallocated: true" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "Vars memory deallocated: true" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "VM destroyed: true" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "Pflash map verified: true" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "Firmware slot IPA: 0x8000000" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "Vars slot IPA: 0xc000000" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "Firmware source bytes: 0x20000" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "Vars source bytes: 0x10000" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "Firmware map flags: read|exec" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "Vars map flags: read|write" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "VM create status name: HV_SUCCESS" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "Firmware allocate status name: HV_SUCCESS" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "Vars allocate status name: HV_SUCCESS" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "Firmware map status name: HV_SUCCESS" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "Vars map status name: HV_SUCCESS" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "Firmware unmap status name: HV_SUCCESS" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "Vars unmap status name: HV_SUCCESS" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "Firmware deallocate status name: HV_SUCCESS" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "Vars deallocate status name: HV_SUCCESS" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "VM destroy status name: HV_SUCCESS" "Windows Arm pflash HVF map live probe output"
assert_contains "$output" "Blockers: none" "Windows Arm pflash HVF map live probe output"

echo "PASS: Windows 11 Arm no-QEMU HVF pflash HVF map live opt-in smoke ($STORE)"
