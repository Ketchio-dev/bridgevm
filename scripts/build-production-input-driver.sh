#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 1 && "$1" = /* && ! -e "$1" ]] || { echo 'requires a fresh absolute output path' >&2; exit 2; }
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ENGINE="$ROOT/apps/macos/Sources/BridgeVMControl/HvfEngine"
sources=()
for name in HvfSessionInputDriver HvfSessionInputRouter HvfOrderedInputQueue \
  HvfRecoverableInputStream HvfNegotiatedInputStream HvfAcknowledgedInputStream \
  HvfInputCapabilitiesRequest HvfUnicodeInputRequest HvfGuestInputEncoding \
  HvfPointerInputEncoding HvfPointerScrollEncoding HvfPointerHeldState HvfInputReceiptEnvelope; do
  sources+=("$ENGINE/$name.swift")
done
swiftc -parse-as-library -O "${sources[@]}" "$ROOT/scripts/live-gates/production-input-driver.swift" -o "$1"
