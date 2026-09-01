#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
MACOS_DIR="$ROOT/apps/macos"
IDENTITY="${BRIDGEVM_CODESIGN_IDENTITY:--}"
SHORT_VERSION="${BRIDGEVM_BUNDLE_SHORT_VERSION:-0.1.0}"
usage() {
  cat >&2 <<'EOF'
usage: apps/macos/scripts/package-hvf-control-app.sh --output APP [--firmware-code FD]

Builds a self-contained BridgeVMControl.app for the custom Windows HVF path.
The packager defaults to the checked-in pinned 3 MiB secure+TPM2 AArch64 UEFI
code volume and never downloads firmware. Existing output is never overwritten.

Environment:
  BRIDGEVM_CODESIGN_IDENTITY  signing identity, defaults to ad-hoc '-'
  BRIDGEVM_BUNDLE_SHORT_VERSION  CFBundleShortVersionString, defaults to 0.1.0
EOF
}
OUTPUT=""
FIRMWARE_CODE="$ROOT/crates/bridgevm-hvf/firmware/edk2-aarch64-secure-code.fd"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --output) [[ $# -ge 2 ]] || { usage; exit 2; }; OUTPUT="$2"; shift 2 ;;
    --firmware-code) [[ $# -ge 2 ]] || { usage; exit 2; }; FIRMWARE_CODE="$2"; shift 2 ;;
    --help|-h) usage; exit 0 ;;
    *) usage; exit 2 ;;
  esac
done
[[ -n "$OUTPUT" ]] || { usage; exit 2; }
[[ "$OUTPUT" == *.app ]] || { echo "output must end in .app: $OUTPUT" >&2; exit 2; }
[[ ! -e "$OUTPUT" ]] || { echo "refusing to overwrite existing output: $OUTPUT" >&2; exit 1; }
[[ "$SHORT_VERSION" =~ ^[0-9]+(\.[0-9]+){2}([.-][0-9A-Za-z]+)*$ ]] || { echo "invalid bundle short version: $SHORT_VERSION" >&2; exit 2; }
[[ -f "$FIRMWARE_CODE" ]] || { echo "firmware code image is missing: $FIRMWARE_CODE" >&2; exit 1; }
readonly SECURE_FIRMWARE_SHA256="b1dc201b1382476ca8c8dcbf8c09abc7ae7429c8437e35bffd54bb9b228b750b"
[[ "$(stat -f '%z' "$FIRMWARE_CODE")" == "3145728" ]] || {
  echo "secure firmware code volume must be exactly 3 MiB: $FIRMWARE_CODE" >&2
  exit 1
}
[[ "$(shasum -a 256 "$FIRMWARE_CODE" | awk '{print $1}')" == "$SECURE_FIRMWARE_SHA256" ]] || {
  echo "secure firmware digest does not match the product trust policy: $FIRMWARE_CODE" >&2
  exit 1
}
output_parent="$(cd "$(dirname "$OUTPUT")" && pwd)"
stage_root="$(mktemp -d "$output_parent/.bridgevm-package.XXXXXX")"
stage_app="$stage_root/$(basename "$OUTPUT")"
trap 'rm -rf "$stage_root"' EXIT

swift_cache="$stage_root/swift-module-cache"
swift_scratch="$stage_root/swift-build"
CLANG_MODULE_CACHE_PATH="$swift_cache" SWIFTPM_MODULECACHE_OVERRIDE="$swift_cache" \
  swift build --disable-sandbox --package-path "$MACOS_DIR" --scratch-path "$swift_scratch" \
  --configuration release --product BridgeVMControl
swift_bin_dir="$(
  CLANG_MODULE_CACHE_PATH="$swift_cache" SWIFTPM_MODULECACHE_OVERRIDE="$swift_cache" \
    swift build --disable-sandbox --package-path "$MACOS_DIR" --scratch-path "$swift_scratch" \
    --configuration release --show-bin-path
)"
cargo build --locked --release -p bridgevm-cli
cargo build --locked --release -p hvf-runner

install -d \
  "$stage_app/Contents/MacOS" \
  "$stage_app/Contents/Resources/scripts/win-assets" \
  "$stage_app/Contents/Resources/scripts/win-tests" \
  "$stage_app/Contents/Resources/firmware" \
  "$stage_app/Contents/Resources/licenses" \
  "$stage_app/Contents/Resources/target/release/examples" \
  "$stage_app/Contents/Frameworks"
install -m 644 "$ROOT/LICENSE" \
  "$stage_app/Contents/Resources/LICENSE"
install -m 644 "$ROOT/THIRD-PARTY-NOTICES.md" "$ROOT/THIRD-PARTY-PATCHES.tsv" \
  "$stage_app/Contents/Resources"
install -m 644 "$ROOT/docs/licenses/virglrenderer-MIT.txt" \
  "$stage_app/Contents/Resources/licenses/virglrenderer-MIT.txt"
install -m 644 "$ROOT/docs/licenses/libepoxy-MIT.txt" \
  "$stage_app/Contents/Resources/licenses/libepoxy-MIT.txt"
python3 "$ROOT/scripts/generate-rust-dependency-inventory.py" \
  --output "$stage_app/Contents/Resources/licenses/rust-dependencies.tsv" >/dev/null
python3 "$ROOT/scripts/generate-rust-license-bundle.py" \
  --output "$stage_app/Contents/Resources/licenses/rust-license-texts.txt" >/dev/null
