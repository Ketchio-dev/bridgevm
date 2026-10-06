#!/bin/bash
# Compile the real development adapter; exercise only private synthetic XML.
set -euo pipefail
umask 077
ROOT="$(cd "${BASH_SOURCE[0]%/*}/../.." && pwd -P)"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-d11-helper-test.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
xcrun swiftc -parse-as-library -swift-version 5 -sdk "$(xcrun --sdk macosx --show-sdk-path)" \
    -module-cache-path "$WORK/module-cache" \
    "$ROOT/apps/macos/Sources/BridgeVMControl/BridgeVMControlResources.swift" \
    "$ROOT/apps/macos/Sources/BridgeVMControl/HvfEngine/HvfWindowsBootSeed.swift" \
    "$ROOT/apps/macos/Sources/BridgeVMControl/HvfEngine/HvfSecureBootProvisioner.swift" \
    "$ROOT/apps/macos/Sources/BridgeVMProductE2E/T17PrivateUnattend.swift" \
    "$ROOT/scripts/development/D11FixtureHelper.swift" -o "$WORK/helper"
python3 -B "$ROOT/tests/integration/d11-fixture-helper-contract.py" "$WORK/helper" "$WORK"
