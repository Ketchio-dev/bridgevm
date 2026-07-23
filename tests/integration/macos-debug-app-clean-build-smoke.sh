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
HVF_FRAMEWORKS="$HVF_LAB/Contents/Frameworks"
HVF_SWTPM="$HVF_LAB/Contents/Helpers/swtpm"
HVF_SWTPM_MANIFEST="$HVF_LAB/Contents/Resources/swtpm/manifest.txt"
HVF_SWTPM_LICENSES="$HVF_LAB/Contents/Resources/swtpm/licenses"
HVF_FIRMWARE="$HVF_LAB/Contents/Resources/firmware/edk2-aarch64-secure-code.fd"
HVF_FIRMWARE_MANIFEST="$HVF_LAB/Contents/Resources/firmware/manifest.txt"
HVF_FIRMWARE_LICENSES="$HVF_LAB/Contents/Resources/firmware/licenses.txt"
HVF_SECURE_BOOT_POLICY="$HVF_LAB/Contents/Resources/BridgeVMApp_BridgeVMControl.bundle/secureboot-microsoft-windows-transition-aarch64-v1.6.5.json"
HVF_PPSSPP_TITLE_MANIFEST="$HVF_LAB/Contents/Resources/scripts/win-assets/bv-ppsspp-title.json"
[[ -x "$HVF_LAB/Contents/MacOS/BridgeVMControl" ]] || fail "bundled Windows HVF Lab executable missing"
[[ -x "$HVF_LAB/Contents/Resources/scripts/run-hvf-windows-installed-boot.sh" ]] \
  || fail "bundled Windows HVF wrapper missing"
[[ -f "$HVF_FRAMEWORKS/libvirglrenderer.1.dylib" ]] \
  || fail "bundled Windows HVF VirGL renderer missing"
[[ -f "$HVF_FRAMEWORKS/libepoxy.0.dylib" ]] \
  || fail "bundled Windows HVF libepoxy dependency missing"
[[ -x "$HVF_SWTPM" ]] || fail "bundled Windows HVF swtpm helper missing"
[[ -f "$HVF_FRAMEWORKS/libtpms.0.dylib" ]] \
  || fail "bundled Windows HVF libtpms dependency missing"
[[ -s "$HVF_SWTPM_MANIFEST" ]] || fail "bundled Windows HVF swtpm manifest missing"
[[ -d "$HVF_SWTPM_LICENSES" ]] || fail "bundled Windows HVF swtpm licenses missing"
[[ -f "$HVF_FIRMWARE" && "$(stat -f '%z' "$HVF_FIRMWARE")" == "3145728" ]] \
  || fail "bundled Windows HVF firmware missing or wrong size"
[[ "$(shasum -a 256 "$HVF_FIRMWARE" | awk '{ print $1 }')" == "b1dc201b1382476ca8c8dcbf8c09abc7ae7429c8437e35bffd54bb9b228b750b" ]] \
  || fail "bundled Windows HVF firmware is not the pinned Secure Boot + TPM2 build"
grep -Fqx "sha256=$(shasum -a 256 "$HVF_FIRMWARE" | awk '{ print $1 }')" "$HVF_FIRMWARE_MANIFEST" \
  || fail "bundled Windows HVF firmware manifest mismatch"
[[ -s "$HVF_FIRMWARE_LICENSES" ]] || fail "bundled Windows HVF firmware license notices missing"
[[ -f "$HVF_SECURE_BOOT_POLICY" ]] || fail "bundled Microsoft Secure Boot policy missing"
[[ -f "$HVF_PPSSPP_TITLE_MANIFEST" ]] || fail "bundled PPSSPP title manifest missing"
grep -Fq '"minimum_runtime_seconds": 600' "$HVF_PPSSPP_TITLE_MANIFEST" \
  || fail "bundled PPSSPP title gate is not the 600-second release policy"
otool -L "$HVF_PROBE" "$HVF_FRAMEWORKS/libvirglrenderer.1.dylib" \
  | grep -E '/Users/|/opt/homebrew/' >/dev/null \
  && fail "bundled Windows HVF runtime retains a development-host dylib path"
"$ROOT/apps/macos/scripts/build-sign-apple-vz-runner.sh" \
  --verify-only "$APP/Contents/Helpers/AppleVzRunner" >/dev/null
"$ROOT/apps/macos/scripts/build-sign-hvf-windows-probe.sh" \
  --verify-only "$HVF_PROBE" >/dev/null
"$ROOT/apps/macos/scripts/bundle-swtpm-runtime.sh" \
  --verify-only "$HVF_LAB" >/dev/null
"$BUILD_SCRIPT" --verify-only "$APP" >/dev/null

echo "PASS: macOS debug app clean build smoke"
