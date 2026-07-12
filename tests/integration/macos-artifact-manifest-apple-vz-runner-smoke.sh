#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$ROOT/packaging/macos/write-artifact-manifest.sh"
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-artifact-manifest-apple-vz-runner.XXXXXX")"
APP="$WORKDIR/BridgeVM.app"
DMG="$WORKDIR/BridgeVM.dmg"
MANIFEST="$WORKDIR/BridgeVM-artifacts.txt"
APP_ONLY_MANIFEST="$WORKDIR/BridgeVM-app-artifacts.txt"
RUNNER="$APP/Contents/Helpers/AppleVzRunner"
BRIDGEVMD="$APP/Contents/Helpers/bridgevmd"
LIGHTVM_RUNNER="$APP/Contents/Helpers/lightvm-runner"
HVF_LAB="$APP/Contents/Applications/BridgeVMControl.app"
HVF_LAB_EXECUTABLE="$HVF_LAB/Contents/MacOS/BridgeVMControl"
HVF_WINDOWS_PROBE="$HVF_LAB/Contents/Resources/target/release/examples/hvf_gic_boot_probe"
HVF_VIRGL_RENDERER="$HVF_LAB/Contents/Frameworks/libvirglrenderer.1.dylib"
HVF_LIBEPOXY="$HVF_LAB/Contents/Frameworks/libepoxy.0.dylib"
HVF_FIRMWARE="$HVF_LAB/Contents/Resources/firmware/edk2-aarch64-code.fd"
HVF_FIRMWARE_MANIFEST="$HVF_LAB/Contents/Resources/firmware/manifest.txt"
HVF_FIRMWARE_LICENSES="$HVF_LAB/Contents/Resources/firmware/licenses.txt"
APP_NOTARY_SUBMIT_JSON="$WORKDIR/app-notary-submit.json"
APP_NOTARY_LOG_JSON="$WORKDIR/app-notary-log.json"
DMG_NOTARY_SUBMIT_JSON="$WORKDIR/dmg-notary-submit.json"
DMG_NOTARY_LOG_JSON="$WORKDIR/dmg-notary-log.json"

cleanup() {
  rm -rf "$WORKDIR"
}
trap cleanup EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

assert_contains_file() {
  local file="$1"
  local needle="$2"
  local label="$3"
  grep -Fq "$needle" "$file" || fail "$label missing expected text: $needle"
}

assert_not_contains_file() {
  local file="$1"
  local needle="$2"
  local label="$3"
  if grep -Fq "$needle" "$file"; then
    fail "$label included unexpected text: $needle"
  fi
}

[[ -x "$SCRIPT" ]] || fail "missing executable manifest script: $SCRIPT"

mkdir -p \
  "$APP/Contents/MacOS" \
  "$APP/Contents/Helpers" \
  "$HVF_LAB/Contents/MacOS" \
  "$(dirname "$HVF_WINDOWS_PROBE")" \
  "$(dirname "$HVF_VIRGL_RENDERER")" \
  "$(dirname "$HVF_FIRMWARE")"
cat >"$APP/Contents/Info.plist" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key>
  <string>dev.bridgevm.artifact-manifest-smoke</string>
  <key>CFBundleName</key>
  <string>BridgeVM</string>
  <key>CFBundleDisplayName</key>
  <string>BridgeVM</string>
  <key>CFBundleShortVersionString</key>
  <string>0.0.0-smoke</string>
  <key>CFBundleVersion</key>
  <string>0</string>
  <key>LSMinimumSystemVersion</key>
  <string>14.0</string>
</dict>
</plist>
EOF
printf '#!/bin/sh\necho BridgeVMApp smoke\n' >"$APP/Contents/MacOS/BridgeVMApp"
printf '#!/bin/sh\necho AppleVzRunner smoke\n' >"$RUNNER"
printf '#!/bin/sh\necho bridgevmd smoke\n' >"$BRIDGEVMD"
printf '#!/bin/sh\necho lightvm-runner smoke\n' >"$LIGHTVM_RUNNER"
printf '#!/bin/sh\necho BridgeVMControl smoke\n' >"$HVF_LAB_EXECUTABLE"
printf '#!/bin/sh\necho hvf_gic_boot_probe smoke\n' >"$HVF_WINDOWS_PROBE"
printf 'virgl renderer smoke\n' >"$HVF_VIRGL_RENDERER"
printf 'libepoxy smoke\n' >"$HVF_LIBEPOXY"
printf 'firmware smoke\n' >"$HVF_FIRMWARE"
printf 'sha256=firmware-smoke\n' >"$HVF_FIRMWARE_MANIFEST"
printf 'firmware licenses smoke\n' >"$HVF_FIRMWARE_LICENSES"
chmod +x \
  "$APP/Contents/MacOS/BridgeVMApp" \
  "$RUNNER" \
  "$BRIDGEVMD" \
  "$LIGHTVM_RUNNER" \
  "$HVF_LAB_EXECUTABLE" \
  "$HVF_WINDOWS_PROBE"
