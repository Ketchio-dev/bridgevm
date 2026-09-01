#!/usr/bin/env bash
# Verify the marker written last by bvinstall.cmd on the fresh target ESP.
set -euo pipefail

[[ $# == 2 && "$1" == --target && "$2" == /* ]] || {
  echo "usage: verify-hvf-windows-install-target.sh --target RAW" >&2
  exit 2
}
TARGET="$2"
[[ -f "$TARGET" && ! -L "$TARGET" ]] || {
  echo "FAIL: install target must be a regular non-symlink file" >&2
  exit 1
}

DEVICE=""
cleanup() {
  [[ -z "$DEVICE" ]] || bridgevm_detach_image "$DEVICE" >/dev/null 2>&1 || true
}
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/hvf-disk-image-utils.sh"
trap cleanup EXIT

ATTACH="$(hdiutil attach -readonly -nobrowse \
  -imagekey diskimage-class=CRawDiskImage "$TARGET")"
DEVICE="$(awk 'NR == 1 { print $1 }' <<<"$ATTACH")"
[[ "$DEVICE" =~ ^/dev/disk[0-9]+$ ]] || {
  echo "FAIL: install target did not attach as a whole disk" >&2
  exit 1
}
EFI_DEVICE="${DEVICE}s1"
MOUNT_POINT="$(diskutil info -plist "$EFI_DEVICE" | \
  plutil -extract MountPoint raw - 2>/dev/null || true)"
if [[ -z "$MOUNT_POINT" || ! -d "$MOUNT_POINT" ]]; then
  diskutil mount readOnly "$EFI_DEVICE" >/dev/null
  MOUNT_POINT="$(diskutil info -plist "$EFI_DEVICE" | plutil -extract MountPoint raw -)"
fi
[[ "$MOUNT_POINT" == /Volumes/* && -d "$MOUNT_POINT" ]] || {
  echo "FAIL: target EFI partition did not mount read-only" >&2
  exit 1
}
MARKER="$MOUNT_POINT/EFI/BridgeVM/install-success.txt"
[[ -f "$MARKER" && ! -L "$MARKER" ]] || {
  echo "FAIL: WinPE install success marker is missing" >&2
  exit 1
}
EXPECTED=$'bridgevm-windows-install-success-v1\npayload-roles=storage,serial,network\nbcdboot=complete\n'
ACTUAL="$(tr -d '\r' < "$MARKER")"$'\n'
[[ "$ACTUAL" == "$EXPECTED" ]] || {
  echo "FAIL: WinPE install success marker is malformed" >&2
  exit 1
}
printf '%s' "$EXPECTED"
