#!/usr/bin/env bash
# Live proof that the PL011 UART model captures real guest serial output through
# VirtPlatform on Hypervisor.framework: a guest writes "HI\n" to UARTDR and the
# host captures it.
#
# Opt-in (needs Apple Silicon + Hypervisor.framework + ad-hoc entitlement signing):
#   BRIDGEVM_HVF_ALLOW_LIVE_UART=1 tests/integration/hvf-uart-live-opt-in-smoke.sh
set -euo pipefail

if [[ "${BRIDGEVM_HVF_ALLOW_LIVE_UART:-0}" != "1" ]]; then
  echo "SKIP: set BRIDGEVM_HVF_ALLOW_LIVE_UART=1 to run the live UART proof"
  exit 0
fi
if [[ "$(sysctl -n kern.hv_support 2>/dev/null || echo 0)" != "1" ]]; then
  echo "SKIP: kern.hv_support != 1 (no Hypervisor.framework on this host)"
  exit 0
fi

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

cargo build -q -p bridgevm-hvf --example hvf_uart_live
BIN="target/debug/examples/hvf_uart_live"

ENTDIR="$(mktemp -d "/tmp/bridgevm-hvf-live-uart.XXXXXX")"
ENT="$ENTDIR/hv.entitlements"
cat > "$ENT" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>com.apple.security.hypervisor</key><true/></dict></plist>
PLIST

codesign --sign - --entitlements "$ENT" --force "$BIN"

OUT="$("$BIN")"
echo "$OUT"
echo "$OUT" | grep -q "LIVE PROOF: real guest UART" \
  || { echo "FAIL: live UART proof line missing"; exit 1; }
echo "PASS: live guest serial captured through VirtPlatform PL011 on real Hypervisor.framework"