printf 'not a real dmg, manifest command recording smoke\n' >"$DMG"
printf '{"id":"app-submit-smoke"}\n' >"$APP_NOTARY_SUBMIT_JSON"
printf '{"status":"Accepted","target":"app"}\n' >"$APP_NOTARY_LOG_JSON"
printf '{"id":"dmg-submit-smoke"}\n' >"$DMG_NOTARY_SUBMIT_JSON"
printf '{"status":"Accepted","target":"dmg"}\n' >"$DMG_NOTARY_LOG_JSON"

output="$(env \
  BRIDGEVM_MACOS_APP_NOTARY_SUBMIT_JSON="$APP_NOTARY_SUBMIT_JSON" \
  BRIDGEVM_MACOS_APP_NOTARY_LOG_JSON="$APP_NOTARY_LOG_JSON" \
  BRIDGEVM_MACOS_DMG_NOTARY_SUBMIT_JSON="$DMG_NOTARY_SUBMIT_JSON" \
  BRIDGEVM_MACOS_DMG_NOTARY_LOG_JSON="$DMG_NOTARY_LOG_JSON" \
  "$SCRIPT" "$APP" "$DMG" "$MANIFEST")"

[[ "$output" == "$MANIFEST" ]] || fail "manifest output expected '$MANIFEST'; got '$output'"
[[ -f "$MANIFEST" ]] || fail "manifest was not written: $MANIFEST"

assert_contains_file "$MANIFEST" "app_executable.path=$APP/Contents/MacOS/BridgeVMApp" "app executable metadata"
assert_contains_file "$MANIFEST" "app_executable.present=true" "app executable metadata"
assert_contains_file "$MANIFEST" "app_executable.executable=true" "app executable metadata"
assert_contains_file "$MANIFEST" "app_executable.sha256=" "app executable metadata"
assert_contains_file "$MANIFEST" "apple_vz_runner.path=$RUNNER" "AppleVzRunner metadata"
assert_contains_file "$MANIFEST" "apple_vz_runner.present=true" "AppleVzRunner metadata"
assert_contains_file "$MANIFEST" "apple_vz_runner.executable=true" "AppleVzRunner metadata"
assert_contains_file "$MANIFEST" "apple_vz_runner.size_bytes=" "AppleVzRunner metadata"
assert_contains_file "$MANIFEST" "apple_vz_runner.sha256=" "AppleVzRunner metadata"
assert_contains_file "$MANIFEST" "apple_vz_runner_codesign_verify.exit=" "AppleVzRunner signature recording"
assert_contains_file "$MANIFEST" "apple_vz_runner_codesign_details.exit=" "AppleVzRunner signature recording"
assert_contains_file "$MANIFEST" "apple_vz_runner_entitlements.exit=" "AppleVzRunner entitlements recording"
assert_contains_file "$MANIFEST" "bridgevmd.path=$BRIDGEVMD" "bridgevmd metadata"
assert_contains_file "$MANIFEST" "bridgevmd.present=true" "bridgevmd metadata"
assert_contains_file "$MANIFEST" "bridgevmd.executable=true" "bridgevmd metadata"
assert_contains_file "$MANIFEST" "bridgevmd.sha256=" "bridgevmd metadata"
assert_contains_file "$MANIFEST" "bridgevmd_codesign_verify.exit=" "bridgevmd signature recording"
assert_contains_file "$MANIFEST" "lightvm_runner.path=$LIGHTVM_RUNNER" "lightvm-runner metadata"
assert_contains_file "$MANIFEST" "lightvm_runner.present=true" "lightvm-runner metadata"
assert_contains_file "$MANIFEST" "lightvm_runner.executable=true" "lightvm-runner metadata"
assert_contains_file "$MANIFEST" "lightvm_runner.sha256=" "lightvm-runner metadata"
assert_contains_file "$MANIFEST" "lightvm_runner_codesign_verify.exit=" "lightvm-runner signature recording"
assert_contains_file "$MANIFEST" "windows_hvf_lab.path=$HVF_LAB" "Windows HVF Lab metadata"
assert_contains_file "$MANIFEST" "windows_hvf_lab.present=true" "Windows HVF Lab metadata"
assert_contains_file "$MANIFEST" "windows_hvf_lab_executable.path=$HVF_LAB_EXECUTABLE" "Windows HVF Lab executable metadata"
assert_contains_file "$MANIFEST" "windows_hvf_lab_executable.sha256=" "Windows HVF Lab executable metadata"
assert_contains_file "$MANIFEST" "windows_hvf_probe.path=$HVF_WINDOWS_PROBE" "Windows HVF probe metadata"
assert_contains_file "$MANIFEST" "windows_hvf_probe.sha256=" "Windows HVF probe metadata"
assert_contains_file "$MANIFEST" "windows_hvf_virgl_renderer.sha256=" "Windows HVF VirGL metadata"
assert_contains_file "$MANIFEST" "windows_hvf_libepoxy.sha256=" "Windows HVF libepoxy metadata"
assert_contains_file "$MANIFEST" "windows_hvf_firmware.sha256=" "Windows HVF firmware metadata"
assert_contains_file "$MANIFEST" "windows_hvf_firmware_manifest.sha256=" "Windows HVF firmware manifest metadata"
assert_contains_file "$MANIFEST" "windows_hvf_firmware_licenses.sha256=" "Windows HVF firmware licenses metadata"
assert_contains_file "$MANIFEST" "windows_hvf_lab_codesign_verify.exit=" "Windows HVF Lab signature recording"
assert_contains_file "$MANIFEST" "windows_hvf_probe_codesign_verify.exit=" "Windows HVF probe signature recording"
assert_contains_file "$MANIFEST" "windows_hvf_probe_entitlements.exit=" "Windows HVF probe entitlement recording"
assert_contains_file "$MANIFEST" "dmg_codesign_verify.exit=" "DMG signature recording"
assert_contains_file "$MANIFEST" "dmg_codesign_details.exit=" "DMG signature recording"

