#!/usr/bin/env bash
# Install and sign the packaged T17 UI driver plus the product snapshot primitive.
set -euo pipefail
[[ $# == 3 ]] || { echo "usage: package-hvf-product-e2e.sh APP SWIFT_BIN_DIR IDENTITY" >&2; exit 2; }
APP="$1"; SWIFT_BIN_DIR="$2"; IDENTITY="$3"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
[[ "$APP" == /* && -d "$APP/Contents/MacOS" ]] || { echo "invalid staged app" >&2; exit 2; }
[[ "$SWIFT_BIN_DIR" == /* && -x "$SWIFT_BIN_DIR/BridgeVMProductE2E" ]] || { echo "missing packaged-product helper" >&2; exit 1; }
cargo build --manifest-path "$ROOT/Cargo.toml" --locked --release -p bridgevm-hvf --example snapshot_pair_cli
SNAPSHOT="$ROOT/target/release/examples/snapshot_pair_cli"
[[ -x "$SNAPSHOT" ]] || { echo "missing snapshot_pair_cli" >&2; exit 1; }
install -m 755 "$SWIFT_BIN_DIR/BridgeVMProductE2E" "$APP/Contents/MacOS/BridgeVMProductE2E"
install -m 755 "$SNAPSHOT" "$APP/Contents/Resources/target/release/examples/snapshot_pair_cli"
for binary in "$APP/Contents/MacOS/BridgeVMProductE2E" "$APP/Contents/Resources/target/release/examples/snapshot_pair_cli"; do
  if [[ "$IDENTITY" == - ]]; then codesign --force --sign - "$binary" >/dev/null
  else codesign --force --sign "$IDENTITY" --options runtime --timestamp "$binary" >/dev/null
  fi
done
