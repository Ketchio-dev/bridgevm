#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
. "$ROOT/packaging/macos/app-name.sh"
MACOS_DIR="$ROOT/apps/macos"
OUT_DIR="${BRIDGEVM_MACOS_BUNDLE_DIR:-$ROOT/target/macos}"
APP_NAME="${BRIDGEVM_MACOS_APP_NAME:-BridgeVMApp}"
bridgevm_validate_macos_app_name "$APP_NAME" BRIDGEVM_MACOS_APP_NAME || exit 2
APP="$OUT_DIR/$APP_NAME.app"
CONTENTS="$APP/Contents"
MACOS="$CONTENTS/MacOS"
RESOURCES="$CONTENTS/Resources"
HELPERS="$CONTENTS/Helpers"
APPLICATIONS="$CONTENTS/Applications"
HVF_LAB_APP="$APPLICATIONS/BridgeVMControl.app"
HVF_LAB_CONTENTS="$HVF_LAB_APP/Contents"
HVF_LAB_MACOS="$HVF_LAB_CONTENTS/MacOS"
HVF_LAB_RESOURCES="$HVF_LAB_CONTENTS/Resources"
HVF_LAB_FRAMEWORKS="$HVF_LAB_CONTENTS/Frameworks"
HVF_LAB_FIRMWARE="$HVF_LAB_RESOURCES/firmware/edk2-aarch64-secure-code.fd"
HVF_LAB_FIRMWARE_MANIFEST="$HVF_LAB_RESOURCES/firmware/manifest.txt"
HVF_WINDOWS_PROBE="$HVF_LAB_RESOURCES/target/release/examples/hvf_gic_boot_probe"
IDENTITY="${BRIDGEVM_CODESIGN_IDENTITY:--}"
BUILD_CONFIGURATION="${BRIDGEVM_MACOS_BUILD_CONFIGURATION:-debug}"
BUNDLE_DISPLAY_NAME="${BRIDGEVM_BUNDLE_DISPLAY_NAME:-BridgeVM}"
BUNDLE_NAME="${BRIDGEVM_BUNDLE_NAME:-BridgeVM}"
BUNDLE_IDENTIFIER="${BRIDGEVM_BUNDLE_IDENTIFIER:-dev.bridgevm.app.debug}"
BUNDLE_SHORT_VERSION="${BRIDGEVM_BUNDLE_SHORT_VERSION:-0.1.0}"
BUNDLE_VERSION="${BRIDGEVM_BUNDLE_VERSION:-1}"
BUNDLE_COPYRIGHT="${BRIDGEVM_BUNDLE_COPYRIGHT:-}"
ICON_FILE="${BRIDGEVM_MACOS_ICON_FILE:-}"
SKIP_APPLE_VZ_RUNNER="${BRIDGEVM_MACOS_SKIP_APPLE_VZ_RUNNER:-0}"
FIRMWARE_CODE="${BRIDGEVM_HVF_FIRMWARE_CODE:-$ROOT/crates/bridgevm-hvf/firmware/edk2-aarch64-secure-code.fd}"
FIRMWARE_LICENSES="${BRIDGEVM_HVF_FIRMWARE_LICENSES:-$(dirname "$FIRMWARE_CODE")/edk2-licenses.txt}"
SECURE_FIRMWARE_SHA256="b1dc201b1382476ca8c8dcbf8c09abc7ae7429c8437e35bffd54bb9b228b750b"
if [[ -z "$ICON_FILE" && -f "$ROOT/packaging/macos/BridgeVM.icns" ]]; then
  ICON_FILE="$ROOT/packaging/macos/BridgeVM.icns"
fi

