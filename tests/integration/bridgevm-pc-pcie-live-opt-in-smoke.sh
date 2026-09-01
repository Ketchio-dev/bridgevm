#!/usr/bin/env bash
# Opt-in live proof for BridgeVM Virtual ARM PC PCIe ECAM enumeration.
set -euo pipefail

if [[ "${BRIDGEVM_HVF_ALLOW_LIVE_BRIDGEVM_PC_PCIE:-0}" != "1" ]]; then
  echo "SKIP: set BRIDGEVM_HVF_ALLOW_LIVE_BRIDGEVM_PC_PCIE=1 to run the BridgeVM PC PCIe probe"
  exit 0
fi
if [[ "$(sysctl -n kern.hv_support 2>/dev/null || echo 0)" != "1" ]]; then
  echo "SKIP: kern.hv_support != 1 (no Hypervisor.framework on this host)"
  exit 0
fi

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"
cargo build -q -p bridgevm-hvf --example bridgevm_pc_pcie_live --locked
BIN="target/debug/examples/bridgevm_pc_pcie_live"
codesign --sign - --entitlements apps/macos/HvfRunner.entitlements --force "$BIN"
OUT="$($BIN)"
echo "$OUT"
echo "$OUT" | grep -q "BridgeVM Virtual ARM PC PCIe enumeration probe: PASS"
echo "$OUT" | grep -q "ecam_base=0x40000000 ecam_size=0x10000000 reads=8"
for role in host-bridge system-storage usb-input installer-media network display guest-agent audio; do
  echo "$OUT" | grep -q "role=$role"
done
echo "$OUT" | grep -q "LIVE PROOF: guest MMIO enumerated all BridgeVM PC v1 PCIe identities"
echo "binary_sha256=$(shasum -a 256 "$BIN" | awk '{print $1}')"
echo "PASS: BridgeVM Virtual ARM PC guest enumerated eight PCIe functions"
