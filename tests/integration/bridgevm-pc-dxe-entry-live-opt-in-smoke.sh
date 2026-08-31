#!/usr/bin/env bash
# Opt-in live proof for BridgeVM PC variable restore and platform tables.
set -euo pipefail

if [[ "${BRIDGEVM_HVF_ALLOW_LIVE_BRIDGEVM_PC_DXE_ENTRY:-0}" != "1" ]]; then
  echo "SKIP: set BRIDGEVM_HVF_ALLOW_LIVE_BRIDGEVM_PC_DXE_ENTRY=1 to run the BridgeVM PC variable-restore proof"
  exit 0
fi
if [[ "$(sysctl -n kern.hv_support 2>/dev/null || echo 0)" != "1" ]]; then
  echo "SKIP: kern.hv_support != 1 (no Hypervisor.framework on this host)"
  exit 0
fi

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
EDK2="${BRIDGEVM_PINNED_EDK2_ROOT:-$HOME/BridgeVM-Workspace/deps/tianocore-edk2-b03a21a}"
[[ -d "$EDK2/.git" ]] || { echo "missing pinned EDK2 checkout: $EDK2" >&2; exit 1; }
WORK="$(mktemp -d "/tmp/bridgevm-pc-dxe-entry-live.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
"$ROOT/scripts/build-bridgevm-pc-dxe-entry-firmware.sh" "$EDK2" "$WORK/artifacts"
FD="$WORK/artifacts/BridgeVmPcDxeEntry.fd"

cd "$ROOT"
cargo build -q -p bridgevm-hvf --example bridgevm_pc_dxe_entry_live
BIN="target/debug/examples/bridgevm_pc_dxe_entry_live"
codesign --sign - --entitlements apps/macos/HvfRunner.entitlements --force "$BIN"

OUT="$($BIN "$FD")"
echo "$OUT"
echo "$OUT" | grep -q "BridgeVM Virtual ARM PC variable restore probe: PASS"
echo "$OUT" | grep -q "board=com.ketchio.bridgevm.virtual-arm-pc abi=1"
echo "$OUT" | grep -q "reset_vector=0x0 firmware_size=0x4000000 ram=0x100000000"
echo "$OUT" | grep -Eq "sec_result=1 hob_count=8 hob_list_gpa=0x100004000 hob_list_size=320 dxe_result=11 .*runtime_crc32=0x3f6f728d variable_state=2 variable_attributes=0x7 .*configuration_entries=[0-9]+ acpi=0x26001000 smbios=0x2600c000"
echo "$OUT" | grep -q "firmware_sha256=37c659e4ec70050790607ab58ec8eb9066284f13eedccb50795cf4623c642172"
initial="$(awk -F= '/^vars_initial_sha256=/{print $2}' <<<"$OUT")"
written="$(awk -F= '/^vars_written_sha256=/{print $2}' <<<"$OUT")"
restored="$(awk -F= '/^vars_restored_sha256=/{print $2}' <<<"$OUT")"
[[ "$initial" != "$written" && "$written" == "$restored" && ${#restored} -eq 64 ]]
echo "$OUT" | grep -q "LIVE PROOF: a recreated HVF VM restored the non-volatile UEFI variable"
echo "binary_sha256=$(shasum -a 256 "$BIN" | awk '{print $1}')"
echo "PASS: BridgeVM Virtual ARM PC restored its variable and retained platform tables"
