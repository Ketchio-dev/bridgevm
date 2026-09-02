#!/usr/bin/env bash
# Package the T17 Accessibility driver as a real, identity-bearing nested app.
set -euo pipefail
[[ $# == 3 ]] || { echo "usage: package-product-e2e-helper-app.sh APP SWIFT_BIN_DIR IDENTITY" >&2; exit 2; }
APP="$1"; SWIFT_BIN_DIR="$2"; IDENTITY="$3"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
HELPER_APP="$APP/Contents/Helpers/BridgeVMProductE2E.app"
HELPER_BIN="$HELPER_APP/Contents/MacOS/BridgeVMProductE2E"
[[ "$APP" == /* && -d "$APP/Contents/MacOS" ]] || { echo "invalid staged app" >&2; exit 2; }
[[ "$SWIFT_BIN_DIR" == /* && -x "$SWIFT_BIN_DIR/BridgeVMProductE2E" ]] || { echo "missing product E2E executable" >&2; exit 1; }
[[ ! -e "$HELPER_APP" && ! -L "$HELPER_APP" ]] || { echo "refusing to replace product E2E helper app" >&2; exit 1; }
[[ ! -e "$APP/Contents/MacOS/BridgeVMProductE2E" ]] || { echo "legacy bare product E2E helper is forbidden" >&2; exit 1; }
install -d "$HELPER_APP/Contents/MacOS"
install -m 644 "$ROOT/apps/macos/BridgeVMProductE2E-Info.plist" "$HELPER_APP/Contents/Info.plist"
install -m 755 "$SWIFT_BIN_DIR/BridgeVMProductE2E" "$HELPER_BIN"
if [[ "$IDENTITY" == - ]]; then
  codesign --force --sign - "$HELPER_APP" >/dev/null
else
  codesign --force --sign "$IDENTITY" --options runtime --timestamp "$HELPER_APP" >/dev/null
fi
"$ROOT/scripts/verify-product-e2e-helper-app.sh" "$APP"