usage() {
  cat >&2 <<'EOF'
usage: packaging/macos/build-debug-app-bundle.sh [--verify-only PATH]

Builds the SwiftPM BridgeVMApp executable and wraps it in a local debug
BridgeVMApp.app bundle under target/macos by default.

Environment:
  BRIDGEVM_MACOS_BUNDLE_DIR    output directory, defaults to target/macos
  BRIDGEVM_MACOS_APP_NAME      .app bundle basename, defaults to BridgeVMApp
  BRIDGEVM_CODESIGN_IDENTITY   codesign identity, defaults to ad-hoc '-'
  BRIDGEVM_MACOS_BUILD_CONFIGURATION
                               debug or release, defaults to debug
  BRIDGEVM_BUNDLE_DISPLAY_NAME CFBundleDisplayName, defaults to BridgeVM
  BRIDGEVM_BUNDLE_NAME         CFBundleName, defaults to BridgeVM
  BRIDGEVM_BUNDLE_IDENTIFIER   CFBundleIdentifier, defaults to dev.bridgevm.app.debug
  BRIDGEVM_BUNDLE_SHORT_VERSION
                               CFBundleShortVersionString, defaults to 0.1.0
  BRIDGEVM_BUNDLE_VERSION      CFBundleVersion, defaults to 1
  BRIDGEVM_BUNDLE_COPYRIGHT    optional NSHumanReadableCopyright value
  BRIDGEVM_MACOS_ICON_FILE     optional .icns file copied into Resources and
                               recorded as CFBundleIconFile
  BRIDGEVM_MACOS_SKIP_APPLE_VZ_RUNNER
                               set to 1 to omit the signed AppleVzRunner helper
                               from the local debug bundle
  BRIDGEVM_HVF_FIRMWARE_CODE    pinned 3 MiB Secure Boot + TPM2 AArch64 EDK2
                               code image embedded in the nested Windows HVF app
  BRIDGEVM_HVF_FIRMWARE_LICENSES
                               license notices corresponding to that image

Bundled Windows HVF runtime:
  Every bundle includes BridgeVMControl.app under Contents/Applications, the
  installed-Windows wrapper scripts, and a release hvf_gic_boot_probe signed
  with com.apple.security.hypervisor. The nested app never invokes Cargo.

Signing note:
  This debug helper only runs codesign. It does not notarize, staple, enable a
  hardened runtime, or otherwise prepare a public release artifact.
EOF
}

