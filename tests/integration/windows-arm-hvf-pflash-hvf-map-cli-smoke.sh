#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-winarm-hvf-pflash-hvf-map-cli.XXXXXX")"
FAKE_BIN="$STORE/bin"
BACKEND_LOG="$STORE/backend-launch.log"
FIRMWARE="$STORE/AAVMF_CODE.fd"
VARS_TEMPLATE="$STORE/AAVMF_VARS.fd"
VARS="$STORE/win11-arm-vars.fd"

# shellcheck source=/dev/null
source "$ROOT/tests/integration/windows-arm-hvf-pflash-fixtures.sh"

install_backend_guards "Windows Arm HVF pflash HVF map CLI smoke"
write_uefi_fv_fixture "$FIRMWARE" 131072
write_uefi_fv_fixture "$VARS_TEMPLATE" 65536

output="$(cargo run -q -p bridgevm-cli -- hvf windows-pflash-hvf-map-probe --firmware "$FIRMWARE" --vars-template "$VARS_TEMPLATE" --vars "$VARS" --create-vars 2>&1)" \
  || fail "bridgevm hvf windows-pflash-hvf-map-probe command failed: $output"

assert_contains "$output" "Windows 11 Arm HVF UEFI pflash HVF map/unmap probe" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "QEMU: not used" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Apple VZ: not used" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Guest execution: not entered" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Allowed: false" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Attempted: false" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "VM created: false" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Firmware memory allocated: false" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Vars memory allocated: false" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Firmware memory populated: false" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Vars memory populated: false" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Firmware memory mapped: false" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Vars memory mapped: false" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Firmware memory unmapped: false" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Vars memory unmapped: false" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Firmware memory deallocated: false" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Vars memory deallocated: false" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "VM destroyed: false" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Pflash map verified: true" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Firmware slot IPA: 0x8000000" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Vars slot IPA: 0xc000000" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Slot bytes: 0x4000000" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Firmware source bytes: 0x20000" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Vars source bytes: 0x10000" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Firmware map flags: read|exec" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Vars map flags: read|write" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "VM create status: not attempted" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Firmware allocate status: not attempted" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Vars allocate status: not attempted" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Firmware map status: not attempted" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Vars map status: not attempted" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Firmware unmap status: not attempted" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Vars unmap status: not attempted" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Firmware deallocate status: not attempted" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Vars deallocate status: not attempted" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "VM destroy status: not attempted" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "Blockers:" "Windows Arm pflash HVF map CLI output"
assert_contains "$output" "set BRIDGEVM_HVF_ALLOW_UEFI_PFLASH_MAP=1 or pass --allow-map" "Windows Arm pflash HVF map CLI output"
assert_not_contains "$output" "qemu-system" "Windows Arm pflash HVF map CLI output"
assert_not_matches "$output" '[0-9]+([.][0-9]+)?%' "Windows Arm pflash HVF map CLI output"
[[ -f "$VARS" ]] || fail "expected mutable vars file to be created"
cmp -s "$VARS_TEMPLATE" "$VARS" || fail "created vars store does not match template"
assert_no_backend_launch

echo "PASS: Windows 11 Arm no-QEMU HVF pflash HVF map CLI smoke ($STORE)"
