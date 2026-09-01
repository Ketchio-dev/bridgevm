#!/usr/bin/env bash
# Build the WinPE-scripted Windows 11 ARM64 installer source disk, including a
# sealed user-supplied signed virtio payload and BridgeVM-owned guest agent.
# Host-side only; run run-hvf-windows-scripted-install.sh afterwards.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/hvf-disk-image-utils.sh"
ISO="${ISO:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/ISO/Win11_25H2_English_Arm64_v2.iso}"
ASSETS="${ASSETS:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/win-assets}"; WIMLIB="${WIMLIB:-}"
OUT="${OUT:-$HOME/BridgeVM/win-nvme-src.raw}"
SIZE_BYTES="${SIZE_BYTES:-17179869184}" # 16 GiB
SWM_SPLIT_MB="${SWM_SPLIT_MB:-3800}"     # keep each .swm < FAT32 4 GiB limit
log() { printf '[build-src] %s\n' "$*"; }
[[ -f "$ISO" ]] || { echo "FAIL: ISO not found: $ISO" >&2; exit 1; }
for f in winpeshl.ini bvinstall.cmd bvdiskpart.txt bvagent.ps1 bvagent-firstboot.ps1; do
  [[ -f "$ASSETS/$f" ]] || { echo "FAIL: missing asset $ASSETS/$f" >&2; exit 1; }
