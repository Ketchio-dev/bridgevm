#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage: scripts/run-hvf-windows-scripted-install.sh --source RAW --target RAW --vars FD --evidence-dir DIR [options]

Required:
  --source RAW            Bootable WinPE scripted installer raw disk.
  --target RAW            NSID-2 target raw disk to write.
  --vars FD               Writable UEFI vars file.
  --evidence-dir DIR      Directory for preflight.txt, run.log, target-stat.txt, cleanup.txt, ramfb/.

Options:
  --fresh-target-size N   Remove --target first, then create a sparse target with mkfile -n N.
  --vars-template FD      Remove --vars first, then copy this template before boot.
  --cleanup-created-media Remove target/vars at exit, but only files created by this script.
  --watchdog-ms N         Probe watchdog in milliseconds. Default: 280000.
  --max-reboots N         Maximum PSCI SYSTEM_RESET reboots. Default: 8.
  --ram-mib N             Guest RAM in MiB. Default: 4096.
  --release               Build and run target/release/examples/hvf_gic_boot_probe.
  --skip-build            Reuse the selected profile's existing hvf_gic_boot_probe.
  --print-policy          Print the enforced policy and exit.
  -h, --help              Show this help.

Policy:
  The script always launches the probe with BRIDGEVM_DISABLE_XHCI=1.
EOF
}

positive_integer() {
  [[ "$1" =~ ^[1-9][0-9]*$ ]]
}

nonnegative_integer() {
  [[ "$1" =~ ^[0-9]+$ ]]
}

