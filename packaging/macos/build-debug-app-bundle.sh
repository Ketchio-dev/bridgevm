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

swift build --package-path "$MACOS_DIR" --configuration "$BUILD_CONFIGURATION" --quiet --product BridgeVMApp
BIN="$(swift build --package-path "$MACOS_DIR" --configuration "$BUILD_CONFIGURATION" --show-bin-path)/BridgeVMApp"
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
install -d "$MACOS" "$RESOURCES" "$HELPERS"
install -m 755 "$BIN" "$MACOS/BridgeVMApp"
install -m 755 "$BRIDGEVMD_BIN" "$HELPERS/bridgevmd"
install -m 755 "$LIGHTVM_RUNNER_BIN" "$HELPERS/lightvm-runner"
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

codesign --force --sign "$IDENTITY" "$APP" >/dev/null
verify_bundle "$APP"
printf '%s\n' "$APP"
