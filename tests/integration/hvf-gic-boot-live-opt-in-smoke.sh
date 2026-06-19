#!/usr/bin/env bash
# Live milestone proof: with Apple's in-kernel GICv3 (hv_gic_create) and minimal
# PSCI, the stock ArmVirtQemu firmware boots through PEI, GIC init, and PSCI into
# the DXE phase on the Path A platform. (Beyond DXE the firmware currently hits an
# unresolved frontier: the redistributor MMIO is not served by Apple hv_gic, and a
# DXE control-flow fault follows — both under investigation.)
#
# Opt-in (needs Apple Silicon + Hypervisor.framework + QEMU's edk2 firmware files):
#   BRIDGEVM_HVF_ALLOW_LIVE_GIC_BOOT=1 tests/integration/hvf-gic-boot-live-opt-in-smoke.sh
set -euo pipefail

if [[ "${BRIDGEVM_HVF_ALLOW_LIVE_GIC_BOOT:-0}" != "1" ]]; then
  echo "SKIP: set BRIDGEVM_HVF_ALLOW_LIVE_GIC_BOOT=1 to run the live hv_gic boot probe"
  exit 0
fi
if [[ "$(sysctl -n kern.hv_support 2>/dev/null || echo 0)" != "1" ]]; then
  echo "SKIP: kern.hv_support != 1 (no Hypervisor.framework on this host)"
  exit 0
fi

CODE="${BRIDGEVM_AARCH64_UEFI_CODE:-/opt/homebrew/Cellar/qemu/11.0.1/share/qemu/edk2-aarch64-code.fd}"
VARS="${BRIDGEVM_AARCH64_UEFI_VARS:-/opt/homebrew/Cellar/qemu/11.0.1/share/qemu/edk2-arm-vars.fd}"
if [[ ! -f "$CODE" || ! -f "$VARS" ]]; then
  echo "SKIP: edk2 firmware not found; set BRIDGEVM_AARCH64_UEFI_CODE/VARS"
  exit 0
fi

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

cargo build -q -p bridgevm-hvf --example hvf_gic_boot_probe
BIN="target/debug/examples/hvf_gic_boot_probe"

ENTDIR="$(mktemp -d "/tmp/bridgevm-hvf-live-gic.XXXXXX")"
ENT="$ENTDIR/hv.entitlements"
cat > "$ENT" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>com.apple.security.hypervisor</key><true/></dict></plist>
PLIST
codesign --sign - --entitlements "$ENT" --force "$BIN"

OUT="$(BRIDGEVM_AARCH64_UEFI_CODE="$CODE" BRIDGEVM_AARCH64_UEFI_VARS="$VARS" "$BIN" || true)"
echo "$OUT" | grep -vE "Failed to install VirtIO" | head -20
echo "$OUT" | grep -q "hv_gic_create = 0x0" || { echo "FAIL: hv_gic_create did not succeed"; exit 1; }
echo "$OUT" | grep -q "UEFI firmware" || { echo "FAIL: firmware did not reach DXE banner"; exit 1; }
echo "PASS: firmware boots through Apple hv_gic + PSCI into DXE on the Path A platform"
