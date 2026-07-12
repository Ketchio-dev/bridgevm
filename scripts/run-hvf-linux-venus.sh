#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
  cat >&2 <<'EOF'
usage: scripts/run-hvf-linux-venus.sh --disk RAW --evidence-dir DIR [options]

Required:
  --disk RAW             Linux raw disk to boot as writable NVMe NSID-1.
  --evidence-dir DIR     Directory for preflight.txt, run.log, target-stat.txt,
                         cleanup.txt, vars.fd, and ramfb/.

Options:
  --cidata RAW           Optional cloud-init/config disk as NVMe NSID-2.
  --ram-mib N            Guest RAM in MiB. Default: 4096.
  --res WIDTHxHEIGHT     Virtio-GPU display resolution. Default: 1280x800.
  --hostmem-mib N        Virtio-GPU host-visible memory in MiB. Default: 64.
                         Must be 0 or a power of two less than 4096.
  --ramfb-samples LIST   Comma-separated RAMFB sample ms values. Default: 120000.
                         Each sample must be <= 120000.
  --watchdog-ms N        Probe watchdog in milliseconds. Default: 360000.
  --max-reboots N        Maximum PSCI SYSTEM_RESET reboots. Default: 2.
  --with-xhci            Keep qemu-xhci enabled for legacy/proven device layouts.
                         Default: disabled.
  --no-net               Disable virtio-net. Default: userspace NAT enabled.
  --skip-build           Reuse target/release/examples/hvf_gic_boot_probe.
  -h, --help             Show this help.

Fixed policy:
  BRIDGEVM_DISABLE_XHCI=1 unless --with-xhci is requested
  BRIDGEVM_BOOT_PROBE_STOP_ON_LINUX=0
  BRIDGEVM_VIRTIO_GPU=1 with 3D/Venus enabled
  Fresh /opt/homebrew/share/qemu/edk2-arm-vars.fd copied to DIR/vars.fd
EOF
}

positive_integer() {
  [[ "$1" =~ ^[1-9][0-9]*$ ]]
}

nonnegative_integer() {
  [[ "$1" =~ ^[0-9]+$ ]]
}

