#!/usr/bin/env bash
# Opt-in live proof for the BridgeVM Virtual ARM PC reset entry at GPA zero.
# It does not execute SEC, PEI, DXE, UEFI services or Windows.
set -euo pipefail

if [[ "${BRIDGEVM_HVF_ALLOW_LIVE_BRIDGEVM_PC_RESET_VECTOR:-0}" != "1" ]]; then
  echo "SKIP: set BRIDGEVM_HVF_ALLOW_LIVE_BRIDGEVM_PC_RESET_VECTOR=1 to run the BridgeVM PC reset-vector proof"
  exit 0
fi
if [[ "$(sysctl -n kern.hv_support 2>/dev/null || echo 0)" != "1" ]]; then
  echo "SKIP: kern.hv_support != 1 (no Hypervisor.framework on this host)"
  exit 0
fi

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
WORK="$(mktemp -d "/tmp/bridgevm-pc-reset-vector-live.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
"$ROOT/scripts/build-bridgevm-pc-reset-vector.sh" "$WORK/artifacts"
FD="$WORK/artifacts/BridgeVmPcResetVector.fd"

cd "$ROOT"
cargo build -q -p bridgevm-hvf --example bridgevm_pc_reset_vector_live
BIN="target/debug/examples/bridgevm_pc_reset_vector_live"
ENT="$WORK/hv.entitlements"
cat > "$ENT" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>com.apple.security.hypervisor</key><true/></dict></plist>
PLIST
codesign --sign - --entitlements "$ENT" --force "$BIN"

OUT="$($BIN "$FD")"
echo "$OUT"
echo "$OUT" | grep -q "BridgeVM Virtual ARM PC reset-vector probe: PASS"
echo "$OUT" | grep -q "board=com.ketchio.bridgevm.virtual-arm-pc abi=1"
echo "$OUT" | grep -q "reset_vector=0x0 firmware_size=0x4000000 ram=0x100000000"
echo "$OUT" | grep -q "result=1 result_gpa=0x100001000 boot_info=0x26000000"
echo "$OUT" | grep -q "LIVE PROOF: reset at GPA zero entered BridgeVM SEC C and validated boot-info v1"
echo "binary_sha256=$(shasum -a 256 "$BIN" | awk '{print $1}')"
echo "PASS: BridgeVM Virtual ARM PC v1 reset vector entered its bounded SEC continuation"
