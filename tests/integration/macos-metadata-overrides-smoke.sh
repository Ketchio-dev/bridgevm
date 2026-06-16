#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

BUILD_SCRIPT="$ROOT/packaging/macos/build-debug-app-bundle.sh"
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-macos-metadata-overrides.XXXXXX")"
OUT_DIR="$WORKDIR/out"
APP_NAME="BridgeVM Metadata Smoke"
BUNDLE_IDENTIFIER="dev.bridgevm.metadata-smoke"
SHORT_VERSION="9.8.7-smoke"
BUNDLE_VERSION="9876"
COPYRIGHT="Copyright 2026 BridgeVM Metadata Smoke"
ICON_FILE="$WORKDIR/BridgeVMMetadataSmoke.icns"

cleanup() {
  rm -rf "$WORKDIR"
}
trap cleanup EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

plist_value() {
  local key="$1"
  /usr/libexec/PlistBuddy -c "Print :$key" "$APP/Contents/Info.plist" 2>/dev/null || true
}

assert_plist_value() {
  local key="$1"
  local expected="$2"
  local actual
  actual="$(plist_value "$key")"
  [[ "$actual" == "$expected" ]] || {
    fail "Info.plist $key expected '$expected'; got '$actual'"
  }
}

[[ -x "$BUILD_SCRIPT" ]] || fail "missing build script: $BUILD_SCRIPT"
printf 'icns metadata smoke fixture\n' >"$ICON_FILE"

output="$(
  env \
    BRIDGEVM_MACOS_BUNDLE_DIR="$OUT_DIR" \
    BRIDGEVM_MACOS_APP_NAME="$APP_NAME" \
    BRIDGEVM_BUNDLE_DISPLAY_NAME="$APP_NAME" \
    BRIDGEVM_BUNDLE_NAME="$APP_NAME" \
    BRIDGEVM_BUNDLE_IDENTIFIER="$BUNDLE_IDENTIFIER" \
    BRIDGEVM_BUNDLE_SHORT_VERSION="$SHORT_VERSION" \
    BRIDGEVM_BUNDLE_VERSION="$BUNDLE_VERSION" \
    BRIDGEVM_BUNDLE_COPYRIGHT="$COPYRIGHT" \
    BRIDGEVM_MACOS_ICON_FILE="$ICON_FILE" \
    BRIDGEVM_CODESIGN_IDENTITY=- \
    "$BUILD_SCRIPT"
)"

APP="$OUT_DIR/$APP_NAME.app"

[[ "$output" == "$APP" ]] || fail "build output expected '$APP'; got '$output'"
[[ -d "$APP" ]] || fail "expected app bundle path does not exist: $APP"
[[ -x "$APP/Contents/MacOS/BridgeVMApp" ]] || {
  fail "BridgeVMApp executable missing from overridden bundle: $APP"
}
[[ -f "$APP/Contents/Info.plist" ]] || fail "Info.plist missing: $APP"

assert_plist_value CFBundleDisplayName "$APP_NAME"
assert_plist_value CFBundleName "$APP_NAME"
assert_plist_value CFBundleIdentifier "$BUNDLE_IDENTIFIER"
assert_plist_value CFBundleShortVersionString "$SHORT_VERSION"
assert_plist_value CFBundleVersion "$BUNDLE_VERSION"
assert_plist_value NSHumanReadableCopyright "$COPYRIGHT"
assert_plist_value CFBundleIconFile "$(basename "$ICON_FILE")"
[[ -f "$APP/Contents/Resources/$(basename "$ICON_FILE")" ]] || {
  fail "icon file was not copied into app resources"
}

"$BUILD_SCRIPT" --verify-only "$APP" >/dev/null

echo "PASS: macOS metadata override smoke ($APP)"
