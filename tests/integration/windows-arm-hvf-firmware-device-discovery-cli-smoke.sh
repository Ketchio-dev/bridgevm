#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-winarm-hvf-firmware-device-discovery-cli.XXXXXX")"
FAKE_BIN="$STORE/bin"
BACKEND_LOG="$STORE/backend-launch.log"
FIRMWARE="$STORE/AAVMF_CODE.fd"
VARS_TEMPLATE="$STORE/AAVMF_VARS.fd"
VARS="$STORE/win11-arm-vars.fd"
ISO="$STORE/Win11_Arm64.iso"
DISK="$STORE/windows-arm.raw"

# shellcheck source=/dev/null
source "$ROOT/tests/integration/windows-arm-hvf-pflash-fixtures.sh"

install_backend_guards "Windows Arm HVF firmware device-discovery CLI smoke"
write_uefi_fv_fixture "$FIRMWARE" 131072
write_uefi_fv_fixture "$VARS_TEMPLATE" 65536

output="$(cargo run -q -p bridgevm-cli -- hvf windows-firmware-device-discovery-probe --firmware "$FIRMWARE" --vars-template "$VARS_TEMPLATE" --vars "$VARS" --create-vars --iso "$ISO" --writable-disk "$DISK" 2>&1)" \
  || fail "bridgevm hvf windows-firmware-device-discovery-probe command failed: $output"

assert_contains "$output" "Windows 11 Arm HVF UEFI firmware device-discovery probe" "Windows Arm firmware device-discovery CLI output"
assert_contains "$output" "QEMU: not used" "Windows Arm firmware device-discovery CLI output"
assert_contains "$output" "Apple VZ: not used" "Windows Arm firmware device-discovery CLI output"
assert_contains "$output" "Windows boot: not claimed" "Windows Arm firmware device-discovery CLI output"
assert_contains "$output" "Underlying probe: windows-firmware-run-loop-probe" "Windows Arm firmware device-discovery CLI output"
assert_contains "$output" "Device discovery boundary reached: false" "Windows Arm firmware device-discovery CLI output"
assert_contains "$output" "Device discovery boundary status: not reached" "Windows Arm firmware device-discovery CLI output"
assert_contains "$output" "Device discovery ready: false" "Windows Arm firmware device-discovery CLI output"
assert_contains "$output" "Device discovery blocker: firmware has not reached a non-diagnostic MMIO/sysreg boundary yet" "Windows Arm firmware device-discovery CLI output"
assert_contains "$output" "Handled MMIO access count: 0" "Windows Arm firmware device-discovery CLI output"
assert_contains "$output" "Handled ICC access count: 0" "Windows Arm firmware device-discovery CLI output"
assert_contains "$output" "Underlying firmware run-loop report:" "Windows Arm firmware device-discovery CLI output"
assert_contains "$output" "Allowed: false" "Windows Arm firmware device-discovery CLI output"
assert_contains "$output" "Attempted: false" "Windows Arm firmware device-discovery CLI output"
assert_contains "$output" "Run loop attempted: false" "Windows Arm firmware device-discovery CLI output"
assert_contains "$output" "Low pflash alias requested: true" "Windows Arm firmware device-discovery CLI output"
assert_contains "$output" "Low vector diagnostic page repair requested: true" "Windows Arm firmware device-discovery CLI output"
assert_contains "$output" "Continue after low-vector repair requested: true" "Windows Arm firmware device-discovery CLI output"
assert_contains "$output" "Interrupt/timer wiring requested: true" "Windows Arm firmware device-discovery CLI output"
assert_contains "$output" "Stop at first post-repair device boundary requested: true" "Windows Arm firmware device-discovery CLI output"
assert_contains "$output" "Pflash map verified: true" "Windows Arm firmware device-discovery CLI output"
assert_contains "$output" "Platform DTB magic verified: true" "Windows Arm firmware device-discovery CLI output"
assert_contains "$output" "Firmware block devices:" "Windows Arm firmware device-discovery CLI output"
assert_contains "$output" "Installer ISO path: $ISO" "Windows Arm firmware device-discovery CLI output"
assert_contains "$output" "Writable target disk path: $DISK" "Windows Arm firmware device-discovery CLI output"
assert_contains "$output" "set BRIDGEVM_HVF_ALLOW_UEFI_FIRMWARE_RUN_LOOP=1 or pass --allow-loop" "Windows Arm firmware device-discovery CLI output"
assert_not_contains "$output" "qemu-system" "Windows Arm firmware device-discovery CLI output"
assert_not_matches "$output" '[0-9]+([.][0-9]+)?%' "Windows Arm firmware device-discovery CLI output"
[[ -f "$VARS" ]] || fail "expected mutable vars file to be created"
cmp -s "$VARS_TEMPLATE" "$VARS" || fail "created vars store does not match template"
assert_no_backend_launch

echo "PASS: Windows 11 Arm no-QEMU HVF firmware device-discovery CLI smoke ($STORE)"