power_of_two_or_zero() {
  nonnegative_integer "$1" || return 1
  (( 10#$1 == 0 )) && return 0
  (( (10#$1 & (10#$1 - 1)) == 0 ))
}

resolution_value() {
  [[ "$1" =~ ^[1-9][0-9]*x[1-9][0-9]*$ ]]
}

ramfb_sample_list() {
  [[ "$1" =~ ^[1-9][0-9]*(,[1-9][0-9]*)*$ ]] || return 1

  local sample
  local count=0
  local old_ifs="$IFS"
  IFS=,
  for sample in $1; do
    if (( sample > 120000 )); then
      IFS="$old_ifs"
      return 1
    fi
    count=$((count + 1))
    if (( count > 16 )); then
      IFS="$old_ifs"
      return 1
    fi
  done
  IFS="$old_ifs"
}

init_defaults() {
  DISK=""
  CIDATA=""
  EVIDENCE_DIR=""
  RAM_MIB="4096"
  GPU_RES="1280x800"
  HOSTMEM_MIB="64"
  RAMFB_SAMPLES="120000"
  WATCHDOG_MS="360000"
  MAX_REBOOTS="2"
  DISABLE_XHCI="1"
  VIRTIO_NET="1"
  SKIP_BUILD="0"
  BRIDGEVM_VENUS_PREFIX="${BRIDGEVM_VENUS_PREFIX:-"$HOME/BridgeVM/3d/prefix"}"
  BRIDGEVM_VULKAN_LIB="${BRIDGEVM_VULKAN_LIB:-/opt/homebrew/lib/libMoltenVK.dylib}"
  VARS_SOURCE="/opt/homebrew/share/qemu/edk2-arm-vars.fd"
  BIN="target/release/examples/hvf_gic_boot_probe"
  RUN_STATUS=0
  PROBE_PID=""
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --disk) [[ $# -ge 2 ]] || { usage; exit 2; }; DISK="$2"; shift 2 ;;
      --cidata) [[ $# -ge 2 ]] || { usage; exit 2; }; CIDATA="$2"; shift 2 ;;
      --evidence-dir) [[ $# -ge 2 ]] || { usage; exit 2; }; EVIDENCE_DIR="$2"; shift 2 ;;
      --ram-mib)
        [[ $# -ge 2 ]] || { usage; exit 2; }
        positive_integer "$2" || { echo "FAIL: --ram-mib requires a positive integer" >&2; exit 2; }
        RAM_MIB="$2"; shift 2
        ;;
      --res)
        [[ $# -ge 2 ]] || { usage; exit 2; }
        resolution_value "$2" || { echo "FAIL: --res requires WIDTHxHEIGHT, for example 1280x800" >&2; exit 2; }
        GPU_RES="$2"; shift 2
        ;;
      --hostmem-mib)
        [[ $# -ge 2 ]] || { usage; exit 2; }
        power_of_two_or_zero "$2" || { echo "FAIL: --hostmem-mib requires 0 or a power-of-two integer" >&2; exit 2; }
        (( 10#$2 < 4096 )) || { echo "FAIL: --hostmem-mib must be less than 4096" >&2; exit 2; }
        HOSTMEM_MIB="$2"; shift 2
        ;;
      --ramfb-samples)
        [[ $# -ge 2 ]] || { usage; exit 2; }
        ramfb_sample_list "$2" || { echo "FAIL: --ramfb-samples requires 1-16 positive comma-separated integers, each <= 120000" >&2; exit 2; }
        RAMFB_SAMPLES="$2"; shift 2
        ;;
      --watchdog-ms)
        [[ $# -ge 2 ]] || { usage; exit 2; }
        positive_integer "$2" || { echo "FAIL: --watchdog-ms requires a positive integer" >&2; exit 2; }
        WATCHDOG_MS="$2"; shift 2
        ;;
      --max-reboots)
        [[ $# -ge 2 ]] || { usage; exit 2; }
        nonnegative_integer "$2" || { echo "FAIL: --max-reboots requires a non-negative integer" >&2; exit 2; }
        MAX_REBOOTS="$2"; shift 2
        ;;
      --with-xhci) DISABLE_XHCI="0"; shift ;;
      --no-net) VIRTIO_NET="0"; shift ;;
      --skip-build) SKIP_BUILD="1"; shift ;;
      -h|--help) usage; exit 0 ;;
      *) usage; exit 2 ;;
    esac
  done
}

validate_required_paths() {
  [[ -n "$DISK" && -n "$EVIDENCE_DIR" ]] || { usage; exit 2; }
  [[ -f "$DISK" ]] || { echo "FAIL: disk image not found: $DISK" >&2; exit 1; }
  if [[ -n "$CIDATA" ]]; then
    [[ -f "$CIDATA" ]] || { echo "FAIL: cidata image not found: $CIDATA" >&2; exit 1; }
  fi
  [[ -f "$BRIDGEVM_VENUS_PREFIX/lib/libvirglrenderer.dylib" ]] || {
    echo "FAIL: Venus host library not found: $BRIDGEVM_VENUS_PREFIX/lib/libvirglrenderer.dylib" >&2
    echo "FAIL: build host dependencies with scripts/build-venus-host-deps.sh" >&2
    exit 1
  }
  [[ -f "$BRIDGEVM_VULKAN_LIB" ]] || { echo "FAIL: BRIDGEVM_VULKAN_LIB not found: $BRIDGEVM_VULKAN_LIB" >&2; exit 1; }
  [[ -f "$VARS_SOURCE" ]] || { echo "FAIL: EDK2 vars template not found: $VARS_SOURCE" >&2; exit 1; }
}

print_media_stat() {
  printf '%s:\n' "$1"
  ls -lh "$2"
  stat -f 'size=%z blocks=%b block_size=%k mtime=%Sm' "$2"
  du -h "$2"
}

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
    printf 'processes_after_cleanup:\n'
    pgrep -fl '[h]vf_gic_boot_probe|qemu-system-aarch64' || true
  } >> "$EVIDENCE_DIR/cleanup.txt" 2>&1
  exit "$status"
}

prepare_evidence_dir() {
  install -d "$EVIDENCE_DIR/ramfb"
  cp "$VARS_SOURCE" "$EVIDENCE_DIR/vars.fd"
}

write_preflight() {
  {
    date -u
    printf 'disk=%s\n' "$DISK"
    printf 'cidata=%s\n' "${CIDATA:-<none>}"
    printf 'evidence_dir=%s\n' "$EVIDENCE_DIR"
    printf 'bin=%s\n' "$BIN"
    printf 'skip_build=%s\n' "$SKIP_BUILD"
    printf 'ram_mib=%s\n' "$RAM_MIB"
    printf 'gpu_res=%s\n' "$GPU_RES"
    printf 'hostmem_mib=%s\n' "$HOSTMEM_MIB"
    printf 'ramfb_samples=%s\n' "$RAMFB_SAMPLES"
    printf 'watchdog_ms=%s\n' "$WATCHDOG_MS"
    printf 'max_reboots=%s\n' "$MAX_REBOOTS"
    printf 'disable_xhci=%s\n' "$DISABLE_XHCI"
    printf 'virtio_net=%s\n' "$VIRTIO_NET"
    printf 'venus_prefix=%s\n' "$BRIDGEVM_VENUS_PREFIX"
    printf 'vulkan_lib=%s\n' "$BRIDGEVM_VULKAN_LIB"
    printf 'vars_source=%s\n' "$VARS_SOURCE"
    printf 'vars=%s\n' "$EVIDENCE_DIR/vars.fd"
    print_media_stat before_disk_stat "$DISK"
    if [[ -n "$CIDATA" ]]; then
      print_media_stat before_cidata_stat "$CIDATA"
    fi
    printf 'before_vars_stat:\n'
    ls -lh "$EVIDENCE_DIR/vars.fd"
    printf 'stale_processes_observed:\n'
    pgrep -fl '[h]vf_gic_boot_probe|qemu-system-aarch64' || true
    printf 'stale_process_cleanup=skipped_unowned_processes\n'
  } > "$EVIDENCE_DIR/preflight.txt" 2>&1
}

build_and_sign_probe_if_needed() {
  [[ "$SKIP_BUILD" != "1" ]] || return 0
  {
    printf '\ncargo_build:\n'
    cargo build --release -p bridgevm-hvf --features venus --example hvf_gic_boot_probe
    printf '\ncodesign_force:\n'
    codesign --sign - --entitlements apps/macos/HvfRunner.entitlements --force "$BIN"
  } >> "$EVIDENCE_DIR/preflight.txt" 2>&1
}

build_env_args() {
  ENV_ARGS=(
    'BRIDGEVM_BOOT_PROBE_STOP_ON_LINUX=0'
    "BRIDGEVM_RAM_MIB=$RAM_MIB"
    "BRIDGEVM_NVME_DISK=$DISK"
    'BRIDGEVM_NVME_DISK_WRITABLE=1'
    'BRIDGEVM_VIRTIO_GPU=1'
    "BRIDGEVM_VIRTIO_GPU_RES=$GPU_RES"
    'BRIDGEVM_VIRTIO_GPU_3D=1'
    "BRIDGEVM_VIRTIO_GPU_HOSTMEM_MIB=$HOSTMEM_MIB"
    "BRIDGEVM_VENUS_PREFIX=$BRIDGEVM_VENUS_PREFIX"
    "BRIDGEVM_VULKAN_LIB=$BRIDGEVM_VULKAN_LIB"
    'BRIDGEVM_RAMFB=1'
    "BRIDGEVM_RAMFB_DUMP_DIR=$EVIDENCE_DIR/ramfb"
    "BRIDGEVM_RAMFB_SAMPLE_MS=$RAMFB_SAMPLES"
    "BRIDGEVM_BOOT_PROBE_WATCHDOG_MS=$WATCHDOG_MS"
    "BRIDGEVM_BOOT_PROBE_MAX_REBOOTS=$MAX_REBOOTS"
    "BRIDGEVM_AARCH64_UEFI_VARS=$EVIDENCE_DIR/vars.fd"
    'BRIDGEVM_AARCH64_UEFI_VARS_WRITABLE=1'
  )
  if [[ "$DISABLE_XHCI" == "1" ]]; then
    ENV_ARGS+=('BRIDGEVM_DISABLE_XHCI=1')
  fi
  if [[ "$VIRTIO_NET" == "1" ]]; then
    ENV_ARGS+=('BRIDGEVM_VIRTIO_NET=1' 'BRIDGEVM_VIRTIO_NET_BACKEND=nat')
  fi
  if [[ -n "$CIDATA" ]]; then
    ENV_ARGS+=("BRIDGEVM_NVME_DISK2=$CIDATA")
  fi
  printf '%s\n' "${ENV_ARGS[@]}"
}

write_probe_command_env() {
  {
    printf '\nentitlements:\n'
    codesign -d --entitlements - "$BIN"
    printf '\nentitlement_grep:\n'
    codesign -d --entitlements - "$BIN" 2>&1 | grep -n 'com.apple.security.hypervisor'
    printf '\ncommand_env:\n'
    build_env_args
  } >> "$EVIDENCE_DIR/preflight.txt" 2>&1
}

run_probe_process() {
  set +e
  env "${ENV_ARGS[@]}" "$BIN" > "$EVIDENCE_DIR/run.log" 2>&1
  RUN_STATUS="$?"
  set -e
}

write_target_stat() {
  {
    printf 'run_status=%s\n' "$RUN_STATUS"
    date -u
    print_media_stat after_disk_stat "$DISK"
    printf 'after_vars_stat:\n'
    ls -lh "$EVIDENCE_DIR/vars.fd"
    printf 'ramfb_files:\n'
    find "$EVIDENCE_DIR/ramfb" -maxdepth 1 -type f -print | sort
    printf 'run_log_summary_grep:\n'
    rg -n 'BV-|deviceName|hv shm map' "$EVIDENCE_DIR/run.log" || true
  } > "$EVIDENCE_DIR/target-stat.txt" 2>&1
}

run_linux_venus_probe() {
  cd "$ROOT"
  prepare_evidence_dir
  trap cleanup EXIT
  write_preflight
  build_and_sign_probe_if_needed
  write_probe_command_env
  build_env_args >/dev/null
  run_probe_process
  write_target_stat
}

init_defaults
parse_args "$@"
validate_required_paths
run_linux_venus_probe

exit "$RUN_STATUS"