install -m 644 "$MACOS_DIR/BridgeVMControl-Info.plist" "$stage_app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $SHORT_VERSION" "$stage_app/Contents/Info.plist"
[[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$stage_app/Contents/Info.plist")" == "$SHORT_VERSION" ]] || { echo "bundle short version did not persist" >&2; exit 1; }
install -m 755 "$swift_bin_dir/BridgeVMControl" "$stage_app/Contents/MacOS/BridgeVMControl"
install -m 755 "$ROOT/target/release/bridgevm" "$stage_app/Contents/Resources/target/release/bridgevm"
install -m 755 "$ROOT/target/release/hvf-runner" "$stage_app/Contents/Resources/target/release/hvf-runner"
install -m 644 "$FIRMWARE_CODE" "$stage_app/Contents/Resources/firmware/edk2-aarch64-secure-code.fd"
install -m 644 \
  "$ROOT/crates/bridgevm-hvf/firmware/edk2-aarch64-secure-code.fd.build.json" \
  "$stage_app/Contents/Resources/firmware/edk2-aarch64-secure-code.fd.build.json"
install -m 644 \
  "$ROOT/crates/bridgevm-hvf/firmware/edk2-licenses.txt" \
  "$stage_app/Contents/Resources/firmware/licenses.txt"
printf '%s\n' \
  'BridgeVM bundled AArch64 UEFI firmware' \
  'component=TianoCore EDK II AARCH64 Secure Boot + TPM2 firmware' \
  'upstream=https://github.com/tianocore/edk2' \
  'commit=b03a21a63e3bd001f52c527e5a57feddb53a690b' \
  'defines=SECURE_BOOT_ENABLE=TRUE TPM2_ENABLE=TRUE TPM2_CONFIG_ENABLE=TRUE' \
  'license_notices=licenses.txt' \
  'build_receipt=edk2-aarch64-secure-code.fd.build.json' \
  'bytes=3145728' \
  "sha256=$SECURE_FIRMWARE_SHA256" \
  > "$stage_app/Contents/Resources/firmware/manifest.txt"

for script in \
  run-hvf-windows-installed-boot.sh \
  run-hvf-windows-installed-boot-usage.sh \
  run-hvf-windows-installed-boot-validation.sh \
  run-hvf-windows-installed-boot-args.sh \
  run-hvf-windows-installed-boot-runner.sh \
  build-hvf-windows-scripted-source.sh \
  build-hvf-windows-viogpu3d-injector.sh \
  build-hvf-windows-driver-injector.sh \
  check-hvf-windows-viogpu3d-package.sh \
  run-hvf-windows-scripted-install.sh
do
  install -m 755 "$ROOT/scripts/$script" "$stage_app/Contents/Resources/scripts/$script"
done
for asset in \
  bv-ppsspp-title.json \
  winpeshl.ini \
  bvinstall.cmd \
  bvdiskpart.txt \
  unattend.xml \
  winpeshl-inject.ini \
  bvinject.cmd \
  bvgpu-firstboot.cmd \
  bvgpu-clean-driver-state.ps1 \
  bvgpu-diagnostics-run.cmd \
  bvgpu-diagnostics-service.c \
  bvgpu-real-title-gate.ps1 \
  bvgpu-title-gate.ps1 \
  bvgpu-diagnostics.ps1 \
  bvgpu-vulkan-probe.ps1 \
  bvgpu-diagnostics-startup.cmd \
  bvagent.ps1 \
  bvagent-install-service.c
do
  install -m 644 "$ROOT/scripts/win-assets/$asset" \
    "$stage_app/Contents/Resources/scripts/win-assets/$asset"
done
for source in bridgevm-vulkan-draw-smoke.c bridgevm-vulkan-draw-shaders.h; do
  install -m 644 "$ROOT/scripts/win-tests/$source" \
    "$stage_app/Contents/Resources/scripts/win-tests/$source"
done

resource_bundle="$swift_bin_dir/BridgeVMApp_BridgeVMControl.bundle"
[[ -d "$resource_bundle" ]] || {
  echo "SwiftPM resource bundle is missing: $resource_bundle" >&2
  exit 1
}
ditto "$resource_bundle" "$stage_app/Contents/Resources/$(basename "$resource_bundle")"

BRIDGEVM_CODESIGN_IDENTITY="$IDENTITY" \
  "$MACOS_DIR/scripts/build-sign-hvf-windows-probe.sh" \
  --release \
  --output "$stage_app/Contents/Resources/target/release/examples/hvf_gic_boot_probe" \
  --bundle-frameworks "$stage_app/Contents/Frameworks" >/dev/null

BRIDGEVM_CODESIGN_IDENTITY="$IDENTITY" \
  "$MACOS_DIR/scripts/bundle-swtpm-runtime.sh" \
  --app "$stage_app" >/dev/null

sign_artifact() {
  if [[ "$IDENTITY" == "-" ]]; then
    codesign --force --sign - "$1" >/dev/null
  else
    codesign --force --sign "$IDENTITY" --options runtime --timestamp "$1" >/dev/null
  fi
}
sign_artifact "$stage_app/Contents/Resources/target/release/bridgevm"
sign_artifact "$stage_app/Contents/MacOS/BridgeVMControl"
sign_artifact "$stage_app"
codesign --verify --deep --strict "$stage_app"
"$MACOS_DIR/scripts/bundle-swtpm-runtime.sh" --verify-only "$stage_app" >/dev/null
"$ROOT/scripts/verify-app-third-party-notices.sh" "$stage_app" >/dev/null
mv "$stage_app" "$OUTPUT"
printf '%s\n' "$OUTPUT"
