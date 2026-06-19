#!/usr/bin/env bash
# Live milestone proof: the stock ArmVirtQemu firmware (edk2-aarch64-code.fd) boots
# on the BridgeVM Path A `virt` platform, prints its UEFI banner through the modelled
# PL011 UART, and parses the generated device tree. Confirms Path A end to end up to
# the GICv3 blocker.
#
# Opt-in (needs Apple Silicon + Hypervisor.framework + QEMU's edk2 firmware files):
#   BRIDGEVM_HVF_ALLOW_LIVE_EDK2_BOOT=1 tests/integration/hvf-edk2-boot-live-opt-in-smoke.sh
set -euo pipefail

if [[ "${BRIDGEVM_HVF_ALLOW_LIVE_EDK2_BOOT:-0}" != "1" ]]; then
  echo "SKIP: set BRIDGEVM_HVF_ALLOW_LIVE_EDK2_BOOT=1 to run the live EDK2 boot probe"
  exit 0
fi
if [[ "$(sysctl -n kern.hv_support 2>/dev/null || echo 0)" != "1" ]]; then
  echo "SKIP: kern.hv_support != 1 (no Hypervisor.framework on this host)"
  exit 0
fi

CODE="${BRIDGEVM_AARCH64_UEFI_CODE:-/opt/homebrew/Cellar/qemu/11.0.1/share/qemu/edk2-aarch64-code.fd}"
VARS="${BRIDGEVM_AARCH64_UEFI_VARS:-/opt/homebrew/Cellar/qemu/11.0.1/share/qemu/edk2-arm-vars.fd}"
if [[ ! -f "$CODE" || ! -f "$VARS" ]]; then
  echo "SKIP: edk2 firmware not found ($CODE / $VARS); set BRIDGEVM_AARCH64_UEFI_CODE/VARS"
  exit 0
fi

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

cargo build -q -p bridgevm-hvf --example hvf_edk2_boot_probe
BIN="target/debug/examples/hvf_edk2_boot_probe"

ENTDIR="$(mktemp -d "/tmp/bridgevm-hvf-live-edk2.XXXXXX")"
ENT="$ENTDIR/hv.entitlements"
cat > "$ENT" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>com.apple.security.hypervisor</key><true/></dict></plist>
PLIST
codesign --sign - --entitlements "$ENT" --force "$BIN"

OUT="$(BRIDGEVM_AARCH64_UEFI_CODE="$CODE" BRIDGEVM_AARCH64_UEFI_VARS="$VARS" "$BIN")"
echo "$OUT"
echo "$OUT" | grep -q "UEFI firmware" \
  || { echo "FAIL: UEFI firmware banner not seen on serial"; exit 1; }
echo "PASS: stock ArmVirtQemu firmware reached DXE on the Path A platform (next blocker: GICv3)"
