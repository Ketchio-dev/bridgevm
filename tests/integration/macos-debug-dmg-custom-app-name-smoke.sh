#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILD_APP_SCRIPT="$ROOT/packaging/macos/build-debug-app-bundle.sh"
BUILD_DMG_SCRIPT="$ROOT/packaging/macos/build-debug-dmg.sh"
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-debug-dmg-custom-app.XXXXXX")"
OUT_DIR="$WORKDIR/out"
DMG="$OUT_DIR/BridgeVMCustomName.dmg"
BAD_LINK_DMG="$OUT_DIR/BridgeVMBadApplicationsLink.dmg"
BAD_VOLUME_DMG="$OUT_DIR/BridgeVMBadVolume.dmg"
BAD_EXTRA_APP_DMG="$OUT_DIR/BridgeVMBadExtraApp.dmg"
CUSTOM_APP_NAME="BridgeVMDmgCustom"
CUSTOM_APP="$OUT_DIR/$CUSTOM_APP_NAME.app"
DEFAULT_APP="$OUT_DIR/BridgeVMApp.app"
DEFAULT_SENTINEL="$DEFAULT_APP/Contents/Resources/stale-default-sentinel"
BAD_LINK_STAGE="$WORKDIR/bad-link-stage"
BAD_VOLUME_STAGE="$WORKDIR/bad-volume-stage"
BAD_EXTRA_APP_STAGE="$WORKDIR/bad-extra-app-stage"

cleanup() {
  rm -rf "$WORKDIR"
}
trap cleanup EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

assert_dmg_detached() {
  local dmg="$1"
  local dmg_real
  dmg_real="$(cd "$(dirname "$dmg")" && pwd -P)/$(basename "$dmg")"

  if hdiutil info | grep -F "$dmg" >/dev/null || hdiutil info | grep -F "$dmg_real" >/dev/null; then
    fail "DMG remained attached after verification cleanup: $dmg"
  fi
}

is_mountpoint_mounted() {
  local mount_dir="$1"
  local mount_dir_real="${2:-$1}"

  mount | grep -F " on $mount_dir " >/dev/null 2>&1 \
    || mount | grep -F " on $mount_dir_real " >/dev/null 2>&1
}

hdiutil_detach_bounded() {
  local target="$1"
  local mode="${2:-}"
  local pid

  if [[ "$mode" == "-force" ]]; then
    hdiutil detach "$target" -force -quiet >/dev/null 2>&1 &
  else
    hdiutil detach "$target" -quiet >/dev/null 2>&1 &
  fi
  pid=$!
  for _ in {1..30}; do
    if ! kill -0 "$pid" 2>/dev/null; then
      wait "$pid" 2>/dev/null || true
      return 0
    fi
    sleep 0.1
  done
  disown "$pid" 2>/dev/null || true
  kill "$pid" 2>/dev/null || true
  sleep 0.1
  kill -9 "$pid" 2>/dev/null || true
  return 1
}

detach_mount() {
  local device="$1"
  local mount_dir="$2"
  local mount_dir_real="${3:-$2}"
  local target

  if [[ -n "$device" ]]; then
    hdiutil_detach_bounded "$device" || true
  fi
  for target in "$mount_dir_real" "$mount_dir"; do
    [[ -n "$target" ]] || continue
    hdiutil_detach_bounded "$target" || true
  done
  is_mountpoint_mounted "$mount_dir" "$mount_dir_real" || return 0
  if [[ -n "$device" ]]; then
    hdiutil_detach_bounded "$device" -force || true
  fi
  for target in "$mount_dir_real" "$mount_dir"; do
    [[ -n "$target" ]] || continue
    hdiutil_detach_bounded "$target" -force || true
  done
  is_mountpoint_mounted "$mount_dir" "$mount_dir_real" && return 1
  return 0
}

env \
  BRIDGEVM_MACOS_BUNDLE_DIR="$OUT_DIR" \
  BRIDGEVM_MACOS_APP_NAME="BridgeVMApp" \
  BRIDGEVM_CODESIGN_IDENTITY=- \
  "$BUILD_APP_SCRIPT" >/dev/null
[[ -d "$DEFAULT_APP" ]] || fail "failed to create stale default app fixture"

env \
  BRIDGEVM_MACOS_BUNDLE_DIR="$OUT_DIR" \
  BRIDGEVM_MACOS_APP_NAME="$CUSTOM_APP_NAME" \
  BRIDGEVM_CODESIGN_IDENTITY=- \
  "$BUILD_APP_SCRIPT" >/dev/null
[[ -d "$CUSTOM_APP" ]] || fail "failed to create prebuilt custom app fixture"

printf '%s\n' stale >"$DEFAULT_SENTINEL"

env \
  BRIDGEVM_MACOS_BUNDLE_DIR="$OUT_DIR" \
  BRIDGEVM_MACOS_APP="$CUSTOM_APP" \
  BRIDGEVM_MACOS_DMG="$DMG" \
  BRIDGEVM_CODESIGN_IDENTITY=- \
  "$BUILD_DMG_SCRIPT" >/dev/null

[[ -f "$DEFAULT_SENTINEL" ]] || fail "stale default app was rebuilt while packaging prebuilt custom app"
env \
  BRIDGEVM_MACOS_BUNDLE_DIR="$OUT_DIR" \
  BRIDGEVM_MACOS_APP="$CUSTOM_APP" \
  "$BUILD_DMG_SCRIPT" --verify-only "$DMG" >/dev/null