absolute_media_path() {
  local path="$1"
  local dir
  local base
  case "$path" in
    /*) ;;
    *) path="$PWD/$path" ;;
  esac
  dir="$(dirname "$path")"
  base="$(basename "$path")"
  if [[ -d "$dir" ]]; then
    (cd "$dir" && printf '%s/%s\n' "$(pwd -P)" "$base")
  else
    printf '%s\n' "$path"
  fi
}

path_has_parent_component() {
  case "$1" in
    ..|../*|*/..|*/../*) return 0 ;;
    *) return 1 ;;
  esac
}

require_destructive_media_path() {
  local label="$1"
  local path
  if path_has_parent_component "$2"; then
    echo "FAIL: destructive $label path must not contain '..' components: $2" >&2
    exit 2
  fi
  path="$(absolute_media_path "$2")"
  case "$path" in
    /tmp/bridgevm-*|/private/tmp/bridgevm-*) ;;
    *)
      echo "FAIL: destructive $label path must be under /tmp/bridgevm-*: $2" >&2
      exit 2
      ;;
  esac
  case "$path" in
    /tmp/bridgevm-c3-unattend-target.raw|/private/tmp/bridgevm-c3-unattend-target.raw|\
    /tmp/bridgevm-c3-unattend-vars.fd|/private/tmp/bridgevm-c3-unattend-vars.fd|\
    /tmp/bridgevm-c3-placeholder-nsid1.raw|/private/tmp/bridgevm-c3-placeholder-nsid1.raw)
      echo "FAIL: destructive $label path matches preserved source media: $2" >&2
      exit 2
      ;;
  esac
}

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE=""
TARGET=""
VARS=""
EVIDENCE_DIR=""
FRESH_TARGET_SIZE=""
VARS_TEMPLATE=""
CLEANUP_CREATED_MEDIA="0"
WATCHDOG_MS="280000"
MAX_REBOOTS="8"
RAM_MIB="4096"
BUILD_PROFILE="debug"
SKIP_BUILD="0"
PRINT_POLICY="0"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --source)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      SOURCE="$2"
      shift 2
      ;;
    --target)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      TARGET="$2"
      shift 2
      ;;
    --vars)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      VARS="$2"
      shift 2
      ;;
    --evidence-dir)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      EVIDENCE_DIR="$2"
      shift 2
      ;;
    --fresh-target-size)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      FRESH_TARGET_SIZE="$2"
      shift 2
      ;;
    --vars-template)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      VARS_TEMPLATE="$2"
      shift 2
      ;;
    --cleanup-created-media)
      CLEANUP_CREATED_MEDIA="1"
      shift
      ;;
    --watchdog-ms)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      positive_integer "$2" || { echo "FAIL: --watchdog-ms requires a positive integer" >&2; exit 2; }
      WATCHDOG_MS="$2"
      shift 2
      ;;
    --max-reboots)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      nonnegative_integer "$2" || { echo "FAIL: --max-reboots requires a non-negative integer" >&2; exit 2; }
      MAX_REBOOTS="$2"
      shift 2
      ;;
    --ram-mib)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      positive_integer "$2" || { echo "FAIL: --ram-mib requires a positive integer" >&2; exit 2; }
      RAM_MIB="$2"
      shift 2
      ;;
    --release)
      BUILD_PROFILE="release"
      shift
      ;;
    --skip-build)
      SKIP_BUILD="1"
      shift
      ;;
    --print-policy)
      PRINT_POLICY="1"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage
      exit 2
      ;;
  esac
done

if [[ "$PRINT_POLICY" == "1" ]]; then
  printf '%s\n' \
    'BRIDGEVM_DISABLE_XHCI=1' \
    'reason=xHCI-present path blocked scripted WinPE install in A2; install is keyboard-free'
  exit 0
fi

[[ -n "$SOURCE" && -n "$TARGET" && -n "$VARS" && -n "$EVIDENCE_DIR" ]] || {
  usage
  exit 2
}
[[ -f "$SOURCE" ]] || { echo "FAIL: source image not found: $SOURCE" >&2; exit 1; }
if [[ -n "$VARS_TEMPLATE" ]]; then
  [[ -f "$VARS_TEMPLATE" ]] || { echo "FAIL: vars template not found: $VARS_TEMPLATE" >&2; exit 1; }
fi
if [[ "$CLEANUP_CREATED_MEDIA" == "1" ]]; then
  [[ -n "$FRESH_TARGET_SIZE" || -n "$VARS_TEMPLATE" ]] || {
    echo "FAIL: --cleanup-created-media requires --fresh-target-size and/or --vars-template" >&2
    exit 2
  }
fi

cd "$ROOT"
install -d "$EVIDENCE_DIR/ramfb"

CREATED_TARGET="0"
CREATED_VARS="0"
RUN_STATUS=0
PROBE_PID=""

terminate_owned_probe() {
  [[ -n "$PROBE_PID" ]] || return 0
  kill -0 "$PROBE_PID" 2>/dev/null || return 0
  pkill -TERM -P "$PROBE_PID" 2>/dev/null || true
  kill -TERM "$PROBE_PID" 2>/dev/null || true
  local wait_count=0
  while kill -0 "$PROBE_PID" 2>/dev/null && (( wait_count < 20 )); do
    sleep 0.1
    wait_count=$((wait_count + 1))
  done
  if kill -0 "$PROBE_PID" 2>/dev/null; then
    pkill -KILL -P "$PROBE_PID" 2>/dev/null || true
    kill -KILL "$PROBE_PID" 2>/dev/null || true
  fi
}

cleanup() {
  local status="$?"
  set +e
  {
    printf '\ncleanup_status=%s\n' "$status"
    date -u
    printf 'processes_before_cleanup:\n'
    pgrep -fl '[h]vf_gic_boot_probe|qemu-system-aarch64' || true
    terminate_owned_probe
    if [[ "$CLEANUP_CREATED_MEDIA" == "1" && "$CREATED_TARGET" == "1" ]]; then
      rm -f "$TARGET"
      printf 'removed_target=%s\n' "$TARGET"
    fi
    if [[ "$CLEANUP_CREATED_MEDIA" == "1" && "$CREATED_VARS" == "1" ]]; then
      rm -f "$VARS"
      printf 'removed_vars=%s\n' "$VARS"
    fi
    printf 'processes_after_cleanup:\n'
    pgrep -fl '[h]vf_gic_boot_probe|qemu-system-aarch64' || true
    printf 'tmux_sessions_after_cleanup:\n'
    tmux ls 2>/dev/null || true
  } >> "$EVIDENCE_DIR/cleanup.txt" 2>&1
  exit "$status"
}
trap cleanup EXIT

if [[ -n "$FRESH_TARGET_SIZE" ]]; then
  require_destructive_media_path target "$TARGET"
  rm -f "$TARGET"
  mkfile -n "$FRESH_TARGET_SIZE" "$TARGET"
  CREATED_TARGET="1"
else
  [[ -f "$TARGET" ]] || { echo "FAIL: target image not found: $TARGET" >&2; exit 1; }
fi
if [[ -n "$VARS_TEMPLATE" ]]; then
  require_destructive_media_path vars "$VARS"
  rm -f "$VARS"
  cp "$VARS_TEMPLATE" "$VARS"
  CREATED_VARS="1"
else
  [[ -f "$VARS" ]] || { echo "FAIL: vars file not found: $VARS" >&2; exit 1; }
fi

if [[ "$BUILD_PROFILE" == "release" ]]; then
  BIN="target/release/examples/hvf_gic_boot_probe"
else
  BIN="target/debug/examples/hvf_gic_boot_probe"
fi
{
  date -u
  printf 'source=%s\n' "$SOURCE"
  printf 'target=%s\n' "$TARGET"
  printf 'vars=%s\n' "$VARS"
  printf 'evidence_dir=%s\n' "$EVIDENCE_DIR"
  printf 'build_profile=%s\n' "$BUILD_PROFILE"
  printf 'policy=BRIDGEVM_DISABLE_XHCI=1\n'
  printf 'source_stat:\n'
  ls -lh "$SOURCE"
  file "$SOURCE"
  printf 'before_target_stat:\n'
  ls -lh "$TARGET"
  stat -f 'size=%z blocks=%b block_size=%k mtime=%Sm' "$TARGET"
  du -h "$TARGET"
  printf 'before_vars_stat:\n'
  ls -lh "$VARS"
  printf 'stale_processes_observed:\n'
  pgrep -fl '[h]vf_gic_boot_probe|qemu-system-aarch64' || true
  printf 'stale_process_cleanup=skipped_unowned_processes\n'
} > "$EVIDENCE_DIR/preflight.txt" 2>&1

if [[ "$SKIP_BUILD" != "1" ]]; then
  {
    printf '\ncargo_build:\n'
    if [[ "$BUILD_PROFILE" == "release" ]]; then
      cargo build --release -p bridgevm-hvf --example hvf_gic_boot_probe
    else
      cargo build -p bridgevm-hvf --example hvf_gic_boot_probe
    fi
    printf '\ncodesign_force:\n'
    codesign --sign - --entitlements apps/macos/HvfRunner.entitlements --force "$BIN"
  } >> "$EVIDENCE_DIR/preflight.txt" 2>&1
fi
{
  printf '\nentitlements:\n'
  codesign -d --entitlements - "$BIN"
  printf '\nentitlement_grep:\n'
  codesign -d --entitlements - "$BIN" 2>&1 | grep -n 'com.apple.security.hypervisor'
  printf '\ncommand_env:\n'
  printf '%s\n' \
    'BRIDGEVM_DISABLE_XHCI=1' \
    "BRIDGEVM_RAM_MIB=$RAM_MIB" \
    'BRIDGEVM_RAMFB=1' \
    "BRIDGEVM_RAMFB_DUMP_DIR=$EVIDENCE_DIR/ramfb" \
    'BRIDGEVM_RAMFB_SAMPLE_MS=1000,5000,15000,60000,100000' \
    "BRIDGEVM_NVME_DISK=$SOURCE" \
    "BRIDGEVM_NVME_DISK2=$TARGET" \
    'BRIDGEVM_NVME_DISK2_WRITABLE=1' \
    "BRIDGEVM_AARCH64_UEFI_VARS=$VARS" \
    'BRIDGEVM_AARCH64_UEFI_VARS_WRITABLE=1' \
    "BRIDGEVM_BOOT_PROBE_WATCHDOG_MS=$WATCHDOG_MS" \
    "BRIDGEVM_BOOT_PROBE_MAX_REBOOTS=$MAX_REBOOTS" \
    'BRIDGEVM_RECENT_NVME_COMMANDS=4096' \
    'BRIDGEVM_RECENT_PCIE_MMIO=2048' \
    'BRIDGEVM_RECENT_PCIE_PIO=1024' \
    'BRIDGEVM_TRACE_MSIX=1' \
    'BRIDGEVM_TRACE_SPI=1'
} >> "$EVIDENCE_DIR/preflight.txt" 2>&1

set +e
env \
  BRIDGEVM_DISABLE_XHCI=1 \
  BRIDGEVM_RAM_MIB="$RAM_MIB" \
  BRIDGEVM_RAMFB=1 \
  BRIDGEVM_RAMFB_DUMP_DIR="$EVIDENCE_DIR/ramfb" \
  BRIDGEVM_RAMFB_SAMPLE_MS=1000,5000,15000,60000,100000 \
  BRIDGEVM_NVME_DISK="$SOURCE" \
  BRIDGEVM_NVME_DISK2="$TARGET" \
  BRIDGEVM_NVME_DISK2_WRITABLE=1 \
  BRIDGEVM_AARCH64_UEFI_VARS="$VARS" \
  BRIDGEVM_AARCH64_UEFI_VARS_WRITABLE=1 \
  BRIDGEVM_BOOT_PROBE_WATCHDOG_MS="$WATCHDOG_MS" \
  BRIDGEVM_BOOT_PROBE_MAX_REBOOTS="$MAX_REBOOTS" \
  BRIDGEVM_RECENT_NVME_COMMANDS=4096 \
  BRIDGEVM_RECENT_PCIE_MMIO=2048 \
  BRIDGEVM_RECENT_PCIE_PIO=1024 \
  BRIDGEVM_TRACE_MSIX=1 \
  BRIDGEVM_TRACE_SPI=1 \
  "$BIN" > "$EVIDENCE_DIR/run.log" 2>&1 &
PROBE_PID="$!"
wait "$PROBE_PID"
RUN_STATUS="$?"
PROBE_PID=""
set -e

{
  printf 'run_status=%s\n' "$RUN_STATUS"
  date -u
  printf 'after_target_stat:\n'
  ls -lh "$TARGET"
  stat -f 'size=%z blocks=%b block_size=%k mtime=%Sm' "$TARGET"
  du -h "$TARGET"
  printf 'after_vars_stat:\n'
  ls -lh "$VARS"
  printf 'ramfb_files:\n'
  find "$EVIDENCE_DIR/ramfb" -maxdepth 1 -type f -print | sort
  printf 'run_log_summary_grep:\n'
  grep -nE 'DRIVER_PNP_WATCHDOG|0x1D5|bugcheck|BridgeVM scripted install|Source drive|diskpart|DiskPart|dism|Apply-Image|op=0x01|nsid=2|storage target effect|exact_target_storage_evidence|target_effect_class|panic|HV_DENIED|hv_vm_create|watchdog|SYSTEM_RESET|SYSTEM_OFF|PSCI' "$EVIDENCE_DIR/run.log" || true
} > "$EVIDENCE_DIR/target-stat.txt" 2>&1

exit "$RUN_STATUS"
