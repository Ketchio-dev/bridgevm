#!/usr/bin/env bash
# Opt-in live proof for BridgeVM PC reset-to-DXE Core dispatch.
set -euo pipefail

if [[ "${BRIDGEVM_HVF_ALLOW_LIVE_BRIDGEVM_PC_DXE_ENTRY:-0}" != "1" ]]; then
  echo "SKIP: set BRIDGEVM_HVF_ALLOW_LIVE_BRIDGEVM_PC_DXE_ENTRY=1 to run the BridgeVM PC DXE-entry proof"
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
ENT="$WORK/hv.entitlements"
cat > "$ENT" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>com.apple.security.hypervisor</key><true/></dict></plist>
PLIST
codesign --sign - --entitlements "$ENT" --force "$BIN"

OUT="$($BIN "$FD")"
echo "$OUT"
echo "$OUT" | grep -q "BridgeVM Virtual ARM PC DXE-dispatch probe: PASS"
echo "$OUT" | grep -q "board=com.ketchio.bridgevm.virtual-arm-pc abi=1"
echo "$OUT" | grep -q "reset_vector=0x0 firmware_size=0x4000000 ram=0x100000000"
echo "$OUT" | grep -Eq "sec_result=1 hob_count=7 hob_list_gpa=0x100004000 hob_list_size=272 dxe_result=8 system_table=0x[0-9a-f]+ system_table_signature=0x5453595320494249"
echo "$OUT" | grep -q "firmware_sha256=57c134b8f3f42bb9bb020936d4d87926b0d6563bfa0339bb110996a6e4ed6da6"
echo "$OUT" | grep -q "LIVE PROOF: DXE Core created the UEFI system table and dispatched the BridgeVM probe"
echo "binary_sha256=$(shasum -a 256 "$BIN" | awk '{print $1}')"
echo "PASS: BridgeVM Virtual ARM PC entered DXE Core and dispatched its bounded marker"
