#!/usr/bin/env bash
# Install the packaged T17 UI driver app plus the product snapshot primitive.
set -euo pipefail
[[ $# == 3 ]] || { echo "usage: package-hvf-product-e2e.sh APP SWIFT_BIN_DIR IDENTITY" >&2; exit 2; }
APP="$1"; SWIFT_BIN_DIR="$2"; IDENTITY="$3"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
[[ "$APP" == /* && -d "$APP/Contents/MacOS" ]] || { echo "invalid staged app" >&2; exit 2; }
[[ "$SWIFT_BIN_DIR" == /* && -x "$SWIFT_BIN_DIR/BridgeVMProductE2E" ]] || { echo "missing packaged-product helper" >&2; exit 1; }
SNAPSHOT="$(python3 "$ROOT/apps/macos/scripts/cargo-built-artifact.py" --root "$ROOT" --target snapshot_pair_cli --kind example -- build --locked --release -p bridgevm-hvf --example snapshot_pair_cli)"
"$ROOT/apps/macos/scripts/package-product-e2e-helper-app.sh" "$APP" "$SWIFT_BIN_DIR" "$IDENTITY"
install -m 755 "$SNAPSHOT" "$APP/Contents/Resources/target/release/examples/snapshot_pair_cli"
if [[ "$IDENTITY" == - ]]; then codesign --force --sign - "$APP/Contents/Resources/target/release/examples/snapshot_pair_cli" >/dev/null
else codesign --force --sign "$IDENTITY" --options runtime --timestamp "$APP/Contents/Resources/target/release/examples/snapshot_pair_cli" >/dev/null
fi
