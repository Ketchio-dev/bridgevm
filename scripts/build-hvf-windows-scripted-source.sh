#!/usr/bin/env bash
# Build the WinPE-scripted Windows 11 ARM64 installer source disk that the
# from-scratch HVF VMM boots from NVMe (NSID 1). The produced raw image holds
# a GPT + FAT32 "WINSETUP" volume with the full ISO tree, the >4GB install.wim
# split into FAT32-safe .swm parts, and a boot.wim whose Setup image runs our
# bvinstall.cmd (diskpart + dism + bcdboot) instead of setup.exe.
#
# This reconstructs the source disk that used to live at
# /tmp/bridgevm-a2-winsetup-scripted-fat32.raw (lost to /tmp cleanup). Output
# goes to a PERSISTENT location by default so it survives across sessions.
#
# Host-side only (no HVF / no live boot). Requires: hdiutil, wimlib-imagex,
# rsync. Run the install itself afterwards with
# scripts/run-hvf-windows-scripted-install.sh --source <this image>.
set -euo pipefail

ISO="${ISO:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/ISO/Win11_25H2_English_Arm64_v2.iso}"
ASSETS="${ASSETS:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/win-assets}"
OUT="${OUT:-$HOME/BridgeVM/win-nvme-src.raw}"
SIZE_BYTES="${SIZE_BYTES:-17179869184}" # 16 GiB
SWM_SPLIT_MB="${SWM_SPLIT_MB:-3800}"     # keep each .swm < FAT32 4 GiB limit

log() { printf '[build-src] %s\n' "$*"; }

[[ -f "$ISO" ]] || { echo "FAIL: ISO not found: $ISO" >&2; exit 1; }
for f in winpeshl.ini bvinstall.cmd bvdiskpart.txt; do
  [[ -f "$ASSETS/$f" ]] || { echo "FAIL: missing asset $ASSETS/$f" >&2; exit 1; }
done
command -v wimlib-imagex >/dev/null || { echo "FAIL: wimlib-imagex not on PATH" >&2; exit 1; }

cleanup() {
  [[ -n "${ISO_MNT:-}" ]] && hdiutil detach "$ISO_MNT" -quiet 2>/dev/null || true
  [[ -n "${DST_DEV:-}" ]] && hdiutil detach "$DST_DEV" -quiet 2>/dev/null || true
}
trap cleanup EXIT

# 1. Mount the ISO read-only.
log "attaching ISO"
ISO_ATTACH="$(hdiutil attach -nomount "$ISO")"
ISO_DEV="$(awk 'NR==1{print $1}' <<<"$ISO_ATTACH")"
ISO_MNT="$(hdiutil mount "$ISO" | awk '{print $NF}' | tail -1)"
log "ISO mounted at $ISO_MNT"

# 2. Create + GPT/FAT32-format the destination raw.
log "creating destination raw $OUT ($SIZE_BYTES bytes)"
rm -f "$OUT"
# qemu-img-less: make a sparse file then partition via hdiutil.
mkfile -n "$SIZE_BYTES" "$OUT" 2>/dev/null || dd if=/dev/zero of="$OUT" bs=1 count=0 seek="$SIZE_BYTES"
DST_DEV="$(hdiutil attach -nomount "$OUT" | awk 'NR==1{print $1}')"
log "destination attached at $DST_DEV"
diskutil partitionDisk "$DST_DEV" GPT FAT32 WINSETUP 100%
DST_VOL="/Volumes/WINSETUP"

# 3. Copy the ISO tree except the oversized install.wim.
log "copying ISO tree (excluding install.wim)"
rsync -a --exclude 'sources/install.wim' "$ISO_MNT"/ "$DST_VOL"/

# 3b. Place the OOBE-skip/autologon unattend at the source root; bvinstall.cmd
# copies it to W:\Windows\Panther\unattend.xml after the WIM apply.
if [[ -f "$ASSETS/unattend.xml" ]]; then
  log "staging unattend.xml at source root"
  cp "$ASSETS/unattend.xml" "$DST_VOL/unattend.xml"
fi

# 4. Split install.wim into FAT32-safe .swm parts on the destination.
log "splitting install.wim -> install.swm/install*.swm (<${SWM_SPLIT_MB}MB each)"
wimlib-imagex split "$ISO_MNT/sources/install.wim" "$DST_VOL/sources/install.swm" "$SWM_SPLIT_MB"

# 5. Inject the scripted-install payload into boot.wim image 2 (Windows Setup).
log "injecting bvinstall payload into boot.wim image 2"
wimlib-imagex update "$DST_VOL/sources/boot.wim" 2 <<UPDATE
add $ASSETS/winpeshl.ini /Windows/System32/winpeshl.ini
add $ASSETS/bvinstall.cmd /Windows/System32/bvinstall.cmd
add $ASSETS/bvdiskpart.txt /Windows/System32/bvdiskpart.txt
UPDATE

# 6. Verify the injected paths and required boot files.
log "verifying payload + boot files"
wimlib-imagex dir "$DST_VOL/sources/boot.wim" 2 | grep -E 'bvinstall.cmd|bvdiskpart.txt|winpeshl.ini' || {
  echo "FAIL: payload not present in boot.wim" >&2; exit 1; }
[[ -f "$DST_VOL/efi/boot/bootaa64.efi" ]] || { echo "FAIL: bootaa64.efi missing" >&2; exit 1; }
[[ -f "$DST_VOL/sources/install.swm" ]] || { echo "FAIL: install.swm missing" >&2; exit 1; }

sync
hdiutil detach "$DST_DEV" -quiet
DST_DEV=""
hdiutil detach "$ISO_MNT" -quiet
ISO_MNT=""

log "DONE: scripted installer source at $OUT"
log "next: scripts/run-hvf-windows-scripted-install.sh --source $OUT --target <NSID2.raw> --vars <vars.fd> ..."
