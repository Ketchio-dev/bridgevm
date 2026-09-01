#!/usr/bin/env bash
# Opt-in live proof for the independent BridgeVM Virtual ARM PC v1 GIC and RAM
# addresses. It boots only a bounded EL1 timer guest, not firmware or Windows.
set -euo pipefail

if [[ "${BRIDGEVM_HVF_ALLOW_LIVE_BRIDGEVM_PC_IRQ:-0}" != "1" ]]; then
  echo "SKIP: set BRIDGEVM_HVF_ALLOW_LIVE_BRIDGEVM_PC_IRQ=1 to run the BridgeVM PC IRQ proof"
  exit 0
fi
if [[ "$(sysctl -n kern.hv_support 2>/dev/null || echo 0)" != "1" ]]; then
  echo "SKIP: kern.hv_support != 1 (no Hypervisor.framework on this host)"
  exit 0
fi

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

cargo build -q -p bridgevm-hvf --example bridgevm_pc_irq_live
BIN="target/debug/examples/bridgevm_pc_irq_live"

ENTDIR="$(mktemp -d "/tmp/bridgevm-pc-gic-irq.XXXXXX")"
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
echo "$OUT" | grep -q "BridgeVM Virtual ARM PC IRQ probe: PASS"
echo "$OUT" | grep -q "gic_dist=0x20000000 gic_redist=0x21000000 gic_msi=0x23000000 ram=0x100000000"
echo "$OUT" | grep -q "flag=1 vtimer_exits=0"
echo "$OUT" | grep -q "LIVE PROOF: BridgeVM Virtual ARM PC v1 GIC delivers"
echo "PASS: BridgeVM Virtual ARM PC v1 delivers an architected-timer PPI"
