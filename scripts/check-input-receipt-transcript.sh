#!/usr/bin/env bash
set -euo pipefail
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
if ! cargo test -p bridgevm-hvf --example hvf_gic_boot_probe --locked \
    input_receipt_cross_language_transcript -- --nocapture --test-threads=1 > "$work/transcript" 2>&1; then
    cat "$work/transcript"
    exit 1
fi
sources=apps/macos/Sources/BridgeVMControl/HvfEngine
swiftc -parse-as-library \
    "$sources/HvfOrderedInputQueue.swift" "$sources/HvfGuestInputEncoding.swift" \
    "$sources/HvfInputReceiptEnvelope.swift" "$sources/HvfUnicodeInputRequest.swift" \
    tests/integration/input-receipt-transcript.swift -o "$work/receipt-contract"
"$work/receipt-contract" "$work/transcript"
