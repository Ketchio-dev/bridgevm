#!/usr/bin/env bash
# Build a minimal bootable WinPE "driver injector" disk: boots WinPE from NVMe
# (NSID 1) and runs bvinject.cmd, which DISM /Add-Drivers the netkvm ARM64
# driver into the installed Windows image on the NSID-2 target, then shuts down.
# Much smaller than the full installer source (no install.wim).
#
# Host-side only. Requires hdiutil, wimlib-imagex, rsync. Output persists.
set -euo pipefail

ISO="${ISO:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/ISO/Win11_25H2_English_Arm64_v2.iso}"
ASSETS="${ASSETS:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/win-assets}"
# DRIVER_DIRS = space-separated list of "name:path" driver source dirs to stage
# under \drivers\<name>. Default: just netkvm. Example for viogpudo:
#   DRIVER_DIRS="viogpudo:$HOME/BridgeVM/drivers/viogpudo"
DRIVER_DIRS="${DRIVER_DIRS:-netkvm:$HOME/BridgeVM/drivers/netkvm}"
# Set ENABLE_TESTSIGNING=1 when staging test-signed drivers such as viogpu3d.
# This plants a marker file consumed by bvinject.cmd inside WinPE; existing
# driver-injection runs leave Windows BCD untouched by default.
ENABLE_TESTSIGNING="${ENABLE_TESTSIGNING:-0}"
OUT="${OUT:-$HOME/BridgeVM/win-injector.raw}"
SIZE_BYTES="${SIZE_BYTES:-1610612736}" # 1.5 GiB

log() { printf '[build-injector] %s\n' "$*"; }

[[ -f "$ISO" ]] || { echo "FAIL: ISO not found: $ISO" >&2; exit 1; }
for spec in $DRIVER_DIRS; do
  src="${spec#*:}"
  ls "$src"/*.inf >/dev/null 2>&1 || { echo "FAIL: no .inf in driver dir $src" >&2; exit 1; }
done
for f in winpeshl-inject.ini bvinject.cmd; do
  [[ -f "$ASSETS/$f" ]] || { echo "FAIL: missing asset $ASSETS/$f" >&2; exit 1; }
done

cleanup() {
  [[ -n "${ISO_MNT:-}" ]] && hdiutil detach "$ISO_MNT" -quiet 2>/dev/null || true
  [[ -n "${DST_DEV:-}" ]] && hdiutil detach "$DST_DEV" -quiet 2>/dev/null || true
}
trap cleanup EXIT

log "attaching ISO"
ISO_MNT="$(hdiutil mount "$ISO" | awk '{print $NF}' | tail -1)"

log "creating destination raw $OUT"
rm -f "$OUT"
mkfile -n "$SIZE_BYTES" "$OUT"
DST_DEV="$(hdiutil attach -imagekey diskimage-class=CRawDiskImage -nomount "$OUT" | awk 'NR==1{print $1}')"
diskutil partitionDisk "$DST_DEV" GPT FAT32 WINJECT 100%
DST_VOL="/Volumes/WINJECT"

# WinPE UEFI boot needs the EFI/boot loader, the BCD store, and boot.wim.
log "copying WinPE boot files (efi tree, boot, bootmgr, boot.wim)"
rsync -a "$ISO_MNT/efi" "$DST_VOL"/
[[ -d "$ISO_MNT/boot" ]] && rsync -a "$ISO_MNT/boot" "$DST_VOL"/ || true
for f in bootmgr bootmgr.efi; do [[ -e "$ISO_MNT/$f" ]] && cp "$ISO_MNT/$f" "$DST_VOL"/ || true; done
mkdir -p "$DST_VOL/sources"
cp "$ISO_MNT/sources/boot.wim" "$DST_VOL/sources/boot.wim"

for spec in $DRIVER_DIRS; do
  name="${spec%%:*}"; src="${spec#*:}"
  log "staging driver '$name' at \\drivers\\$name"
  mkdir -p "$DST_VOL/drivers/$name"
  cp "$src"/* "$DST_VOL/drivers/$name/"
done

# Plant the guest agent at the source root; bvinject.cmd copies it to C:\ and
# registers an HKLM Run key. Enabled by default; set PLANT_AGENT=0 to skip.
if [[ "${PLANT_AGENT:-1}" == "1" && -f "$ASSETS/bvagent.ps1" ]]; then
  log "staging guest agent \\bvagent.ps1 (single HKLM Run autostart)"
  cp "$ASSETS/bvagent.ps1" "$DST_VOL/bvagent.ps1"
  # Deliberately do NOT stage bvagent.bat: a Startup-folder launcher would be a
  # second autostart racing the Run key, and the loser's port open/close churns
  # vioser's single-open port. One autostart only.
fi

# Plant the first-boot GPU driver activation script at the source root. When a
# viogpu3d package is staged, bvinject.cmd copies it to C:\BridgeVM and registers
# an elevated HKLM RunOnce that trusts the test cert and forces pnputil install.
# Offline dism only STAGES the test-signed package; it neither trusts the
# self-signed publisher nor re-triggers a driver search for the present device.
if [[ -f "$ASSETS/bvgpu-firstboot.cmd" ]]; then
  log "staging first-boot GPU activation \\bvgpu-firstboot.cmd"
  cp "$ASSETS/bvgpu-firstboot.cmd" "$DST_VOL/bvgpu-firstboot.cmd"
fi

if [[ "$ENABLE_TESTSIGNING" == "1" ]]; then
  log "staging testsigning marker \\bridgevm-enable-testsigning.txt"
  printf 'BridgeVM WinPE injector: enable offline Windows test-signing\n' \
    > "$DST_VOL/bridgevm-enable-testsigning.txt"
fi

log "injecting bvinject payload into boot.wim image 2"
wimlib-imagex update "$DST_VOL/sources/boot.wim" 2 <<UPDATE
add "$ASSETS/winpeshl-inject.ini" /Windows/System32/winpeshl.ini
add "$ASSETS/bvinject.cmd" /Windows/System32/bvinject.cmd
UPDATE

log "verifying"
wimlib-imagex dir "$DST_VOL/sources/boot.wim" 2 | grep -E 'bvinject.cmd|winpeshl.ini' || {
  echo "FAIL: payload not in boot.wim" >&2; exit 1; }
[[ -f "$DST_VOL/efi/boot/bootaa64.efi" ]] || { echo "FAIL: bootaa64.efi missing" >&2; exit 1; }
ls "$DST_VOL"/drivers/*/*.inf >/dev/null 2>&1 || { echo "FAIL: no staged drivers" >&2; exit 1; }
if [[ "$ENABLE_TESTSIGNING" == "1" ]]; then
  [[ -f "$DST_VOL/bridgevm-enable-testsigning.txt" ]] || {
    echo "FAIL: testsigning marker missing" >&2; exit 1; }
fi

sync
hdiutil detach "$DST_DEV" -quiet; DST_DEV=""
hdiutil detach "$ISO_MNT" -quiet; ISO_MNT=""
log "DONE: driver injector at $OUT"
log "run: run-hvf-windows-installed-boot.sh with NSID1=this injector, NSID2=desktop target"