VERIFY_ONLY=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --verify-only)
      [[ $# -ge 2 ]] || {
        usage
        exit 2
      }
      VERIFY_ONLY="$2"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      usage
      exit 2
      ;;
  esac
done

verify_bundle() {
  local app="$1"
  local executable="$app/Contents/MacOS/BridgeVMApp"
  local plist="$app/Contents/Info.plist"

  [[ -d "$app" ]] || {
    echo "BridgeVMApp bundle is missing: $app" >&2
    exit 1
  }
  [[ -x "$executable" ]] || {
    echo "BridgeVMApp executable is missing or not executable: $executable" >&2
    exit 1
  }
  [[ -f "$plist" ]] || {
    echo "BridgeVMApp Info.plist is missing: $plist" >&2
    exit 1
  }

  local require_plist_value
  require_plist_value() {
    local key="$1"
    local value
    value="$(/usr/libexec/PlistBuddy -c "Print :$key" "$plist" 2>/dev/null || true)"
    [[ -n "$value" ]] || {
      echo "BridgeVMApp Info.plist is missing $key" >&2
      exit 1
    }
    printf '%s' "$value"
  }

  local bundle_executable
  bundle_executable="$(require_plist_value CFBundleExecutable)"
  [[ "$bundle_executable" == "BridgeVMApp" ]] || {
    echo "BridgeVMApp Info.plist has the wrong CFBundleExecutable" >&2
    exit 1
  }
  local bundle_package_type
  bundle_package_type="$(require_plist_value CFBundlePackageType)"
  [[ "$bundle_package_type" == "APPL" ]] || {
    echo "BridgeVMApp Info.plist has the wrong CFBundlePackageType" >&2
    exit 1
  }
  local bundle_identifier
  bundle_identifier="$(require_plist_value CFBundleIdentifier)"
  [[ "$bundle_identifier" =~ ^[A-Za-z0-9][A-Za-z0-9-]*(\.[A-Za-z0-9][A-Za-z0-9-]*)+$ ]] || {
    echo "BridgeVMApp Info.plist has an invalid CFBundleIdentifier: $bundle_identifier" >&2
    exit 1
  }
  require_plist_value CFBundleShortVersionString >/dev/null
  require_plist_value CFBundleVersion >/dev/null
  local minimum_system_version
  minimum_system_version="$(require_plist_value LSMinimumSystemVersion)"
  [[ "$minimum_system_version" == "14.0" ]] || {
    echo "BridgeVMApp Info.plist has the wrong LSMinimumSystemVersion" >&2
    exit 1
  }
  local icon_file
  icon_file="$(/usr/libexec/PlistBuddy -c "Print :CFBundleIconFile" "$plist" 2>/dev/null || true)"
  if [[ -n "$icon_file" && ! -f "$app/Contents/Resources/$icon_file" ]]; then
    echo "BridgeVMApp Info.plist references a missing icon resource: $icon_file" >&2
    exit 1
  fi
  local apple_vz_runner="$app/Contents/Helpers/AppleVzRunner"
  if [[ -e "$apple_vz_runner" ]]; then
    "$ROOT/apps/macos/scripts/build-sign-apple-vz-runner.sh" \
      --verify-only "$apple_vz_runner" >/dev/null
  fi
  local hvf_lab="$app/Contents/Applications/BridgeVMControl.app"
  local hvf_lab_executable="$hvf_lab/Contents/MacOS/BridgeVMControl"
  local hvf_lab_plist="$hvf_lab/Contents/Info.plist"
  local hvf_lab_resources="$hvf_lab/Contents/Resources"
  local hvf_probe="$hvf_lab_resources/target/release/examples/hvf_gic_boot_probe"
  local hvf_firmware="$hvf_lab_resources/firmware/edk2-aarch64-secure-code.fd"
  local hvf_firmware_manifest="$hvf_lab_resources/firmware/manifest.txt"
  local hvf_firmware_licenses="$hvf_lab_resources/firmware/licenses.txt"
  local hvf_firmware_build_receipt="$hvf_lab_resources/firmware/edk2-aarch64-secure-code.fd.build.json"
  local hvf_resource_bundle="$hvf_lab_resources/BridgeVMApp_BridgeVMControl.bundle"
  [[ -d "$hvf_lab" ]] || {
    echo "BridgeVM Windows HVF Lab bundle is missing: $hvf_lab" >&2
    exit 1
  }
  [[ -x "$hvf_lab_executable" ]] || {
    echo "BridgeVM Windows HVF Lab executable is missing: $hvf_lab_executable" >&2
    exit 1
  }
  [[ -f "$hvf_lab_plist" ]] || {
    echo "BridgeVM Windows HVF Lab Info.plist is missing: $hvf_lab_plist" >&2
    exit 1
  }
  local hvf_lab_bundle_executable
  hvf_lab_bundle_executable="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$hvf_lab_plist" 2>/dev/null || true)"
  [[ "$hvf_lab_bundle_executable" == "BridgeVMControl" ]] || {
    echo "BridgeVM Windows HVF Lab Info.plist has the wrong CFBundleExecutable" >&2
    exit 1
  }
  local hvf_lab_bundle_identifier
  hvf_lab_bundle_identifier="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$hvf_lab_plist" 2>/dev/null || true)"
  [[ "$hvf_lab_bundle_identifier" == "$bundle_identifier.hvf-lab" ]] || {
    echo "BridgeVM Windows HVF Lab Info.plist has the wrong CFBundleIdentifier" >&2
    exit 1
  }
  local hvf_script
  for hvf_script in \
    run-hvf-windows-installed-boot.sh \
    run-hvf-windows-installed-boot-usage.sh \
    run-hvf-windows-installed-boot-validation.sh \
    run-hvf-windows-installed-boot-args.sh \
    run-hvf-windows-installed-boot-runner.sh; do
    [[ -x "$hvf_lab_resources/scripts/$hvf_script" && ! -L "$hvf_lab_resources/scripts/$hvf_script" ]] || {
      echo "BridgeVM Windows HVF runtime script is missing, non-executable, or a symlink: $hvf_script" >&2
      exit 1
    }
  done
  "$ROOT/apps/macos/scripts/build-sign-hvf-windows-probe.sh" \
    --verify-only "$hvf_probe" >/dev/null
  "$ROOT/apps/macos/scripts/bundle-swtpm-runtime.sh" \
    --verify-only "$hvf_lab" >/dev/null
  [[ -f "$hvf_firmware" && "$(stat -f '%z' "$hvf_firmware")" == "3145728" ]] || {
    echo "BridgeVM Windows HVF firmware is missing or not 3 MiB: $hvf_firmware" >&2
    exit 1
  }
  local expected_firmware_sha actual_firmware_sha
  expected_firmware_sha="$(awk -F= '$1 == "sha256" { print $2; exit }' "$hvf_firmware_manifest" 2>/dev/null || true)"
  actual_firmware_sha="$(shasum -a 256 "$hvf_firmware" | awk '{ print $1 }')"
  [[ "$expected_firmware_sha" =~ ^[0-9a-f]{64}$ && "$actual_firmware_sha" == "$expected_firmware_sha" ]] || {
    echo "BridgeVM Windows HVF firmware manifest is missing or does not match" >&2
    exit 1
  }
  [[ "$actual_firmware_sha" == "$SECURE_FIRMWARE_SHA256" ]] || {
    echo "BridgeVM Windows HVF firmware is not the pinned Secure Boot + TPM2 build" >&2
    exit 1
  }
  [[ -s "$hvf_firmware_build_receipt" ]] || {
    echo "BridgeVM Windows HVF firmware build receipt is missing" >&2
    exit 1
  }
  [[ -f "$hvf_resource_bundle/secureboot-microsoft-only-aarch64-v1.6.5.json" ]] || {
    echo "BridgeVM Windows HVF Secure Boot policy resource is missing" >&2
    exit 1
  }
  [[ -s "$hvf_firmware_licenses" ]] || {
    echo "BridgeVM Windows HVF firmware license notices are missing" >&2
    exit 1
  }
  codesign --verify --strict "$hvf_lab" >/dev/null 2>&1 || {
    echo "BridgeVM Windows HVF Lab signature verification failed: $hvf_lab" >&2
    exit 1
  }
  local helper
  for helper in bridgevmd lightvm-runner; do
    local helper_path="$app/Contents/Helpers/$helper"
    [[ -x "$helper_path" ]] || {
      echo "BridgeVM helper is missing or not executable: $helper_path" >&2
      exit 1
    }
    codesign --verify --strict "$helper_path" >/dev/null 2>&1 || {
      echo "BridgeVM helper signature verification failed: $helper_path" >&2
      exit 1
    }
  done
  codesign --verify --deep --strict "$app" >/dev/null 2>&1 || {
    echo "BridgeVMApp bundle signature verification failed: $app" >&2
    exit 1
  }
}