mkdir -p "$BAD_LINK_STAGE" "$BAD_VOLUME_STAGE" "$BAD_EXTRA_APP_STAGE"
ditto "$CUSTOM_APP" "$BAD_LINK_STAGE/$CUSTOM_APP_NAME.app"
ln -s /Applications "$BAD_VOLUME_STAGE/Applications"
ditto "$CUSTOM_APP" "$BAD_VOLUME_STAGE/$CUSTOM_APP_NAME.app"
ln -s /Applications "$BAD_EXTRA_APP_STAGE/Applications"
ditto "$CUSTOM_APP" "$BAD_EXTRA_APP_STAGE/$CUSTOM_APP_NAME.app"
ditto "$DEFAULT_APP" "$BAD_EXTRA_APP_STAGE/BridgeVMApp.app"
hdiutil create \
  -volname BridgeVM \
  -srcfolder "$BAD_LINK_STAGE" \
  -ov \
  -format UDZO \
  "$BAD_LINK_DMG" >/dev/null
hdiutil create \
  -volname WrongBridgeVM \
  -srcfolder "$BAD_VOLUME_STAGE" \
  -ov \
  -format UDZO \
  "$BAD_VOLUME_DMG" >/dev/null
hdiutil create \
  -volname BridgeVM \
  -srcfolder "$BAD_EXTRA_APP_STAGE" \
  -ov \
  -format UDZO \
  "$BAD_EXTRA_APP_DMG" >/dev/null

set +e
bad_link_output="$(env \
  BRIDGEVM_MACOS_BUNDLE_DIR="$OUT_DIR" \
  BRIDGEVM_MACOS_APP="$CUSTOM_APP" \
  "$BUILD_DMG_SCRIPT" --verify-only "$BAD_LINK_DMG" 2>&1)"
bad_link_status=$?
set -e
[[ "$bad_link_status" -ne 0 ]] || fail "debug DMG verify-only accepted a missing Applications symlink"
case "$bad_link_output" in
  *"BridgeVM DMG is missing Applications symlink"*) ;;
  *) fail "debug DMG verify-only did not report missing Applications symlink: $bad_link_output" ;;
esac
assert_dmg_detached "$BAD_LINK_DMG"

set +e
bad_volume_output="$(env \
  BRIDGEVM_MACOS_BUNDLE_DIR="$OUT_DIR" \
  BRIDGEVM_MACOS_APP="$CUSTOM_APP" \
  "$BUILD_DMG_SCRIPT" --verify-only "$BAD_VOLUME_DMG" 2>&1)"
bad_volume_status=$?
set -e
[[ "$bad_volume_status" -ne 0 ]] || fail "debug DMG verify-only accepted a wrong volume name"
case "$bad_volume_output" in
  *"BridgeVM DMG volume name mismatch: expected BridgeVM, got WrongBridgeVM"*) ;;
  *) fail "debug DMG verify-only did not report wrong volume name: $bad_volume_output" ;;
esac
assert_dmg_detached "$BAD_VOLUME_DMG"

set +e
bad_extra_app_output="$(env \
  BRIDGEVM_MACOS_BUNDLE_DIR="$OUT_DIR" \
  BRIDGEVM_MACOS_APP="$CUSTOM_APP" \
  "$BUILD_DMG_SCRIPT" --verify-only "$BAD_EXTRA_APP_DMG" 2>&1)"
bad_extra_app_status=$?
set -e
[[ "$bad_extra_app_status" -ne 0 ]] || fail "debug DMG verify-only accepted an extra top-level app bundle"
case "$bad_extra_app_output" in
  *"BridgeVM DMG must contain exactly one top-level app bundle, found 2"*) ;;
  *) fail "debug DMG verify-only did not report extra app bundle: $bad_extra_app_output" ;;
esac
assert_dmg_detached "$BAD_EXTRA_APP_DMG"

MOUNT_DIR="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-debug-dmg-custom-mount.XXXXXX")"
MOUNT_DIR_REAL="$(cd "$(dirname "$MOUNT_DIR")" && pwd -P)/$(basename "$MOUNT_DIR")"
DEVICE=""
ATTACH_OUTPUT=""
cleanup_mount() {
  if [[ -n "$DEVICE" ]]; then
    detach_mount "$DEVICE" "$MOUNT_DIR" "$MOUNT_DIR_REAL" || true
  fi
  if ! is_mountpoint_mounted "$MOUNT_DIR" "$MOUNT_DIR_REAL"; then
    rm -rf "$MOUNT_DIR"
  fi
}
trap 'cleanup_mount; cleanup' EXIT

ATTACH_OUTPUT="$(hdiutil attach "$DMG" -readonly -nobrowse -mountpoint "$MOUNT_DIR")"
DEVICE="$(printf '%s\n' "$ATTACH_OUTPUT" | awk '$1 ~ /^\/dev\/disk[0-9]+$/ { print $1; exit }')"
[[ -n "$DEVICE" ]] || fail "failed to attach custom app name DMG"
[[ -d "$MOUNT_DIR/$CUSTOM_APP_NAME.app" ]] || fail "custom app was missing from DMG"
[[ ! -e "$MOUNT_DIR/BridgeVMApp.app" ]] || fail "stale default app was packaged into DMG"
"$BUILD_APP_SCRIPT" --verify-only "$MOUNT_DIR/$CUSTOM_APP_NAME.app" >/dev/null

echo "PASS: macOS debug DMG custom app name smoke"