done
[[ "$WIMLIB" == /* && -x "$WIMLIB" ]] || { echo "FAIL: WIMLIB must name an absolute executable" >&2; exit 1; }
cleanup() {
  [[ -n "${ISO_DEV:-}" ]] && bridgevm_detach_image "$ISO_DEV" 2>/dev/null || true
  [[ -n "${DST_DEV:-}" ]] && bridgevm_detach_image "$DST_DEV" 2>/dev/null || true
  [[ -n "${MOUNT_ROOT:-}" && -d "$MOUNT_ROOT/provisioning" ]] && rm -rf "$MOUNT_ROOT/provisioning" || true
  [[ -n "${MOUNT_ROOT:-}" ]] && rmdir "$MOUNT_ROOT/iso" "$MOUNT_ROOT/dst" "$MOUNT_ROOT" 2>/dev/null || true
}
trap cleanup EXIT
MOUNT_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-win-source.XXXXXX")"
ISO_MNT="$MOUNT_ROOT/iso"
DST_VOL="$MOUNT_ROOT/dst"
mkdir "$ISO_MNT" "$DST_VOL"
PROVISION_STAGE="$MOUNT_ROOT/provisioning"
"$SCRIPT_DIR/stage-hvf-windows-guest-payload.sh" \
  --payload-dir "${WINDOWS_GUEST_PAYLOAD_DIR:-}" --manifest "${WINDOWS_GUEST_PAYLOAD_MANIFEST:-}" \
  --assets "$ASSETS" --output "$PROVISION_STAGE" \
  --catalog-verifier "${WINDOWS_GUEST_PAYLOAD_CATALOG_VERIFIER:-}"
UNATTEND_PATH="${WINDOWS_UNATTEND_PATH:-$ASSETS/unattend.xml}"
[[ "$UNATTEND_PATH" == /* && -f "$UNATTEND_PATH" && ! -L "$UNATTEND_PATH" ]] || { echo "FAIL: unattend answer file must be an absolute regular non-symlink file" >&2; exit 1; }
log "attaching ISO"
ISO_ATTACH="$(hdiutil attach -readonly -nobrowse -mountpoint "$ISO_MNT" "$ISO")"
ISO_DEV="$(awk 'NR==1{print $1}' <<<"$ISO_ATTACH")"
[[ "$ISO_DEV" == /dev/* && -d "$ISO_MNT" ]] || { echo "FAIL: ISO attach did not produce a device and mount" >&2; exit 1; }
log "ISO mounted at $ISO_MNT"
log "creating destination raw $OUT ($SIZE_BYTES bytes)"
mkdir -p "$(dirname "$OUT")"
rm -f "$OUT"
# qemu-img-less: make a sparse file then partition via hdiutil.
mkfile -n "$SIZE_BYTES" "$OUT" 2>/dev/null || dd if=/dev/zero of="$OUT" bs=1 count=0 seek="$SIZE_BYTES"
DST_DEV="$(hdiutil attach -imagekey diskimage-class=CRawDiskImage -nomount "$OUT" | awk 'NR==1{print $1}')"
log "destination attached at $DST_DEV"
diskutil partitionDisk "$DST_DEV" GPT FAT32 WINSETUP 100%
bridgevm_detach_image "$DST_DEV"
DST_DEV=""
DST_ATTACH="$(hdiutil attach -imagekey diskimage-class=CRawDiskImage -nobrowse -mountpoint "$DST_VOL" "$OUT")"
DST_DEV="$(awk 'NR==1{print $1}' <<<"$DST_ATTACH")"
[[ "$DST_DEV" == /dev/* && -d "$DST_VOL" ]] || { echo "FAIL: destination attach did not produce a device and mount" >&2; exit 1; }
log "copying ISO tree (excluding install.wim)"
rsync -a --exclude 'sources/install.wim' "$ISO_MNT"/ "$DST_VOL"/
mkdir -p "$DST_VOL/bridgevm"
cp -R "$PROVISION_STAGE" "$DST_VOL/bridgevm/provisioning"
rm -rf "$PROVISION_STAGE"
log "staging unattend.xml at source root"
cp "$UNATTEND_PATH" "$DST_VOL/unattend.xml"

log "splitting install.wim -> install.swm/install*.swm (<${SWM_SPLIT_MB}MB each)"
"$WIMLIB" split "$ISO_MNT/sources/install.wim" "$DST_VOL/sources/install.swm" "$SWM_SPLIT_MB"

log "injecting bvinstall payload into boot.wim image 2"
"$WIMLIB" update "$DST_VOL/sources/boot.wim" 2 <<UPDATE
add "$ASSETS/winpeshl.ini" /Windows/System32/winpeshl.ini
add "$ASSETS/bvinstall.cmd" /Windows/System32/bvinstall.cmd
add "$ASSETS/bvdiskpart.txt" /Windows/System32/bvdiskpart.txt
UPDATE

log "verifying payload + boot files"
"$WIMLIB" dir "$DST_VOL/sources/boot.wim" 2 | grep -E 'bvinstall.cmd|bvdiskpart.txt|winpeshl.ini' || {
  echo "FAIL: payload not present in boot.wim" >&2; exit 1; }
[[ -f "$DST_VOL/efi/boot/bootaa64.efi" ]] || { echo "FAIL: bootaa64.efi missing" >&2; exit 1; }
[[ -f "$DST_VOL/sources/install.swm" ]] || { echo "FAIL: install.swm missing" >&2; exit 1; }
[[ -f "$DST_VOL/bridgevm/provisioning/payload-receipt.tsv" ]] || { echo "FAIL: sealed guest payload missing" >&2; exit 1; }

sync
bridgevm_detach_image "$DST_DEV"
DST_DEV=""
bridgevm_detach_image "$ISO_DEV"
ISO_DEV=""
SOURCE_SHA256="$(/usr/bin/openssl dgst -sha256 -r "$OUT" | cut -d' ' -f1)"
[[ "$SOURCE_SHA256" =~ ^[0-9a-f]{64}$ ]] || { echo "FAIL: source digest failed" >&2; exit 1; }
printf '%s\n' "$SOURCE_SHA256" > "$OUT.sha256.tmp"
mv "$OUT.sha256.tmp" "$OUT.sha256"

log "DONE: scripted installer source at $OUT"
log "next: scripts/run-hvf-windows-scripted-install.sh --source $OUT --target <NSID2.raw> --vars <vars.fd> ..."
