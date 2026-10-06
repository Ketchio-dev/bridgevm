#!/bin/bash
# Build a private development adapter; no guest or production app changes.
set -euo pipefail
umask 077
ROOT="$(cd "${BASH_SOURCE[0]%/*}/../.." && pwd -P)"
OUTPUT="${1:?exclusive absolute output directory}"
[[ "$OUTPUT" == /* && ! -e "$OUTPUT" && ! -L "$OUTPUT" ]] || exit 2
/bin/bash --noprofile --norc -p "$ROOT/scripts/live-gates/t22-pair-source-admission.sh" \
    "$ROOT" "$(/usr/bin/git -C "$ROOT" rev-parse HEAD)"
mkdir -m 700 "$OUTPUT"
RESOURCES="$OUTPUT/D11Resources.bundle/Contents/Resources"
mkdir -p "$RESOURCES"
printf '%s\n' '<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>org.bridgevm.d11.resources</string><key>CFBundlePackageType</key><string>BNDL</string></dict></plist>' \
    > "$OUTPUT/D11Resources.bundle/Contents/Info.plist"
for file in windows-boot-seed-vars.fd.gz secureboot-microsoft-windows-transition-aarch64-v1.6.5.json; do
    cp "$ROOT/apps/macos/Sources/BridgeVMControl/Resources/$file" "$RESOURCES/$file"
done
SCRATCH="$OUTPUT/compile-private"
mkdir "$SCRATCH"
xcrun swiftc -parse-as-library -swift-version 5 -sdk "$(xcrun --sdk macosx --show-sdk-path)" \
    -module-cache-path "$SCRATCH/module-cache" \
    "$ROOT/apps/macos/Sources/BridgeVMControl/BridgeVMControlResources.swift" \
    "$ROOT/apps/macos/Sources/BridgeVMControl/HvfEngine/HvfWindowsBootSeed.swift" \
    "$ROOT/apps/macos/Sources/BridgeVMControl/HvfEngine/HvfSecureBootProvisioner.swift" \
    "$ROOT/apps/macos/Sources/BridgeVMProductE2E/T17PrivateUnattend.swift" \
    "$ROOT/scripts/development/D11FixtureHelper.swift" -o "$OUTPUT/d11-fixture-helper"
rm -rf "$SCRATCH"
/usr/bin/git -C "$ROOT" rev-parse HEAD > "$OUTPUT/source-commit.txt"
chmod 500 "$OUTPUT/d11-fixture-helper"
chmod 400 "$OUTPUT/source-commit.txt" "$RESOURCES/"* "$OUTPUT/D11Resources.bundle/Contents/Info.plist"