if [[ -n "$VERIFY_ONLY" ]]; then
  verify_bundle "$VERIFY_ONLY"
  printf '%s\n' "$VERIFY_ONLY"
  exit 0
fi

case "$BUILD_CONFIGURATION" in
  debug|release) ;;
  *)
    echo "BRIDGEVM_MACOS_BUILD_CONFIGURATION must be debug or release, got: $BUILD_CONFIGURATION" >&2
    exit 2
    ;;
esac

[[ -f "$FIRMWARE_CODE" ]] || {
  echo "BridgeVM Windows HVF firmware source is missing: $FIRMWARE_CODE" >&2
  exit 1
}
[[ "$(stat -f '%z' "$FIRMWARE_CODE")" == "3145728" ]] || {
  echo "BridgeVM Windows HVF firmware source must be exactly 3 MiB: $FIRMWARE_CODE" >&2
  exit 1
}
[[ "$(shasum -a 256 "$FIRMWARE_CODE" | awk '{ print $1 }')" == "$SECURE_FIRMWARE_SHA256" ]] || {
  echo "BridgeVM Windows HVF firmware source does not match the pinned Secure Boot + TPM2 build" >&2
  exit 1
}
FIRMWARE_BUILD_RECEIPT="$FIRMWARE_CODE.build.json"
[[ -s "$FIRMWARE_BUILD_RECEIPT" ]] || {
  echo "BridgeVM Windows HVF firmware build receipt is missing: $FIRMWARE_BUILD_RECEIPT" >&2
  exit 1
}
[[ -s "$FIRMWARE_LICENSES" ]] || {
  echo "BridgeVM Windows HVF firmware license notices are missing: $FIRMWARE_LICENSES" >&2
  exit 1
}

