#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-winarm-hvf-reset-vector-entry-runner.XXXXXX")"
FAKE_BIN="$STORE/bin"
BACKEND_LOG="$STORE/backend-launch.log"
FIRMWARE="$STORE/AAVMF_CODE.fd"
VARS_TEMPLATE="$STORE/AAVMF_VARS.fd"
VARS="$STORE/win11-arm-vars.fd"

# shellcheck source=/dev/null
source "$ROOT/tests/integration/windows-arm-hvf-pflash-fixtures.sh"

install_backend_guards "Windows Arm HVF reset-vector entry runner smoke"
write_uefi_fv_fixture "$FIRMWARE" 131072
write_uefi_fv_fixture "$VARS_TEMPLATE" 65536

output="$(cargo run -q -p hvf-runner -- --windows-reset-vector-entry-probe --firmware "$FIRMWARE" --vars-template "$VARS_TEMPLATE" --vars "$VARS" --create-vars 2>&1)" \
  || fail "hvf-runner --windows-reset-vector-entry-probe command failed: $output"

assert_contains "$output" "Windows 11 Arm HVF UEFI reset-vector entry probe" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "QEMU: not used" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "Apple VZ: not used" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "Guest execution: UEFI reset vector entered under watchdog" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "Windows boot: not claimed" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "Allowed: false" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "Attempted: false" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "VM created: false" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "Firmware memory allocated: false" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "Vars memory allocated: false" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "Firmware memory mapped: false" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "Vars memory mapped: false" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "vCPU created: false" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "PC set: false" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "CPSR set: false" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "Run attempted: false" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "Reset-vector entry observed: false" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "Firmware progress observed: false" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "VM destroyed: false" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "Pflash map verified: true" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "Reset vector IPA: 0x8000000" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "Firmware slot IPA: 0x8000000" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "Vars slot IPA: 0xc000000" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "Slot bytes: 0x4000000" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "Firmware source bytes: 0x20000" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "Vars source bytes: 0x10000" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "Firmware map flags: read|exec" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "Vars map flags: read|write" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "VM create status name: not attempted" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "vCPU create status name: not attempted" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "PC set status name: not attempted" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "CPSR set status name: not attempted" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "Run status name: not attempted" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "Exit exception class name: not observed" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "Blockers:" "Windows Arm reset-vector entry runner output"
assert_contains "$output" "set BRIDGEVM_HVF_ALLOW_UEFI_RESET_VECTOR_ENTRY=1 or pass --allow-entry" "Windows Arm reset-vector entry runner output"
assert_not_contains "$output" "qemu-system" "Windows Arm reset-vector entry runner output"
assert_not_matches "$output" '[0-9]+([.][0-9]+)?%' "Windows Arm reset-vector entry runner output"
[[ -f "$VARS" ]] || fail "expected mutable vars file to be created"
cmp -s "$VARS_TEMPLATE" "$VARS" || fail "created vars store does not match template"
assert_no_backend_launch

echo "PASS: Windows 11 Arm no-QEMU HVF reset-vector entry runner smoke ($STORE)"
