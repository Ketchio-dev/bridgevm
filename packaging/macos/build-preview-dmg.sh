#!/usr/bin/env bash
# Build an ad-hoc-signed DMG; "preview" is a path contract, not product state.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="${BRIDGEVM_MACOS_PREVIEW_DIR:-$ROOT/target/preview}"
APP_NAME="${BRIDGEVM_MACOS_APP_NAME:-BridgeVM}"
APP="$OUT_DIR/$APP_NAME.app"
DMG="${BRIDGEVM_MACOS_DMG:-$OUT_DIR/BridgeVM.dmg}"
VOLUME_NAME="${BRIDGEVM_MACOS_DMG_VOLUME:-BridgeVM}"
SHORT_VERSION="${BRIDGEVM_BUNDLE_SHORT_VERSION:-0.1.0}"
BUILD_VERSION="${BRIDGEVM_BUNDLE_VERSION:-1}"
BUNDLE_ID="${BRIDGEVM_BUNDLE_IDENTIFIER:-dev.ketchio.bridgevm.preview}"

usage() {
  cat >&2 <<'EOF'
usage: packaging/macos/build-preview-dmg.sh

Builds an ad-hoc-signed BridgeVM app and DMG for local testing. This path
deliberately does not require Developer ID credentials or Apple notarization.

Environment:
  BRIDGEVM_MACOS_PREVIEW_DIR       output directory, defaults to target/preview
  BRIDGEVM_MACOS_APP_NAME          app basename, defaults to BridgeVM
  BRIDGEVM_MACOS_DMG               DMG path, defaults to <preview-dir>/BridgeVM.dmg
  BRIDGEVM_MACOS_DMG_VOLUME        DMG volume name, defaults to BridgeVM
  BRIDGEVM_BUNDLE_IDENTIFIER       defaults to dev.ketchio.bridgevm.preview
  BRIDGEVM_BUNDLE_SHORT_VERSION    defaults to 0.1.0
  BRIDGEVM_BUNDLE_VERSION          defaults to 1
EOF
}

case "${1:-}" in
  "") ;;
  --help|-h) usage; exit 0 ;;
  *) usage; exit 2 ;;
esac