swift build --disable-sandbox --package-path "$MACOS_DIR" --configuration "$BUILD_CONFIGURATION" --quiet --product BridgeVMApp
swift build --disable-sandbox --package-path "$MACOS_DIR" --configuration "$BUILD_CONFIGURATION" --quiet --product BridgeVMControl
SWIFT_BIN_DIR="$(swift build --disable-sandbox --package-path "$MACOS_DIR" --configuration "$BUILD_CONFIGURATION" --show-bin-path)"
BIN="$SWIFT_BIN_DIR/BridgeVMApp"
HVF_LAB_BIN="$SWIFT_BIN_DIR/BridgeVMControl"
HVF_RESOURCE_BUNDLE="$SWIFT_BIN_DIR/BridgeVMApp_BridgeVMControl.bundle"
[[ -d "$HVF_RESOURCE_BUNDLE" ]] || {
  echo "BridgeVM Windows HVF Swift resource bundle is missing: $HVF_RESOURCE_BUNDLE" >&2
  exit 1
}
cargo_args=(build --quiet --bin bridgevmd --bin lightvm-runner)
cargo_profile_dir="debug"
if [[ "$BUILD_CONFIGURATION" == "release" ]]; then
  cargo_args+=(--release)
  cargo_profile_dir="release"
fi
cargo "${cargo_args[@]}"
BRIDGEVMD_BIN="$ROOT/target/$cargo_profile_dir/bridgevmd"
LIGHTVM_RUNNER_BIN="$ROOT/target/$cargo_profile_dir/lightvm-runner"

rm -rf "$APP"
install -d "$MACOS" "$RESOURCES" "$HELPERS" "$HVF_LAB_MACOS" "$HVF_LAB_RESOURCES/scripts"
install -m 755 "$BIN" "$MACOS/BridgeVMApp"
install -m 755 "$HVF_LAB_BIN" "$HVF_LAB_MACOS/BridgeVMControl"
install -m 755 "$BRIDGEVMD_BIN" "$HELPERS/bridgevmd"
install -m 755 "$LIGHTVM_RUNNER_BIN" "$HELPERS/lightvm-runner"
install -d "$(dirname "$HVF_LAB_FIRMWARE")"
install -m 644 "$FIRMWARE_CODE" "$HVF_LAB_FIRMWARE"
install -m 644 "$FIRMWARE_LICENSES" "$(dirname "$HVF_LAB_FIRMWARE")/licenses.txt"
install -m 644 "$FIRMWARE_BUILD_RECEIPT" "$HVF_LAB_FIRMWARE.build.json"
ditto "$HVF_RESOURCE_BUNDLE" "$HVF_LAB_RESOURCES/$(basename "$HVF_RESOURCE_BUNDLE")"
FIRMWARE_SHA256="$(shasum -a 256 "$HVF_LAB_FIRMWARE" | awk '{ print $1 }')"
printf '%s\n' \
  'BridgeVM bundled AArch64 UEFI firmware' \
  'component=TianoCore EDK II AARCH64 Secure Boot + TPM2 firmware' \
  'upstream=https://github.com/tianocore/edk2' \
  'commit=b03a21a63e3bd001f52c527e5a57feddb53a690b' \
  'defines=SECURE_BOOT_ENABLE=TRUE TPM2_ENABLE=TRUE TPM2_CONFIG_ENABLE=TRUE' \
  'license_notices=licenses.txt' \
  'build_receipt=edk2-aarch64-secure-code.fd.build.json' \
  'bytes=3145728' \
  "sha256=$FIRMWARE_SHA256" > "$HVF_LAB_FIRMWARE_MANIFEST"
codesign --force --sign "$IDENTITY" "$HELPERS/bridgevmd" >/dev/null
codesign --force --sign "$IDENTITY" "$HELPERS/lightvm-runner" >/dev/null

