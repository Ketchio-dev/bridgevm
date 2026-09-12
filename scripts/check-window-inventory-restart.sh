#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TEMP="$(mktemp -d)"; trap 'rm -rf "$TEMP"' EXIT
PROTOCOL="$ROOT/apps/macos/Sources/BridgeVMWindowProtocol"
swiftc -emit-module -emit-library -module-name BridgeVMWindowProtocol \
  "$PROTOCOL/GuestWindowRecord.swift" "$PROTOCOL/HvfGuestWindowValidation.swift" \
  -emit-module-path "$TEMP/BridgeVMWindowProtocol.swiftmodule" -o "$TEMP/libBridgeVMWindowProtocol.dylib"
swiftc -parse-as-library -I "$TEMP" -L "$TEMP" -lBridgeVMWindowProtocol \
  -Xlinker -rpath -Xlinker "$TEMP" \
  "$ROOT/apps/macos/Sources/BridgeVMControl/HvfEngine/HvfWindowInventoryRequest.swift" \
  "$ROOT/tests/integration/window-inventory-restart.swift" -o "$TEMP/check"
"$TEMP/check"