[[ "$APP_NAME" != */* && "$APP_NAME" != "." && "$APP_NAME" != ".." ]] || {
  echo "invalid preview app name: $APP_NAME" >&2
  exit 2
}
[[ "$DMG" == *.dmg ]] || {
  echo "preview output must end in .dmg: $DMG" >&2
  exit 2
}

install_rust_license_bundle() {
  local resources="$1"
  local licenses="$resources/licenses"
  install -d "$licenses"
  python3 "$ROOT/scripts/generate-rust-dependency-inventory.py" \
    --output "$licenses/rust-dependencies.tsv" >/dev/null
  python3 "$ROOT/scripts/generate-rust-license-bundle.py" \
    --output "$licenses/rust-license-texts.txt" >/dev/null
  "$ROOT/scripts/verify-rust-dependency-inventory.sh" \
    "$licenses/rust-dependencies.tsv" >/dev/null
  "$ROOT/scripts/verify-rust-license-bundle.sh" \
    "$licenses/rust-dependencies.tsv" \
    "$licenses/rust-license-texts.txt" >/dev/null
}

install_project_notices() {
  local resources="$1"
  install -m 644 "$ROOT/LICENSE" "$resources/LICENSE"
  install -m 644 "$ROOT/THIRD-PARTY-NOTICES.md" "$ROOT/THIRD-PARTY-PATCHES.tsv" "$resources"
  cmp -s "$ROOT/LICENSE" "$resources/LICENSE" || {
    echo "bundled project license differs from repository LICENSE: $resources" >&2
    exit 1
  }
  cmp -s "$ROOT/THIRD-PARTY-NOTICES.md" \
    "$resources/THIRD-PARTY-NOTICES.md" || {
    echo "bundled third-party notice differs from repository source: $resources" >&2
    exit 1
  }
  cmp -s "$ROOT/THIRD-PARTY-PATCHES.tsv" "$resources/THIRD-PARTY-PATCHES.tsv" ||
    { echo "bundled patch registry differs from repository source: $resources" >&2; exit 1; }
}
rm -rf "$APP"
rm -f "$DMG" "$DMG.sha256"
mkdir -p "$OUT_DIR" "$(dirname "$DMG")"

env \
  BRIDGEVM_MACOS_BUNDLE_DIR="$OUT_DIR" \
  BRIDGEVM_MACOS_APP_NAME="$APP_NAME" \
  BRIDGEVM_MACOS_BUILD_CONFIGURATION=release \
  BRIDGEVM_CODESIGN_IDENTITY=- \
  BRIDGEVM_BUNDLE_DISPLAY_NAME=BridgeVM \
  BRIDGEVM_BUNDLE_NAME=BridgeVM \
  BRIDGEVM_BUNDLE_IDENTIFIER="$BUNDLE_ID" \
  BRIDGEVM_BUNDLE_SHORT_VERSION="$SHORT_VERSION" \
  BRIDGEVM_BUNDLE_VERSION="$BUILD_VERSION" \
  BRIDGEVM_BUNDLE_COPYRIGHT="Copyright © 2026 Ketchio-dev" \
  "$ROOT/packaging/macos/build-debug-app-bundle.sh" >/dev/null

RESOURCES="$APP/Contents/Resources"
HVF_LAB="$APP/Contents/Applications/BridgeVMControl.app"
HVF_RESOURCES="$HVF_LAB/Contents/Resources"
HVF_LICENSES="$HVF_RESOURCES/licenses"

# The outer application contains BridgeVM executables and is independently
# distributable, so carry the project license and locked Rust dependency notices.
install_project_notices "$RESOURCES"
install_rust_license_bundle "$RESOURCES"

# The nested Windows HVF application carries its own renderer/swtpm runtime.
# Make that bundle independently notice-complete before sealing the parent app.
install_project_notices "$HVF_RESOURCES"
install_rust_license_bundle "$HVF_RESOURCES"
install -m 644 "$ROOT/docs/licenses/virglrenderer-MIT.txt" \
  "$HVF_LICENSES/virglrenderer-MIT.txt"
install -m 644 "$ROOT/docs/licenses/libepoxy-MIT.txt" \
  "$HVF_LICENSES/libepoxy-MIT.txt"

# Adding resources invalidates the affected bundle seals. Sign inside-out, then
# run the same third-party gate used by the dedicated HVF packager.
codesign --force --sign - "$HVF_LAB" >/dev/null
"$ROOT/scripts/verify-app-third-party-notices.sh" "$HVF_LAB" >/dev/null
codesign --force --sign - "$APP" >/dev/null
"$ROOT/packaging/macos/build-debug-app-bundle.sh" --verify-only "$APP" >/dev/null

STAGE="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-preview-dmg.XXXXXX")"
cleanup() {
  rm -rf "$STAGE"
}
trap cleanup EXIT

ditto "$APP" "$STAGE/$APP_NAME.app"
install -m 644 "$ROOT/LICENSE" "$ROOT/THIRD-PARTY-NOTICES.md" \
  "$ROOT/THIRD-PARTY-PATCHES.tsv" "$STAGE"
ln -s /Applications "$STAGE/Applications"

hdiutil create \
  -volname "$VOLUME_NAME" \
  -srcfolder "$STAGE" \
  -ov \
  -format UDZO \
  "$DMG" >/dev/null
hdiutil verify "$DMG" >/dev/null

env \
  BRIDGEVM_MACOS_APP="$APP" \
  BRIDGEVM_MACOS_DMG_VOLUME="$VOLUME_NAME" \
  "$ROOT/packaging/macos/build-debug-dmg.sh" --verify-only "$DMG" >/dev/null

SHA256="$(shasum -a 256 "$DMG" | awk '{ print $1 }')"
printf '%s  %s\n' "$SHA256" "$(basename "$DMG")" > "$DMG.sha256"
printf 'app=%s\n' "$APP"
printf 'dmg=%s\n' "$DMG"
printf 'sha256=%s\n' "$SHA256"