cat >"$CONTENTS/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string></string>
  <key>CFBundleExecutable</key>
  <string>BridgeVMApp</string>
  <key>CFBundleIdentifier</key>
  <string></string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string></string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string></string>
  <key>CFBundleVersion</key>
  <string></string>
  <key>LSMinimumSystemVersion</key>
  <string>14.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
PLIST

/usr/libexec/PlistBuddy -c "Set :CFBundleDisplayName $BUNDLE_DISPLAY_NAME" "$CONTENTS/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier $BUNDLE_IDENTIFIER" "$CONTENTS/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleName $BUNDLE_NAME" "$CONTENTS/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $BUNDLE_SHORT_VERSION" "$CONTENTS/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $BUNDLE_VERSION" "$CONTENTS/Info.plist"
if [[ -n "$BUNDLE_COPYRIGHT" ]]; then
  /usr/libexec/PlistBuddy -c "Add :NSHumanReadableCopyright string $BUNDLE_COPYRIGHT" "$CONTENTS/Info.plist"
fi

cat >"$HVF_LAB_CONTENTS/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string>BridgeVM Windows HVF Lab</string>
  <key>CFBundleExecutable</key>
  <string>BridgeVMControl</string>
  <key>CFBundleIdentifier</key>
  <string></string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>BridgeVMControl</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string></string>
  <key>CFBundleVersion</key>
  <string></string>
  <key>LSMinimumSystemVersion</key>
  <string>14.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
PLIST
/usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier $BUNDLE_IDENTIFIER.hvf-lab" "$HVF_LAB_CONTENTS/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $BUNDLE_SHORT_VERSION" "$HVF_LAB_CONTENTS/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $BUNDLE_VERSION" "$HVF_LAB_CONTENTS/Info.plist"

for hvf_script in \
  run-hvf-windows-installed-boot.sh \
  run-hvf-windows-installed-boot-usage.sh \
  run-hvf-windows-installed-boot-validation.sh \
  run-hvf-windows-installed-boot-args.sh \
  run-hvf-windows-installed-boot-runner.sh; do
  install -m 755 "$ROOT/scripts/$hvf_script" "$HVF_LAB_RESOURCES/scripts/$hvf_script"
done

"$ROOT/apps/macos/scripts/build-sign-hvf-windows-probe.sh" \
  --release \
  --output "$HVF_WINDOWS_PROBE" \
  --bundle-frameworks "$HVF_LAB_FRAMEWORKS" >/dev/null
BRIDGEVM_CODESIGN_IDENTITY="$IDENTITY" \
  "$ROOT/apps/macos/scripts/bundle-swtpm-runtime.sh" \
  --app "$HVF_LAB_APP" >/dev/null
if [[ -n "$ICON_FILE" ]]; then
  [[ -f "$ICON_FILE" ]] || {
    echo "BridgeVM icon file is missing: $ICON_FILE" >&2
    exit 1
  }
  [[ "$ICON_FILE" == *.icns ]] || {
    echo "BridgeVM icon file must use the .icns extension: $ICON_FILE" >&2
    exit 1
  }
  [[ -s "$ICON_FILE" ]] || {
    echo "BridgeVM icon file is empty: $ICON_FILE" >&2
    exit 1
  }
  icon_name="$(basename "$ICON_FILE")"
  install -m 644 "$ICON_FILE" "$RESOURCES/$icon_name"
  /usr/libexec/PlistBuddy -c "Add :CFBundleIconFile string $icon_name" "$CONTENTS/Info.plist"
fi

if [[ "$SKIP_APPLE_VZ_RUNNER" != "1" ]]; then
  "$ROOT/apps/macos/scripts/build-sign-apple-vz-runner.sh" \
    --output "$HELPERS/AppleVzRunner" >/dev/null
fi

if [[ "$BUILD_CONFIGURATION" == "release" ]]; then
  codesign --force --options runtime --sign "$IDENTITY" "$HVF_LAB_APP" >/dev/null
  codesign --force --options runtime --sign "$IDENTITY" "$APP" >/dev/null
else
  codesign --force --sign "$IDENTITY" "$HVF_LAB_APP" >/dev/null
  codesign --force --sign "$IDENTITY" "$APP" >/dev/null
fi
verify_bundle "$APP"
printf '%s\n' "$APP"
