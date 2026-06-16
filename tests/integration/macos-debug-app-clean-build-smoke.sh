#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILD_SCRIPT="$ROOT/packaging/macos/build-debug-app-bundle.sh"
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-debug-app-clean-build.XXXXXX")"
OUT_DIR="$WORKDIR/out"
APP="$OUT_DIR/BridgeVMCleanBuild.app"

cleanup() {
  rm -rf "$WORKDIR"
}
trap cleanup EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

env \
  BRIDGEVM_MACOS_BUNDLE_DIR="$OUT_DIR" \
  BRIDGEVM_MACOS_APP_NAME="BridgeVMCleanBuild" \
  BRIDGEVM_CODESIGN_IDENTITY=- \
  "$BUILD_SCRIPT" >/dev/null

mkdir -p "$APP/Contents/Helpers" "$APP/Contents/Resources"
printf 'stale helper\n' >"$APP/Contents/Helpers/StaleHelper"
printf 'stale resource\n' >"$APP/Contents/Resources/StaleResource.txt"

env \
  BRIDGEVM_MACOS_BUNDLE_DIR="$OUT_DIR" \
  BRIDGEVM_MACOS_APP_NAME="BridgeVMCleanBuild" \
  BRIDGEVM_CODESIGN_IDENTITY=- \
  "$BUILD_SCRIPT" >/dev/null

[[ ! -e "$APP/Contents/Helpers/StaleHelper" ]] || fail "stale helper survived rebuild"
[[ ! -e "$APP/Contents/Resources/StaleResource.txt" ]] || fail "stale resource survived rebuild"
[[ -x "$APP/Contents/MacOS/BridgeVMApp" ]] || fail "rebuilt app executable missing"
"$ROOT/apps/macos/scripts/build-sign-apple-vz-runner.sh" \
  --verify-only "$APP/Contents/Helpers/AppleVzRunner" >/dev/null
"$BUILD_SCRIPT" --verify-only "$APP" >/dev/null

echo "PASS: macOS debug app clean build smoke"
