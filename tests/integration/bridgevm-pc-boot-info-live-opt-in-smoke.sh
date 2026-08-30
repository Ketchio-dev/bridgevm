#!/usr/bin/env bash
# Opt-in live proof for the BridgeVM Virtual ARM PC v1 boot-info GPA. It boots
# only a bounded EL1 reader, not firmware or Windows.
set -euo pipefail

if [[ "${BRIDGEVM_HVF_ALLOW_LIVE_BRIDGEVM_PC_BOOT_INFO:-0}" != "1" ]]; then
  echo "SKIP: set BRIDGEVM_HVF_ALLOW_LIVE_BRIDGEVM_PC_BOOT_INFO=1 to run the BridgeVM PC boot-info proof"
  exit 0
fi
if [[ "$(sysctl -n kern.hv_support 2>/dev/null || echo 0)" != "1" ]]; then
  echo "SKIP: kern.hv_support != 1 (no Hypervisor.framework on this host)"
  exit 0
fi

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

cargo build -q -p bridgevm-hvf --example bridgevm_pc_boot_info_live
BIN="target/debug/examples/bridgevm_pc_boot_info_live"

ENTDIR="$(mktemp -d "/tmp/bridgevm-pc-boot-info.XXXXXX")"
trap 'rm -rf "$ENTDIR"' EXIT
ENT="$ENTDIR/hv.entitlements"
cat > "$ENT" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>com.apple.security.hypervisor</key><true/></dict></plist>
PLIST
codesign --sign - --entitlements "$ENT" --force "$BIN"

OUT="$($BIN)"
echo "$OUT"
echo "$OUT" | grep -q "BridgeVM Virtual ARM PC boot-info probe: PASS"
echo "$OUT" | grep -q "board=com.ketchio.bridgevm.virtual-arm-pc abi=1"
echo "$OUT" | grep -q "boot_info=0x26000000 size=0x10000 ram=0x100000000"
echo "$OUT" | grep -q "guest_result=1 header_checksum=0 rsdp=0x26001000 xsdt=0x26002000"
echo "$OUT" | grep -q "LIVE PROOF: EL1 read BridgeVM boot-info v1 and followed its RSDP to XSDT"
echo "binary_sha256=$(shasum -a 256 "$BIN" | awk '{print $1}')"
echo "PASS: BridgeVM Virtual ARM PC v1 boot-info is readable by an EL1 guest"
