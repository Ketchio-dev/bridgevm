#!/usr/bin/env bash
# Live end-to-end proof that the Path A platform works on real Hypervisor.framework:
# a guest vCPU MMIO read of the fw_cfg signature is trapped, decoded, and routed
# through VirtPlatform::on_mmio -> fwcfg, and the guest observes 'Q' (0x51).
#
# Opt-in (needs Apple Silicon + Hypervisor.framework + ad-hoc entitlement signing):
#   BRIDGEVM_HVF_ALLOW_LIVE_FW_CFG_MMIO=1 tests/integration/hvf-fw-cfg-mmio-live-opt-in-smoke.sh
set -euo pipefail

if [[ "${BRIDGEVM_HVF_ALLOW_LIVE_FW_CFG_MMIO:-0}" != "1" ]]; then
  echo "SKIP: set BRIDGEVM_HVF_ALLOW_LIVE_FW_CFG_MMIO=1 to run the live fw_cfg MMIO proof"
  exit 0
fi
if [[ "$(sysctl -n kern.hv_support 2>/dev/null || echo 0)" != "1" ]]; then
  echo "SKIP: kern.hv_support != 1 (no Hypervisor.framework on this host)"
  exit 0
fi

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

cargo build -q -p bridgevm-hvf --example hvf_fw_cfg_live
BIN="target/debug/examples/hvf_fw_cfg_live"

ENTDIR="$(mktemp -d "/tmp/bridgevm-hvf-live-fwcfg.XXXXXX")"
ENT="$ENTDIR/hv.entitlements"
cat > "$ENT" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>com.apple.security.hypervisor</key><true/></dict></plist>
PLIST

# Ad-hoc sign with the hypervisor entitlement so the binary may call hv_* APIs.
codesign --sign - --entitlements "$ENT" --force "$BIN"

OUT="$("$BIN")"
echo "$OUT"
echo "$OUT" | grep -q "LIVE PROOF: real guest MMIO" \
  || { echo "FAIL: live proof line missing"; exit 1; }
echo "PASS: live fw_cfg MMIO routed through VirtPlatform on real Hypervisor.framework"
