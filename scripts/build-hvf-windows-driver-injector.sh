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
# The injector's GPT partition GUID is pinned, not random: the known-good inject
# vars address it as HD(1,GPT,<this>,0x800,0x2FF000) in Boot0003. See the
# normalisation step at the end of this script for why a random GUID is unsafe.
INJECTOR_PARTITION_UUID="${INJECTOR_PARTITION_UUID:-a0a0c780-d438-47c2-8c96-1b56363c72dd}"
# Set REMOVE_DRIVER_OEMNAMES="oem1.inf oem4.inf" to have bvinject.cmd
# dism /Remove-Driver those offline packages before staging the new tree.
# Set ENABLE_TESTSIGNING=1 when staging test-signed drivers such as viogpu3d.
# This plants a marker file consumed by bvinject.cmd inside WinPE; existing
# driver-injection runs leave Windows BCD untouched by default.
ENABLE_TESTSIGNING="${ENABLE_TESTSIGNING:-0}"
# Set SKIP_OFFLINE_DISM=1 only when re-injecting an already-installed Windows
# image.  WinPE then plants the live pnputil activation payload without first
# running offline DISM, which can stall while superseding an active display
# package.  The default remains the complete offline driver-staging path.
SKIP_OFFLINE_DISM="${SKIP_OFFLINE_DISM:-0}"
# Set QUARANTINE_VIOGPU3D=1 when the currently bound viogpu3d crashes before
# the live first-boot activation service can run. WinPE temporarily sets the
# offline VioGpu3D service to Start=4; the replacement INF restores its demand
# start value when stage 2 binds the newly staged package.
QUARANTINE_VIOGPU3D="${QUARANTINE_VIOGPU3D:-0}"
# Set DIAGNOSTICS_ONLY=1 to build a WinPE helper that only plants the GPU
# diagnostics/Vulkan probe and a one-shot live-Windows runner. It deliberately
# skips DISM driver injection and display-topology mutation in bvinject.cmd.
DIAGNOSTICS_ONLY="${DIAGNOSTICS_ONLY:-0}"
# Optional real-title payload. When set, the directory must contain the native
# ARM64 PPSSPP executable; the checked-in gated launcher replaces any stale
# launcher in the source directory on the injector image.
PPSSPP_DIR="${PPSSPP_DIR:-}"
OUT="${OUT:-$HOME/BridgeVM/win-injector.raw}"
SIZE_BYTES="${SIZE_BYTES:-1610612736}" # 1.5 GiB

log() { printf '[build-injector] %s\n' "$*"; }

