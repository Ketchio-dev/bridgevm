#!/usr/bin/env bash
# Live proof that the interrupt + architected-timer subsystem works through Apple
# hv_gic: a minimal EL1 guest configures the GICv3, enables the virtual-timer PPI,
# installs a vector, arms CNTV, and its IRQ handler runs (writing a flag) when the
# timer fires -- delivered in-kernel by hv_gic, no VTIMER_ACTIVATED exit.
#
# Opt-in (needs Apple Silicon + Hypervisor.framework + ad-hoc entitlement signing):
#   BRIDGEVM_HVF_ALLOW_LIVE_GIC_IRQ=1 tests/integration/hvf-gic-irq-live-opt-in-smoke.sh
set -euo pipefail

if [[ "${BRIDGEVM_HVF_ALLOW_LIVE_GIC_IRQ:-0}" != "1" ]]; then
  echo "SKIP: set BRIDGEVM_HVF_ALLOW_LIVE_GIC_IRQ=1 to run the live GIC interrupt-delivery proof"
  exit 0
fi
if [[ "$(sysctl -n kern.hv_support 2>/dev/null || echo 0)" != "1" ]]; then
  echo "SKIP: kern.hv_support != 1 (no Hypervisor.framework on this host)"
  exit 0
fi

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

cargo build -q -p bridgevm-hvf --example hvf_gic_irq_live
BIN="target/debug/examples/hvf_gic_irq_live"

ENTDIR="$(mktemp -d "/tmp/bridgevm-hvf-live-gic-irq.XXXXXX")"
ENT="$ENTDIR/hv.entitlements"
cat > "$ENT" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>com.apple.security.hypervisor</key><true/></dict></plist>
PLIST
codesign --sign - --entitlements "$ENT" --force "$BIN"

OUT="$("$BIN")"
echo "$OUT"
echo "$OUT" | grep -q "LIVE PROOF: hv_gic delivers" \
  || { echo "FAIL: timer PPI not delivered to the guest IRQ handler"; exit 1; }
echo "PASS: hv_gic delivers the architected-timer PPI to an EL1 guest IRQ handler"
