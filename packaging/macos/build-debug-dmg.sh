#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
. "$ROOT/packaging/macos/app-name.sh"
OUT_DIR="${BRIDGEVM_MACOS_BUNDLE_DIR:-$ROOT/target/macos}"
APP_NAME="${BRIDGEVM_MACOS_APP_NAME:-BridgeVMApp}"
bridgevm_validate_macos_app_name "$APP_NAME" BRIDGEVM_MACOS_APP_NAME || exit 2
APP="${BRIDGEVM_MACOS_APP:-$OUT_DIR/$APP_NAME.app}"
APP_BASENAME="$(basename "$APP")"
DMG="${BRIDGEVM_MACOS_DMG:-$OUT_DIR/BridgeVM.dmg}"
VOLUME_NAME="${BRIDGEVM_MACOS_DMG_VOLUME:-BridgeVM}"

usage() {
  cat >&2 <<'EOF'
usage: packaging/macos/build-debug-dmg.sh [--verify-only PATH]

Builds a local debug BridgeVM.dmg containing a BridgeVM app bundle. The default debug
app bundle is refreshed first unless BRIDGEVM_MACOS_APP points at a prebuilt app,
then the selected app is verified and packaged.

Environment:
  BRIDGEVM_MACOS_APP_NAME    app bundle basename without .app, defaults to BridgeVMApp
  BRIDGEVM_MACOS_APP         app bundle to package, defaults to target/macos/$BRIDGEVM_MACOS_APP_NAME.app
                             when set explicitly, package the prebuilt app without rebuilding it
  BRIDGEVM_MACOS_DMG         output dmg path, defaults to target/macos/BridgeVM.dmg
  BRIDGEVM_MACOS_DMG_VOLUME  mounted volume name, defaults to BridgeVM
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

verify_app_bundle() {
  local app="$1"
  "$ROOT/packaging/macos/build-debug-app-bundle.sh" --verify-only "$app" >/dev/null
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

attach_dmg_bounded() {
  local dmg="$1"
  local mount_dir="$2"
  local output=""
  local status=1

  for _ in {1..20}; do
    set +e
    output="$(hdiutil attach "$dmg" -readonly -nobrowse -mountpoint "$mount_dir" 2>&1)"
    status=$?
    set -e
    if [[ "$status" -eq 0 ]]; then
      printf '%s\n' "$output"
      return 0
    fi
    case "$output" in
      *"Resource busy"*) sleep 0.25 ;;
      *)
        printf '%s\n' "$output" >&2
        return "$status"
        ;;
    esac
  done

  printf '%s\n' "$output" >&2
  return "$status"
}

verify_dmg() {
  local dmg="$1"
  local mount_dir
  mount_dir="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-dmg.XXXXXX")"
  local mount_dir_real
  mount_dir_real="$(cd "$(dirname "$mount_dir")" && pwd -P)/$(basename "$mount_dir")"
  local device=""
  local volume_device=""

  cleanup_mount() {
    local cleanup_device="${device:-}"
    local cleanup_mount_dir="${mount_dir:-}"
    local cleanup_mount_dir_real="${mount_dir_real:-$cleanup_mount_dir}"

    [[ -n "$cleanup_mount_dir" ]] || return 0
    if [[ -n "$cleanup_device" ]]; then
      detach_mount "$cleanup_device" "$cleanup_mount_dir" "$cleanup_mount_dir_real" || true
    fi
    if ! is_mountpoint_mounted "$cleanup_mount_dir" "$cleanup_mount_dir_real"; then
      rm -rf "$cleanup_mount_dir"
    fi
  }
  trap cleanup_mount RETURN

  [[ -f "$dmg" ]] || {
    echo "BridgeVM DMG is missing: $dmg" >&2
    return 1
  }
  hdiutil verify "$dmg" >/dev/null
  local attach_output
  attach_output="$(attach_dmg_bounded "$dmg" "$mount_dir")"
  device="$(printf '%s\n' "$attach_output" | awk '$1 ~ /^\/dev\/disk[0-9]+$/ { print $1; exit }')"
  volume_device="$(printf '%s\n' "$attach_output" | awk 'END { print $1 }')"
  [[ -n "$device" && -n "$volume_device" ]] || {
    echo "Failed to attach BridgeVM DMG: $dmg" >&2
    return 1
  }
  local volume_name
  volume_name="$(diskutil info "$volume_device" | awk -F': *' '/Volume Name:/ { print $2; exit }')"
  [[ "$volume_name" == "$VOLUME_NAME" ]] || {
    echo "BridgeVM DMG volume name mismatch: expected $VOLUME_NAME, got ${volume_name:-<missing>}" >&2
    return 1
  }
  local app_count
  local unexpected_app
  app_count="$(find "$mount_dir" -maxdepth 1 -name '*.app' -type d | wc -l | tr -d '[:space:]')"
  [[ "$app_count" == "1" ]] || {
    echo "BridgeVM DMG must contain exactly one top-level app bundle, found $app_count" >&2
    return 1
  }
  unexpected_app="$(find "$mount_dir" -maxdepth 1 -name '*.app' -type d ! -name "$APP_BASENAME" -print -quit)"
  [[ -z "$unexpected_app" ]] || {
    echo "BridgeVM DMG contains unexpected app bundle: $(basename "$unexpected_app")" >&2
    return 1
  }
  verify_app_bundle "$mount_dir/$APP_BASENAME"
  [[ -L "$mount_dir/Applications" ]] || {
    echo "BridgeVM DMG is missing Applications symlink" >&2
    return 1
  }
  [[ "$(readlink "$mount_dir/Applications")" == "/Applications" ]] || {
    echo "BridgeVM DMG Applications symlink does not target /Applications" >&2
    return 1
  }
}

if [[ -n "$VERIFY_ONLY" ]]; then
  verify_dmg "$VERIFY_ONLY"
  printf '%s\n' "$VERIFY_ONLY"
  exit 0
fi

if [[ -z "${BRIDGEVM_MACOS_APP+x}" ]]; then
  "$ROOT/packaging/macos/build-debug-app-bundle.sh" >/dev/null
fi
verify_app_bundle "$APP"

stage="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-dmg-stage.XXXXXX")"
cleanup_stage() {
  rm -rf "$stage"
}
trap cleanup_stage EXIT

ditto "$APP" "$stage/$APP_BASENAME"
ln -s /Applications "$stage/Applications"
mkdir -p "$(dirname "$DMG")"
hdiutil create \
  -volname "$VOLUME_NAME" \
  -srcfolder "$stage" \
  -ov \
  -format UDZO \
  "$DMG" >/dev/null

verify_dmg "$DMG"
printf '%s\n' "$DMG"