for sidecar_key in \
  app_notary_submit_json \
  app_notary_log_json \
  dmg_notary_submit_json \
  dmg_notary_log_json
do
  path_var="$(printf '%s' "$sidecar_key" | tr '[:lower:]' '[:upper:]')"
  path_value="${!path_var}"
  assert_contains_file "$MANIFEST" "$sidecar_key.path=$path_value" "$sidecar_key metadata"
  assert_contains_file "$MANIFEST" "$sidecar_key.present=true" "$sidecar_key metadata"
  assert_contains_file "$MANIFEST" "$sidecar_key.size_bytes=" "$sidecar_key metadata"
  assert_contains_file "$MANIFEST" "$sidecar_key.sha256=" "$sidecar_key metadata"
done

assert_contains_file "$MANIFEST" "app_notary_submit_json.id=app-submit-smoke" "app notary submit metadata"
assert_contains_file "$MANIFEST" "app_notary_log_json.status=Accepted" "app notary log metadata"
assert_contains_file "$MANIFEST" "dmg_notary_submit_json.id=dmg-submit-smoke" "DMG notary submit metadata"
assert_contains_file "$MANIFEST" "dmg_notary_log_json.status=Accepted" "DMG notary log metadata"

app_only_output="$(env \
  BRIDGEVM_MACOS_APP_NOTARY_SUBMIT_JSON="$APP_NOTARY_SUBMIT_JSON" \
  BRIDGEVM_MACOS_APP_NOTARY_LOG_JSON="$APP_NOTARY_LOG_JSON" \
  "$SCRIPT" --app-only "$APP" "$APP_ONLY_MANIFEST")"

[[ "$app_only_output" == "$APP_ONLY_MANIFEST" ]] \
  || fail "app-only manifest output expected '$APP_ONLY_MANIFEST'; got '$app_only_output'"
[[ -f "$APP_ONLY_MANIFEST" ]] || fail "app-only manifest was not written: $APP_ONLY_MANIFEST"

assert_contains_file "$APP_ONLY_MANIFEST" "mode=app-only" "app-only mode metadata"
assert_contains_file "$APP_ONLY_MANIFEST" "app.path=$APP" "app-only app metadata"
assert_contains_file "$APP_ONLY_MANIFEST" "apple_vz_runner.path=$RUNNER" "app-only AppleVzRunner metadata"
assert_contains_file "$APP_ONLY_MANIFEST" "bridgevmd.path=$BRIDGEVMD" "app-only bridgevmd metadata"
assert_contains_file "$APP_ONLY_MANIFEST" "lightvm_runner.path=$LIGHTVM_RUNNER" "app-only lightvm-runner metadata"
assert_contains_file "$APP_ONLY_MANIFEST" "windows_hvf_lab.path=$HVF_LAB" "app-only Windows HVF Lab metadata"
assert_contains_file "$APP_ONLY_MANIFEST" "windows_hvf_probe.path=$HVF_WINDOWS_PROBE" "app-only Windows HVF probe metadata"
assert_contains_file "$APP_ONLY_MANIFEST" "windows_hvf_virgl_renderer.path=$HVF_VIRGL_RENDERER" "app-only Windows HVF VirGL metadata"
assert_contains_file "$APP_ONLY_MANIFEST" "windows_hvf_firmware.path=$HVF_FIRMWARE" "app-only Windows HVF firmware metadata"
assert_contains_file "$APP_ONLY_MANIFEST" "app_codesign_verify.exit=" "app-only app signature recording"
assert_contains_file "$APP_ONLY_MANIFEST" "app_notary_submit_json.id=app-submit-smoke" "app-only app notary submit metadata"
assert_contains_file "$APP_ONLY_MANIFEST" "app_notary_log_json.status=Accepted" "app-only app notary log metadata"
assert_not_contains_file "$APP_ONLY_MANIFEST" "dmg.path=" "app-only DMG metadata"
assert_not_contains_file "$APP_ONLY_MANIFEST" "dmg_codesign_verify.exit=" "app-only DMG signature recording"
assert_not_contains_file "$APP_ONLY_MANIFEST" "dmg_hdiutil_verify.exit=" "app-only DMG verification recording"
assert_not_contains_file "$APP_ONLY_MANIFEST" "dmg_notary_submit_json" "app-only DMG notary metadata"

echo "PASS: macOS artifact manifest AppleVzRunner smoke"