[[ -f "$ISO" ]] || { echo "FAIL: ISO not found: $ISO" >&2; exit 1; }
[[ "$DIAGNOSTICS_ONLY" == "0" || "$DIAGNOSTICS_ONLY" == "1" ]] || {
  echo "FAIL: DIAGNOSTICS_ONLY must be 0 or 1" >&2
  exit 1
}
[[ "$SKIP_OFFLINE_DISM" == "0" || "$SKIP_OFFLINE_DISM" == "1" ]] || {
  echo "FAIL: SKIP_OFFLINE_DISM must be 0 or 1" >&2
  exit 1
}
[[ "$QUARANTINE_VIOGPU3D" == "0" || "$QUARANTINE_VIOGPU3D" == "1" ]] || {
  echo "FAIL: QUARANTINE_VIOGPU3D must be 0 or 1" >&2
  exit 1
}
for spec in $DRIVER_DIRS; do
  name="${spec%%:*}"
  src="${spec#*:}"
  ls "$src"/*.inf >/dev/null 2>&1 || { echo "FAIL: no .inf in driver dir $src" >&2; exit 1; }
  if [[ "$name" == "viogpu3d" ]]; then
    NEEDS_GPU_FIRSTBOOT=1
  fi
done
NEEDS_GPU_FIRSTBOOT="${NEEDS_GPU_FIRSTBOOT:-0}"
if [[ "$QUARANTINE_VIOGPU3D" == "1" && "$NEEDS_GPU_FIRSTBOOT" != "1" ]]; then
  echo "FAIL: QUARANTINE_VIOGPU3D=1 requires a staged viogpu3d package" >&2
  exit 1
fi
NEEDS_GPU_SERVICE=0
if [[ "$DIAGNOSTICS_ONLY" == "1" || "$NEEDS_GPU_FIRSTBOOT" == "1" ]]; then
  NEEDS_GPU_SERVICE=1
fi
for f in winpeshl-inject.ini bvinject.cmd; do
  [[ -f "$ASSETS/$f" ]] || { echo "FAIL: missing asset $ASSETS/$f" >&2; exit 1; }
done
if [[ "$DIAGNOSTICS_ONLY" == "1" ]]; then
  for f in bvgpu-diagnostics.ps1 bvgpu-vulkan-probe.ps1 \
    bvgpu-diagnostics-run.cmd bvgpu-diagnostics-startup.cmd; do
    [[ -f "$ASSETS/$f" ]] || { echo "FAIL: missing diagnostics-only asset $ASSETS/$f" >&2; exit 1; }
  done
  [[ -f "$ASSETS/bvgpu-d3dkmt-probe.c" ]] || {
    echo "FAIL: missing D3DKMT probe source $ASSETS/bvgpu-d3dkmt-probe.c" >&2
    exit 1
  }
fi
if [[ "$NEEDS_GPU_FIRSTBOOT" == "1" ]]; then
  for f in bvgpu-firstboot.cmd bvgpu-clean-driver-state.ps1 bvgpu-diagnostics-run.cmd; do
    [[ -f "$ASSETS/$f" ]] || { echo "FAIL: missing viogpu3d firstboot asset $ASSETS/$f" >&2; exit 1; }
  done
fi
if [[ -n "$PPSSPP_DIR" ]]; then
  [[ "$NEEDS_GPU_FIRSTBOOT" == "1" ]] || {
    echo "FAIL: PPSSPP_DIR requires a staged viogpu3d package" >&2
    exit 1
  }
  [[ -d "$PPSSPP_DIR" && -f "$PPSSPP_DIR/PPSSPPWindowsARM64.exe" ]] || {
    echo "FAIL: PPSSPP_DIR must contain PPSSPPWindowsARM64.exe: $PPSSPP_DIR" >&2
    exit 1
  }
  [[ -f "$ASSETS/bv-ppsspp-demo.cmd" && -f "$ASSETS/bv-ppsspp.ini" && \
        -f "$ASSETS/bv-ppsspp-title.json" && -f "$ASSETS/bvgpu-title-gate.ps1" && \
        -f "$ASSETS/bvgpu-real-title-gate.ps1" ]] || {
    echo "FAIL: checked-in PPSSPP promotion-gate assets are missing" >&2
    exit 1
  }
fi
if [[ "$NEEDS_GPU_SERVICE" == "1" ]]; then
  [[ -f "$ASSETS/bvgpu-diagnostics-service.c" ]] || {
    echo "FAIL: missing GPU handoff service source $ASSETS/bvgpu-diagnostics-service.c" >&2
    exit 1
  }
  command -v zig >/dev/null 2>&1 || {
    echo "FAIL: zig is required to build the ARM64 GPU handoff service" >&2
    exit 1
  }
fi
if [[ "${PLANT_AGENT:-1}" == "1" ]]; then
  [[ -f "$ASSETS/bvagent.ps1" && -f "$ASSETS/bvagent-install-service.c" ]] || {
    echo "FAIL: missing guest-agent service assets" >&2
    exit 1
  }
  command -v zig >/dev/null 2>&1 || {
    echo "FAIL: zig is required to build the ARM64 guest-agent installer service" >&2
    exit 1
  }
fi

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

if [[ -n "$PPSSPP_DIR" ]]; then
  log "staging PPSSPP real-title payload at \\apps\\ppsspp"
  mkdir -p "$DST_VOL/apps/ppsspp"
  rsync -a "$PPSSPP_DIR/" "$DST_VOL/apps/ppsspp/"
  cp "$ASSETS/bv-ppsspp-demo.cmd" "$DST_VOL/apps/ppsspp/bv-ppsspp-demo.cmd"
  cp "$ASSETS/bv-ppsspp.ini" "$DST_VOL/apps/ppsspp/bv-ppsspp.ini"
  cp "$ASSETS/bv-ppsspp-title.json" "$DST_VOL/apps/ppsspp/bv-ppsspp-title.json"
fi

# Plant the guest agent and its one-shot scheduled-task installer service at
# the source root. Enabled by default; set PLANT_AGENT=0 to skip.
if [[ "${PLANT_AGENT:-1}" == "1" && -f "$ASSETS/bvagent.ps1" ]]; then
  log "staging guest agent \\bvagent.ps1 (single elevated ONLOGON task)"
  cp "$ASSETS/bvagent.ps1" "$DST_VOL/bvagent.ps1"
  log "building ARM64 guest-agent scheduled-task installer service"
  zig cc -target aarch64-windows-gnu -Os -s \
    "$ASSETS/bvagent-install-service.c" \
    -o "$DST_VOL/bvagent-install-service.exe" \
    -municode -ladvapi32
  {
    printf 'BVAGENT-PACKAGE version=v3-share2\n'
    printf 'agent_sha256=%s\n' "$(shasum -a 256 "$ASSETS/bvagent.ps1" | awk '{print $1}')"
    printf 'installer_sha256=%s\n' "$(shasum -a 256 "$DST_VOL/bvagent-install-service.exe" | awk '{print $1}')"
    printf 'autostart=scheduled-task:onlogon:highest:interactive\n'
    printf 'uac_disabled=false\n'
  } > "$DST_VOL/bvagent-package.log"
  # Deliberately do NOT stage bvagent.bat: a Startup-folder launcher would be a
  # second autostart racing the scheduled task, and the loser's port open/close
  # churns vioser's single-open port. One autostart only.
fi

# Plant the first-boot GPU driver activation script at the source root. When a
# viogpu3d package is staged, bvinject.cmd copies it to C:\BridgeVM and registers
# a LocalSystem boot service (with RunOnce fallback) that trusts the test cert
# and forces pnputil install.
# Offline dism only STAGES the test-signed package; it neither trusts the
# self-signed publisher nor re-triggers a driver search for the present device.
if [[ -f "$ASSETS/bvgpu-firstboot.cmd" ]]; then
  log "staging first-boot GPU activation \\bvgpu-firstboot.cmd"
  cp "$ASSETS/bvgpu-firstboot.cmd" "$DST_VOL/bvgpu-firstboot.cmd"
fi
for asset in bvgpu-clean-driver-state.ps1 bvgpu-driver-preflight.ps1; do
  [[ ! -f "$ASSETS/$asset" ]] || { log "staging driver policy \\$asset"; cp "$ASSETS/$asset" "$DST_VOL/$asset"; }
done
if [[ -f "$ASSETS/bvgpu-real-title-gate.ps1" ]]; then
  log "staging real-title promotion gate \\bvgpu-real-title-gate.ps1"
  cp "$ASSETS/bvgpu-real-title-gate.ps1" "$DST_VOL/bvgpu-real-title-gate.ps1"
fi
if [[ -f "$ASSETS/bvgpu-title-gate.ps1" ]]; then
  log "staging generic title promotion gate \\bvgpu-title-gate.ps1"
  cp "$ASSETS/bvgpu-title-gate.ps1" "$DST_VOL/bvgpu-title-gate.ps1"
fi
if [[ -f "$ASSETS/bvgpu-diagnostics.ps1" ]]; then
  log "staging first-boot GPU diagnostics \\bvgpu-diagnostics.ps1"
  cp "$ASSETS/bvgpu-diagnostics.ps1" "$DST_VOL/bvgpu-diagnostics.ps1"
fi
if [[ -f "$ASSETS/bvgpu-vulkan-probe.ps1" ]]; then
  log "staging first-boot Vulkan probe \\bvgpu-vulkan-probe.ps1"
  cp "$ASSETS/bvgpu-vulkan-probe.ps1" "$DST_VOL/bvgpu-vulkan-probe.ps1"
fi
# VN_DEBUG_EXTRA lets a run flip an ICD behaviour switch (no_multi_ring,
# no_fence_feedback, ...) to test it against the ring stall. It travels as a
# file because the guest side is a .cmd with no other way to receive a
# parameter. Absent by default, and firstboot logs whatever it reads, so a run
# is always attributable to a setting.
# KEEP_RUNNING=1 stops firstboot from powering the guest off when it finishes,
# for runs that need the guest alive afterwards -- driving the agent for the
# clipboard and folder-share checks, for instance. Measurement gates leave it
# unset and keep the fast power off.
if [[ "${KEEP_RUNNING:-0}" == "1" ]]; then
  log "staging keep-running marker \\keep-running.txt"
  printf '1' > "$DST_VOL/keep-running.txt"
fi
if [[ -n "${VN_DEBUG_EXTRA:-}" ]]; then
  log "staging VN_DEBUG extra \\vn-debug-extra.txt = $VN_DEBUG_EXTRA"
  printf '%s' "$VN_DEBUG_EXTRA" > "$DST_VOL/vn-debug-extra.txt"
fi
if [[ -f "$ASSETS/bvgpu-diagnostics-run.cmd" ]]; then
  log "staging one-shot GPU diagnostics runner \\bvgpu-diagnostics-run.cmd"
  cp "$ASSETS/bvgpu-diagnostics-run.cmd" "$DST_VOL/bvgpu-diagnostics-run.cmd"
fi
# The Vulkan draw smoke gives stage 3 an image-level render assertion beyond
# the enumerate-only probe. It needs the platform-independent vulkan_core.h
# (brew install vulkan-headers) plus the checked-in embedded SPIR-V header.
VULKAN_INCLUDE="${VULKAN_INCLUDE:-/opt/homebrew/include}"
if [[ "$NEEDS_GPU_FIRSTBOOT" == "1" || "$DIAGNOSTICS_ONLY" == "1" ]]; then
  [[ -f "$VULKAN_INCLUDE/vulkan/vulkan_core.h" ]] || {
    echo "FAIL: vulkan/vulkan_core.h not under $VULKAN_INCLUDE (brew install vulkan-headers)" >&2
    exit 1
  }
  log "building ARM64 Windows Vulkan draw smoke \\bvgpu-vulkan-draw-smoke.exe"
  zig cc -target aarch64-windows-gnu -Os -s \
    -I"$VULKAN_INCLUDE" \
    "$ASSETS/../win-tests/bridgevm-vulkan-draw-smoke.c" \
    -o "$DST_VOL/bvgpu-vulkan-draw-smoke.exe"
fi
if [[ "$NEEDS_GPU_SERVICE" == "1" ]]; then
  log "building ARM64 Windows GPU handoff service \\bvgpu-diagnostics-service.exe"
  zig cc -target aarch64-windows-gnu -Os -s \
    "$ASSETS/bvgpu-diagnostics-service.c" \
    -o "$DST_VOL/bvgpu-diagnostics-service.exe" \
    -ladvapi32 -luserenv -lwtsapi32
fi
if [[ "$DIAGNOSTICS_ONLY" == "1" ]]; then
  log "building ARM64 Windows D3DKMT probe \\bvgpu-d3dkmt-probe.exe"
  zig cc -target aarch64-windows-gnu -Os -s \
    "$ASSETS/bvgpu-d3dkmt-probe.c" \
    -o "$DST_VOL/bvgpu-d3dkmt-probe.exe" \
    -lgdi32 -luser32
fi
if [[ -f "$ASSETS/bvgpu-diagnostics-startup.cmd" ]]; then
  log "staging GPU diagnostics Startup launcher \\bvgpu-diagnostics-startup.cmd"
  cp "$ASSETS/bvgpu-diagnostics-startup.cmd" "$DST_VOL/bvgpu-diagnostics-startup.cmd"
fi

if [[ "$DIAGNOSTICS_ONLY" == "1" ]]; then
  log "staging diagnostics-only intent marker \\bridgevm-diagnostics-only.txt"
  printf 'BridgeVM WinPE injector: plant GPU diagnostics without driver mutation\n' \
    > "$DST_VOL/bridgevm-diagnostics-only.txt"
fi
if [[ "$NEEDS_GPU_FIRSTBOOT" == "1" ]]; then
  [[ -f "$DST_VOL/bvgpu-diagnostics-run.cmd" ]] || {
    echo "FAIL: viogpu3d firstboot runner missing" >&2; exit 1; }
  [[ -f "$DST_VOL/bvgpu-diagnostics-service.exe" ]] || {
    echo "FAIL: viogpu3d firstboot native service missing" >&2; exit 1; }
  [[ -f "$DST_VOL/bvgpu-vulkan-draw-smoke.exe" ]] || {
    echo "FAIL: viogpu3d Vulkan draw smoke missing" >&2; exit 1; }
  [[ -f "$DST_VOL/bvgpu-clean-driver-state.ps1" ]] || {
    echo "FAIL: viogpu3d driver-state cleanup missing" >&2; exit 1; }
fi

if [[ -n "${REMOVE_DRIVER_OEMNAMES:-}" ]]; then
  log "staging offline driver-removal list \\bridgevm-remove-driver-oemnames.txt"
  # The variable is a space-separated list and must word-split, but it must not
  # glob: an entry like oem*.inf would otherwise expand against the host's cwd.
  set -f
  # shellcheck disable=SC2086 # deliberate word-splitting; globbing disabled above
  printf '%s\n' $REMOVE_DRIVER_OEMNAMES > "$DST_VOL/bridgevm-remove-driver-oemnames.txt"
  set +f
fi
if [[ "$ENABLE_TESTSIGNING" == "1" ]]; then
  log "staging testsigning marker \\bridgevm-enable-testsigning.txt"
  printf 'BridgeVM WinPE injector: enable offline Windows test-signing\n' \
    > "$DST_VOL/bridgevm-enable-testsigning.txt"
fi
if [[ "$SKIP_OFFLINE_DISM" == "1" ]]; then
  log "staging live-activation marker \\bridgevm-skip-offline-dism.txt"
  printf 'BridgeVM WinPE injector: skip offline DISM and use live pnputil activation\n' \
    > "$DST_VOL/bridgevm-skip-offline-dism.txt"
fi
if [[ "$QUARANTINE_VIOGPU3D" == "1" ]]; then
  log "staging viogpu3d boot-quarantine marker \\bridgevm-quarantine-viogpu3d.txt"
  printf 'BridgeVM WinPE injector: quarantine the crashing bound viogpu3d until live replacement\n' \
    > "$DST_VOL/bridgevm-quarantine-viogpu3d.txt"
fi
if [[ "$DIAGNOSTICS_ONLY" == "1" ]]; then
  [[ -f "$DST_VOL/bridgevm-diagnostics-only.txt" ]] || {
    echo "FAIL: diagnostics-only marker missing" >&2; exit 1; }
  [[ -f "$DST_VOL/bvgpu-diagnostics-run.cmd" ]] || {
    echo "FAIL: diagnostics-only runner missing" >&2; exit 1; }
  [[ -f "$DST_VOL/bvgpu-diagnostics-service.exe" ]] || {
    echo "FAIL: diagnostics-only native service missing" >&2; exit 1; }
  [[ -f "$DST_VOL/bvgpu-d3dkmt-probe.exe" ]] || {
    echo "FAIL: diagnostics-only D3DKMT probe missing" >&2; exit 1; }
  [[ -f "$DST_VOL/bvgpu-diagnostics-startup.cmd" ]] || {
    echo "FAIL: diagnostics-only Startup launcher missing" >&2; exit 1; }
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
if [[ "$SKIP_OFFLINE_DISM" == "1" ]]; then
  [[ -f "$DST_VOL/bridgevm-skip-offline-dism.txt" ]] || {
    echo "FAIL: skip-offline-DISM marker missing" >&2; exit 1; }
fi
if [[ "$QUARANTINE_VIOGPU3D" == "1" ]]; then
  [[ -f "$DST_VOL/bridgevm-quarantine-viogpu3d.txt" ]] || {
    echo "FAIL: viogpu3d boot-quarantine marker missing" >&2; exit 1; }
fi

sync
hdiutil detach "$DST_DEV" -quiet; DST_DEV=""
hdiutil detach "$ISO_MNT" -quiet; ISO_MNT=""

# `diskutil partitionDisk` mints a fresh random partition GUID on every build,
# but the known-good inject vars pin Boot0003 to
# HD(1,GPT,<PARTITION_UUID>,0x800,0x2FF000). A random GUID makes that boot entry
# unresolvable, and UEFI then silently falls through to the installed Windows
# partition: the run looks like it booted but injects nothing. Normalise the
# GUID so the pinned boot entry always resolves.
log "normalising GPT partition GUID to $INJECTOR_PARTITION_UUID"
PARTITION_UUID="$INJECTOR_PARTITION_UUID" python3 - "$OUT" <<'PYGPT'
import os, struct, sys, uuid, zlib

path = sys.argv[1]
want = uuid.UUID(os.environ["PARTITION_UUID"]).bytes_le

with open(path, "r+b") as image:
    def rewrite(header_lba):
        image.seek(header_lba * 512)
        header = bytearray(image.read(512))
        if header[:8] != b"EFI PART":
            raise SystemExit(f"FAIL: no GPT header at LBA {header_lba}")
        header_size = struct.unpack_from("<I", header, 12)[0]
        entries_lba = struct.unpack_from("<Q", header, 72)[0]
        count, size = struct.unpack_from("<II", header, 80)
        image.seek(entries_lba * 512)
        entries = bytearray(image.read(count * size))
        entries[16:32] = want          # partition 1 unique GUID
        image.seek(entries_lba * 512)
        image.write(entries)
        struct.pack_into("<I", header, 88, zlib.crc32(bytes(entries)) & 0xFFFFFFFF)
        struct.pack_into("<I", header, 16, 0)
        struct.pack_into("<I", header, 16, zlib.crc32(bytes(header[:header_size])) & 0xFFFFFFFF)
        image.seek(header_lba * 512)
        image.write(header)

    rewrite(1)                                     # primary
    image.seek(512)
    rewrite(struct.unpack_from("<Q", image.read(512), 32)[0])   # backup
PYGPT

actual_uuid="$(python3 - "$OUT" <<'PYCHECK'
import struct, sys, uuid
with open(sys.argv[1], "rb") as image:
    image.seek(512)
    header = image.read(512)
    entries_lba = struct.unpack_from("<Q", header, 72)[0]
    image.seek(entries_lba * 512)
    print(uuid.UUID(bytes_le=image.read(128)[16:32]))
PYCHECK
)"
[[ "$actual_uuid" == "$INJECTOR_PARTITION_UUID" ]] || {
  echo "FAIL: GPT partition GUID is $actual_uuid, expected $INJECTOR_PARTITION_UUID" >&2
  exit 1
}

log "DONE: driver injector at $OUT"
log "run: run-hvf-windows-installed-boot.sh with NSID1=this injector, NSID2=desktop target"
