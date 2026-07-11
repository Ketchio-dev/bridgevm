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
HVF_LAB="$APP/Contents/Applications/BridgeVMControl.app"
HVF_PROBE="$HVF_LAB/Contents/Resources/target/release/examples/hvf_gic_boot_probe"
[[ -x "$HVF_LAB/Contents/MacOS/BridgeVMControl" ]] || fail "bundled Windows HVF Lab executable missing"
[[ -x "$HVF_LAB/Contents/Resources/scripts/run-hvf-windows-installed-boot.sh" ]] \
  || fail "bundled Windows HVF wrapper missing"
"$ROOT/apps/macos/scripts/build-sign-apple-vz-runner.sh" \
  --verify-only "$APP/Contents/Helpers/AppleVzRunner" >/dev/null
"$ROOT/apps/macos/scripts/build-sign-hvf-windows-probe.sh" \
  --verify-only "$HVF_PROBE" >/dev/null
"$BUILD_SCRIPT" --verify-only "$APP" >/dev/null

echo "PASS: macOS debug app clean build smoke"
