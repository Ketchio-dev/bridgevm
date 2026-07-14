terminate_owned_probe() {
  [[ -n "$PROBE_PID" ]] || return 0
  kill -0 "$PROBE_PID" 2>/dev/null || return 0
  # A SIGSTOP-based host-pause proof cannot receive TERM until continued.
  # Always release a possibly stopped child before the normal TERM/KILL path.
  kill -CONT "$PROBE_PID" 2>/dev/null || true
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
    printf 'tmux_sessions_after_cleanup:\n'
    tmux ls 2>/dev/null || true
  } >> "$EVIDENCE_DIR/cleanup.txt" 2>&1
  exit "$status"
}

write_installed_boot_preflight() {
  {
    date -u
    printf 'target=%s\n' "$TARGET"
    printf 'placeholder_nsid1=%s\n' "${PLACEHOLDER_NSID1:-<none>}"
    printf 'vars=%s\n' "$VARS"
    printf 'firmware_code=%s\n' "$FIRMWARE_CODE"
    printf 'evidence_dir=%s\n' "$EVIDENCE_DIR"
    printf 'build_profile=%s\n' "$BUILD_PROFILE"
    printf 'daily_preset=%s\n' "$DAILY"
    printf 'ram_mib=%s\n' "$RAM_MIB"
    printf 'watchdog_ms=%s\n' "$WATCHDOG_MS"
    printf 'watchdog_disabled=%s\n' "$WATCHDOG_DISABLED"
    printf 'smp_cpus=%s\n' "${SMP_CPUS:-<unset>}"
    printf 'xhci_report_interval_ms=%s\n' "$([[ "$DAILY" == "1" ]] && printf '30' || printf '<probe-default 30>')"
    printf 'boot_timer=%s\n' "$BOOT_TIMER"
    printf 'boot_timer_ramfb_ms=%s\n' "${BOOT_TIMER_RAMFB_MS:-<probe-default 1000>}"
    printf 'boot_timer_desktop_checksum64=%s\n' "${BOOT_TIMER_DESKTOP_CHECKSUM64:-<unset>}"
    printf 'boot_timer_desktop_agent=%s\n' "$BOOT_TIMER_DESKTOP_AGENT"
    printf 'shutdown_after_agent_ready=%s\n' "$SHUTDOWN_AFTER_AGENT_READY"
    printf 'host_pause_resume_proof_ms=%s\n' "${HOST_PAUSE_RESUME_PROOF_MS:-<unset>}"
    printf 'agent_service_control=%s\n' "${AGENT_SERVICE_CONTROL:-<unset>}"
    printf 'agent_service_command=%s\n' "${AGENT_SERVICE_COMMAND:-<unset>}"
    printf 'agent_clipboard_sync=%s\n' "$AGENT_CLIPBOARD_SYNC"
    printf 'agent_share_host=%s\n' "${AGENT_SHARE_HOST:-<unset>}"
    printf 'agent_share_guest=%s\n' "${AGENT_SHARE_GUEST:-<unset>}"
    printf 'agent_share_ms=%s\n' "${AGENT_SHARE_MS:-<unset>}"
    printf 'agent_share_max_kb=%s\n' "${AGENT_SHARE_MAX_KB:-<unset>}"
    printf 'nvme_buffered_io=%s\n' "$NVME_BUFFERED_IO"
    if [[ "$SHUTDOWN_AFTER_AGENT_READY" == "1" || -n "$HOST_PAUSE_RESUME_PROOF_MS" || -n "$AGENT_SERVICE_CONTROL" ]]; then
      printf 'virtio_console_test_periodic=1\n'
    else
      printf 'virtio_console_test_periodic=0\n'
    fi
    if [[ "$BOOT_TIMER_DESKTOP_AGENT" == "1" || "$SHUTDOWN_AFTER_AGENT_READY" == "1" || -n "$HOST_PAUSE_RESUME_PROOF_MS" || -n "$AGENT_SERVICE_CONTROL" ]]; then
      printf 'virtio_console=1\n'
    else
      printf 'virtio_console=0\n'
    fi
    printf 'virtio_gpu_3d=%s\n' "$VIRTIO_GPU_3D"
    printf 'virtio_gpu_pci_device_id=%s\n' "${VIRTIO_GPU_PCI_DEVICE_ID:-10F7 (BRIDGEVM_VIRTIO_GPU_3D_BIND_ID alias)}"
    printf 'virtio_gpu_trace_jsonl=%s\n' "${VIRTIO_GPU_TRACE_JSONL:-$EVIDENCE_DIR/virtio-gpu.jsonl}"
    printf 'gpu_trace_protocol=%s\n' "$GPU_TRACE_PROTOCOL"
    printf 'viogpu3d_dir=%s\n' "${VIOGPU3D_DIR:-<unset>}"
    printf 'require_viogpu3d_readiness=%s\n' "$REQUIRE_VIOGPU3D_READINESS"
    printf 'policy=%s %s writable-target\n' "$XHCI_POLICY" "$BOOT_MODE"
    printf 'ramfb_samples=%s\n' "$RAMFB_SAMPLES"
    print_input_summary
    print_media_stat before_target_stat "$TARGET"
    if [[ -n "$PLACEHOLDER_NSID1" ]]; then
      print_media_stat before_placeholder_nsid1_stat "$PLACEHOLDER_NSID1"
    fi
    printf 'before_vars_stat:\n'
    ls -lh "$VARS"
    printf 'stale_processes_observed:\n'
    pgrep -fl '[h]vf_gic_boot_probe|qemu-system-aarch64' || true
    printf 'stale_process_cleanup=skipped_unowned_processes\n'
  } > "$EVIDENCE_DIR/preflight.txt" 2>&1
}

print_input_summary() {
  printf 'setup_input_actions=%s\n' "${SETUP_INPUT_ACTIONS:-<unset>}"
  printf 'setup_input_marker=%s\n' "${SETUP_INPUT_MARKER:-<probe-default>}"
  printf 'setup_input_fire_delay_ms=%s\n' "${SETUP_INPUT_FIRE_DELAY_MS:-<unset>}"
  printf 'setup_input_ramfb_delay_ms=%s\n' "${SETUP_INPUT_RAMFB_DELAY_MS:-<probe-default>}"
  printf 'setup_input2_actions=%s\n' "${SETUP_INPUT2_ACTIONS:-<unset>}"
  printf 'setup_input2_marker=%s\n' "${SETUP_INPUT2_MARKER:-<probe-default>}"
  printf 'setup_input2_fire_delay_ms=%s\n' "${SETUP_INPUT2_FIRE_DELAY_MS:-<unset>}"
  printf 'setup_input2_ramfb_delay_ms=%s\n' "${SETUP_INPUT2_RAMFB_DELAY_MS:-<probe-default>}"
  printf 'setup_input3_actions=%s\n' "${SETUP_INPUT3_ACTIONS:-<unset>}"
  printf 'setup_input3_marker=%s\n' "${SETUP_INPUT3_MARKER:-<probe-default>}"
  printf 'setup_input3_fire_delay_ms=%s\n' "${SETUP_INPUT3_FIRE_DELAY_MS:-<unset>}"
  printf 'setup_input3_ramfb_delay_ms=%s\n' "${SETUP_INPUT3_RAMFB_DELAY_MS:-<probe-default>}"
  printf 'pointer_input_actions=%s\n' "${POINTER_INPUT_ACTIONS:-<unset>}"
  printf 'pointer_input_marker=%s\n' "${POINTER_INPUT_MARKER:-<probe-default>}"
  printf 'pointer_input_fire_delay_ms=%s\n' "${POINTER_INPUT_FIRE_DELAY_MS:-<unset>}"
  printf 'pointer_input_ramfb_delay_ms=%s\n' "${POINTER_INPUT_RAMFB_DELAY_MS:-<probe-default>}"
}

print_media_stat() {
  printf '%s:\n' "$1"
  ls -lh "$2"
  stat -f 'size=%z blocks=%b block_size=%k mtime=%Sm' "$2"
  du -h "$2"
}

virtio_gpu_trace_path() {
  printf '%s\n' "${VIRTIO_GPU_TRACE_JSONL:-$EVIDENCE_DIR/virtio-gpu.jsonl}"
}

build_and_sign_probe_if_needed() {
  [[ "$SKIP_BUILD" != "1" ]] || return 0
  {
    printf '\ncargo_build:\n'
    if [[ "$BUILD_PROFILE" == "release" ]]; then
      if [[ "$VIRTIO_GPU_3D" == "1" ]]; then
        cargo build --release -p bridgevm-hvf --features venus --example hvf_gic_boot_probe
      else
        cargo build --release -p bridgevm-hvf --example hvf_gic_boot_probe
      fi
    else
      if [[ "$VIRTIO_GPU_3D" == "1" ]]; then
        cargo build -p bridgevm-hvf --features venus --example hvf_gic_boot_probe
      else
        cargo build -p bridgevm-hvf --example hvf_gic_boot_probe
      fi
    fi
    printf '\ncodesign_force:\n'
    codesign --sign - --entitlements apps/macos/HvfRunner.entitlements --force "$BIN"
  } >> "$EVIDENCE_DIR/preflight.txt" 2>&1
}

verify_probe_build_capabilities() {
  [[ "${VIRTIO_GPU_3D:-0}" == "1" ]] || return 0

  local report="$EVIDENCE_DIR/probe-build-capabilities.txt"
  local output
  local probe_status
  local status=0
  set +e
  output="$(BRIDGEVM_PROBE_PRINT_CAPABILITIES=1 "$BIN" 2>&1)"
  probe_status="$?"
  set -e
  if [[ "$probe_status" != "0" ]] || ! grep -Fqx 'virtio_gpu_3d_compiled=true' <<<"$output"; then
    status=1
  fi
  {
    date -u
    printf 'binary=%s\n' "$BIN"
    printf '%s\n' "$output"
    printf 'probe_status=%s\n' "$probe_status"
    printf 'virtio_gpu_3d_required=true\n'
    printf 'status=%s\n' "$status"
  } > "$report"
  if [[ "$status" != "0" && "${RUN_STATUS:-0}" == "0" ]]; then
    RUN_STATUS="$status"
  fi
  [[ "$status" == "0" ]]
}

build_installed_boot_env_args() {
  COMMON_ENV=(
    "BRIDGEVM_RAM_MIB=$RAM_MIB" 'BRIDGEVM_RAMFB=1'
    "BRIDGEVM_RAMFB_DUMP_DIR=$EVIDENCE_DIR/ramfb"
    "BRIDGEVM_RAMFB_SAMPLE_MS=$RAMFB_SAMPLES"
    "BRIDGEVM_AARCH64_UEFI_VARS=$VARS" 'BRIDGEVM_AARCH64_UEFI_VARS_WRITABLE=1'
    "BRIDGEVM_AARCH64_UEFI_CODE=$FIRMWARE_CODE"
    "BRIDGEVM_BOOT_PROBE_WATCHDOG_MS=$WATCHDOG_MS"
    "BRIDGEVM_BOOT_PROBE_MAX_REBOOTS=$MAX_REBOOTS"
    'BRIDGEVM_RECENT_NVME_COMMANDS=4096' 'BRIDGEVM_RECENT_PCIE_MMIO=2048'
    'BRIDGEVM_RECENT_PCIE_PIO=1024'
  )
  if [[ "${WATCHDOG_DISABLED:-0}" == "1" ]]; then
    COMMON_ENV+=('BRIDGEVM_BOOT_PROBE_WATCHDOG_DISABLED=1')
  fi
  # Per-interrupt MSIX/SPI tracing is opt-in: always-on it emitted 50k+ lines
  # per run and drowned the BVAGENT evidence (service-mode logs must stay
  # greppable). Use --trace-irq when debugging interrupt delivery.
  if [[ "${TRACE_IRQ:-0}" == "1" ]]; then
    COMMON_ENV+=('BRIDGEVM_TRACE_MSIX=1' 'BRIDGEVM_TRACE_SPI=1')
  fi
  if [[ "${DAILY:-0}" == "1" ]]; then
    COMMON_ENV+=('BRIDGEVM_XHCI_REPORT_INTERVAL_MS=30')
  fi
  if [[ -n "$PLACEHOLDER_NSID1" ]]; then
    DISK_ENV=("BRIDGEVM_NVME_DISK=$PLACEHOLDER_NSID1" "BRIDGEVM_NVME_DISK2=$TARGET" 'BRIDGEVM_NVME_DISK2_WRITABLE=1')
  else
    DISK_ENV=("BRIDGEVM_NVME_DISK=$TARGET" 'BRIDGEVM_NVME_DISK_WRITABLE=1')
  fi
  ENV_ARGS=("${COMMON_ENV[@]}" "${DISK_ENV[@]}")
  if [[ -n "$SMP_CPUS" ]]; then
    ENV_ARGS+=("BRIDGEVM_SMP_CPUS=$SMP_CPUS")
  fi
  if [[ "$BOOT_TIMER" == "1" ]]; then
    ENV_ARGS+=('BRIDGEVM_BOOT_TIMER=1')
    [[ -z "$BOOT_TIMER_RAMFB_MS" ]] || ENV_ARGS+=("BRIDGEVM_BOOT_TIMER_RAMFB_MS=$BOOT_TIMER_RAMFB_MS")
    [[ -z "$BOOT_TIMER_DESKTOP_CHECKSUM64" ]] || ENV_ARGS+=("BRIDGEVM_BOOT_TIMER_DESKTOP_CHECKSUM64=$BOOT_TIMER_DESKTOP_CHECKSUM64")
    if [[ "$BOOT_TIMER_DESKTOP_AGENT" == "1" ]]; then
      ENV_ARGS+=('BRIDGEVM_BOOT_TIMER_DESKTOP_AGENT=1')
    fi
  fi
  if [[ "$BOOT_TIMER_DESKTOP_AGENT" == "1" || "$SHUTDOWN_AFTER_AGENT_READY" == "1" || -n "$HOST_PAUSE_RESUME_PROOF_MS" || -n "$AGENT_SERVICE_CONTROL" ]]; then
    ENV_ARGS+=('BRIDGEVM_VIRTIO_CONSOLE=1')
  fi
  if [[ "$SHUTDOWN_AFTER_AGENT_READY" == "1" ]]; then
    ENV_ARGS+=(
      'BRIDGEVM_VIRTIO_CONSOLE_TEST=1'
      'BRIDGEVM_VIRTIO_CONSOLE_TEST_PERIODIC=1'
      'BRIDGEVM_VIRTIO_CONSOLE_CMDS=shutdown.exe /p /f'
      "BRIDGEVM_VIRTIO_CONSOLE_TEST_TIMEOUT_MS=$WATCHDOG_MS"
    )
  fi
  if [[ -n "$HOST_PAUSE_RESUME_PROOF_MS" ]]; then
    ENV_ARGS+=(
      'BRIDGEVM_VIRTIO_CONSOLE_TEST=1'
      'BRIDGEVM_VIRTIO_CONSOLE_TEST_PERIODIC=1'
      'BRIDGEVM_VIRTIO_CONSOLE_CMDS=ver'
      "BRIDGEVM_VIRTIO_CONSOLE_TEST_TIMEOUT_MS=$WATCHDOG_MS"
      'BRIDGEVM_VIRTIO_CONSOLE_SERVICE=1'
      "BRIDGEVM_VIRTIO_CONSOLE_CTL=$(host_pause_resume_control_path)"
    )
  fi
  if [[ -n "$AGENT_SERVICE_CONTROL" ]]; then
    ENV_ARGS+=(
      'BRIDGEVM_VIRTIO_CONSOLE_TEST=1'
      'BRIDGEVM_VIRTIO_CONSOLE_TEST_PERIODIC=1'
      "BRIDGEVM_VIRTIO_CONSOLE_CMDS=$AGENT_SERVICE_COMMAND"
      "BRIDGEVM_VIRTIO_CONSOLE_TEST_TIMEOUT_MS=$WATCHDOG_MS"
      'BRIDGEVM_VIRTIO_CONSOLE_SERVICE=1'
      "BRIDGEVM_VIRTIO_CONSOLE_CTL=$AGENT_SERVICE_CONTROL"
    )
    [[ "$AGENT_CLIPBOARD_SYNC" == "1" ]] && ENV_ARGS+=('BRIDGEVM_VIRTIO_CONSOLE_CLIPSYNC=1')
    if [[ -n "$AGENT_SHARE_HOST" ]]; then
      ENV_ARGS+=(
        "BRIDGEVM_VIRTIO_CONSOLE_SHARE=$AGENT_SHARE_HOST::$AGENT_SHARE_GUEST"
        "BRIDGEVM_VIRTIO_CONSOLE_SHARE_MS=$AGENT_SHARE_MS"
        "BRIDGEVM_VIRTIO_CONSOLE_SHARE_MAX_KB=$AGENT_SHARE_MAX_KB"
      )
    fi
  fi
  append_input_env_args
  if [[ "$ENABLE_XHCI" != "1" ]]; then
    ENV_ARGS=('BRIDGEVM_DISABLE_XHCI=1' "${ENV_ARGS[@]}")
  fi
  if [[ "${VIRTIO_NET:-0}" == "1" ]]; then
    ENV_ARGS+=('BRIDGEVM_VIRTIO_NET=1' 'BRIDGEVM_VIRTIO_NET_BACKEND=nat')
  fi
  if [[ "${NVME_BUFFERED_IO:-0}" == "1" ]]; then
    ENV_ARGS+=('BRIDGEVM_NVME_BUFFERED_IO=1')
  fi
  if [[ "${VIRTIO_GPU_3D:-0}" == "1" ]]; then
    ENV_ARGS+=(
      'BRIDGEVM_VIRTIO_GPU=1'
      'BRIDGEVM_VIRTIO_GPU_3D=1'
      "BRIDGEVM_VIRTIO_GPU_3D_PROTOCOL=$(virtio_gpu_3d_runtime_protocol)"
    )
    if [[ -n "${VIRTIO_GPU_PCI_DEVICE_ID:-}" ]]; then
      ENV_ARGS+=("BRIDGEVM_VIRTIO_GPU_PCI_DEVICE_ID=0x$VIRTIO_GPU_PCI_DEVICE_ID")
    else
      ENV_ARGS+=('BRIDGEVM_VIRTIO_GPU_3D_BIND_ID=1')
    fi
    ENV_ARGS+=("BRIDGEVM_VIRTIO_GPU_TRACE_JSONL=${VIRTIO_GPU_TRACE_JSONL:-$EVIDENCE_DIR/virtio-gpu.jsonl}")
    if [[ -n "${GPU_TRACE_SUBMIT_PREFIX:-}" ]]; then
      ENV_ARGS+=("BRIDGEVM_VIRTIO_GPU_TRACE_SUBMIT_PREFIX=$GPU_TRACE_SUBMIT_PREFIX")
    fi
  fi
  if [[ -n "${DISPLAY_EXPORT_PPM:-}" ]]; then
    ENV_ARGS+=(
      "BRIDGEVM_DISPLAY_EXPORT_PPM=$DISPLAY_EXPORT_PPM"
      "BRIDGEVM_DISPLAY_EXPORT_MS=$DISPLAY_EXPORT_MS"
      "BRIDGEVM_VIRTIO_GPU_SCANOUT_READBACK_MS=$DISPLAY_EXPORT_MS"
    )
  fi
  if [[ -n "${DISPLAY_EXPORT_FB:-}" ]]; then
    # Device-inline shared-framebuffer export (no export thread; publish runs on
    # the vCPU thread at RESOURCE_FLUSH). READBACK_MS=0 removes the artificial FPS
    # cap so the display tracks the guest present rate (60-120fps, no limit).
    ENV_ARGS+=(
      "BRIDGEVM_DISPLAY_EXPORT_FB=$DISPLAY_EXPORT_FB"
      "BRIDGEVM_VIRTIO_GPU_SCANOUT_READBACK_MS=0"
    )
  fi
  if [[ -n "${INPUT_CONTROL:-}" ]]; then
    ENV_ARGS+=("BRIDGEVM_INPUT_CONTROL=$INPUT_CONTROL")
  fi
  printf '%s\n' "${ENV_ARGS[@]}"
}

append_input_env_args() {
  append_optional_input_env SETUP_INPUT "$SETUP_INPUT_ACTIONS" "$SETUP_INPUT_MARKER" "$SETUP_INPUT_FIRE_DELAY_MS" "$SETUP_INPUT_RAMFB_DELAY_MS"
  append_optional_input_env SETUP_INPUT2 "$SETUP_INPUT2_ACTIONS" "$SETUP_INPUT2_MARKER" "$SETUP_INPUT2_FIRE_DELAY_MS" "$SETUP_INPUT2_RAMFB_DELAY_MS"
  append_optional_input_env SETUP_INPUT3 "$SETUP_INPUT3_ACTIONS" "$SETUP_INPUT3_MARKER" "$SETUP_INPUT3_FIRE_DELAY_MS" "$SETUP_INPUT3_RAMFB_DELAY_MS"
  if [[ -n "$POINTER_INPUT_ACTIONS" ]]; then
    ENV_ARGS+=("BRIDGEVM_XHCI_POINTER_INPUT_ACTIONS=$POINTER_INPUT_ACTIONS")
    [[ -z "$POINTER_INPUT_MARKER" ]] || ENV_ARGS+=("BRIDGEVM_XHCI_POINTER_INPUT_SERIAL_MARKER=$POINTER_INPUT_MARKER")
    [[ -z "$POINTER_INPUT_FIRE_DELAY_MS" ]] || ENV_ARGS+=("BRIDGEVM_XHCI_POINTER_INPUT_FIRE_DELAY_MS=$POINTER_INPUT_FIRE_DELAY_MS")
    [[ -z "$POINTER_INPUT_RAMFB_DELAY_MS" ]] || ENV_ARGS+=("BRIDGEVM_XHCI_POINTER_INPUT_RAMFB_DELAY_MS=$POINTER_INPUT_RAMFB_DELAY_MS")
  fi
}

append_optional_input_env() {
  [[ -n "$2" ]] || return 0
  ENV_ARGS+=("BRIDGEVM_XHCI_${1}_ACTIONS=$2")
  [[ -z "$3" ]] || ENV_ARGS+=("BRIDGEVM_XHCI_${1}_SERIAL_MARKER=$3")
  [[ -z "$4" ]] || ENV_ARGS+=("BRIDGEVM_XHCI_${1}_FIRE_DELAY_MS=$4")
  [[ -z "$5" ]] || ENV_ARGS+=("BRIDGEVM_XHCI_${1}_RAMFB_DELAY_MS=$5")
}

write_probe_command_env() {
  {
    printf '\nentitlements:\n'
    codesign -d --entitlements - "$BIN"
    printf '\nentitlement_grep:\n'
    codesign -d --entitlements - "$BIN" 2>&1 | grep -n 'com.apple.security.hypervisor'
    printf '\ncommand_env:\n'
    build_installed_boot_env_args
  } >> "$EVIDENCE_DIR/preflight.txt" 2>&1
}

write_p3_gpu_readiness() {
  [[ "${VIRTIO_GPU_3D:-0}" == "1" ]] || return 0
  [[ -n "${VIOGPU3D_DIR:-}" || "${REQUIRE_VIOGPU3D_READINESS:-0}" == "1" ]] || return 0

  local readiness
  local -a args
  local status
  readiness="$EVIDENCE_DIR/p3-gpu-readiness.txt"
  args=("$ROOT/scripts/check-hvf-windows-p3-gpu-readiness.sh")
  if [[ -n "${VIOGPU3D_DIR:-}" ]]; then
    args+=(--driver-dir "$VIOGPU3D_DIR")
    args+=(--manifest "$EVIDENCE_DIR/viogpu3d-package-manifest.txt")
  fi
  args+=(--pci-device-id "${VIRTIO_GPU_PCI_DEVICE_ID:-10F7}")
  if [[ "${REQUIRE_VIOGPU3D_READINESS:-0}" == "1" ]]; then
    args+=(--require-driver-package)
  fi

  {
    date -u
    printf 'command=%q' "${args[0]}"
    local arg
    for arg in "${args[@]:1}"; do
      printf ' %q' "$arg"
    done
    printf '\n'
  } > "$readiness"

  set +e
  BRIDGEVM_VIRTIO_GPU_3D_PROTOCOL="$(virtio_gpu_3d_runtime_protocol)" \
    "${args[@]}" >> "$readiness" 2>&1
  status="$?"
  set -e

  printf 'status=%s\n' "$status" >> "$readiness"
  if [[ "$status" != "0" && "${RUN_STATUS:-0}" == "0" ]]; then
    RUN_STATUS="$status"
  fi
  [[ "$status" == "0" ]]
}

host_pause_resume_control_path() {
  printf '%s/host-pause-resume-control.txt\n' "$EVIDENCE_DIR"
}

host_pause_resume_observation_path() {
  printf '%s/host-pause-resume-observation.txt\n' "$EVIDENCE_DIR"
}

probe_log_match_count() {
  local pattern="$1"
  local count
  count="$(grep -cE "$pattern" "$EVIDENCE_DIR/run.log" 2>/dev/null || true)"
  printf '%s\n' "${count:-0}"
}

wait_for_probe_log_count() {
  local pattern="$1"
  local expected="$2"
  local timeout_ms="${WATCHDOG_MS:-900000}"
  local timeout_seconds=$(( (10#$timeout_ms + 999) / 1000 ))
  local deadline=$((SECONDS + timeout_seconds))
  local count

  while (( SECONDS <= deadline )); do
    count="$(probe_log_match_count "$pattern")"
    if (( 10#$count >= expected )); then
      return 0
    fi
    if ! kill -0 "$PROBE_PID" 2>/dev/null; then
      return 1
    fi
    sleep 0.1
  done
  return 1
}

fail_host_pause_resume_control() {
  local reason="$1"
  local observation
  observation="$(host_pause_resume_observation_path)"
  kill -CONT "$PROBE_PID" 2>/dev/null || true
  {
    printf 'failure_reason=%s\n' "$reason"
    printf 'control_status=1\n'
  } >> "$observation"
  return 1
}

drive_host_pause_resume_proof() {
  local control
  local observation
  local pause_seconds
  local state=""
  local paused_start_bytes
  local paused_end_bytes
  local initial_ver_count
  local resumed_ver_target
  local stable="false"
  local state_wait

  control="$(host_pause_resume_control_path)"
  observation="$(host_pause_resume_observation_path)"
  pause_seconds="$(printf '%d.%03d' \
    "$((10#$HOST_PAUSE_RESUME_PROOF_MS / 1000))" \
    "$((10#$HOST_PAUSE_RESUME_PROOF_MS % 1000))")"

  {
    date -u
    printf 'configured_pause_ms=%s\n' "$HOST_PAUSE_RESUME_PROOF_MS"
    printf 'probe_pid=%s\n' "$PROBE_PID"
    printf 'control_path=%s\n' "$control"
  } > "$observation"

  if ! wait_for_probe_log_count '^BVAGENT SERVICE start' 1; then
    fail_host_pause_resume_control service_ready_timeout
    return 1
  fi
  printf 'service_ready=true\n' >> "$observation"

  if ! kill -STOP "$PROBE_PID" 2>/dev/null; then
    fail_host_pause_resume_control sigstop_failed
    return 1
  fi
  for state_wait in $(seq 1 20); do
    state="$(ps -o state= -p "$PROBE_PID" 2>/dev/null | tr -d '[:space:]')"
    [[ "$state" == T* ]] && break
    sleep 0.05
  done
  if [[ "$state" != T* ]]; then
    fail_host_pause_resume_control process_did_not_stop
    return 1
  fi

  paused_start_bytes="$(stat -f %z "$EVIDENCE_DIR/run.log")"
  sleep "$pause_seconds"
  state="$(ps -o state= -p "$PROBE_PID" 2>/dev/null | tr -d '[:space:]')"
  paused_end_bytes="$(stat -f %z "$EVIDENCE_DIR/run.log")"
  [[ "$paused_start_bytes" == "$paused_end_bytes" ]] && stable="true"
  {
    printf 'during_state=%s\n' "$state"
    printf 'paused_start_log_bytes=%s\n' "$paused_start_bytes"
    printf 'paused_end_log_bytes=%s\n' "$paused_end_bytes"
    printf 'log_stable_while_stopped=%s\n' "$stable"
  } >> "$observation"
  if [[ "$state" != T* || "$stable" != "true" ]]; then
    fail_host_pause_resume_control pause_observation_failed
    return 1
  fi

  if ! kill -CONT "$PROBE_PID" 2>/dev/null; then
    fail_host_pause_resume_control sigcont_failed
    return 1
  fi
  printf 'continue_signal_sent=true\n' >> "$observation"

  initial_ver_count="$(probe_log_match_count '^BVAGENT CMD ver exit=0')"
  resumed_ver_target=$((10#$initial_ver_count + 1))
  printf 'ver\n' >> "$control"
  printf 'post_resume_command_sent=true\n' >> "$observation"
  if ! wait_for_probe_log_count '^BVAGENT CMD ver exit=0' "$resumed_ver_target"; then
    fail_host_pause_resume_control post_resume_agent_timeout
    return 1
  fi
  printf 'post_resume_command_ok=true\n' >> "$observation"

  printf 'shutdown.exe /p /f\n' >> "$control"
  {
    printf 'shutdown_command_sent=true\n'
    printf 'control_status=0\n'
  } >> "$observation"
}

prepare_virtio_gpu_trace() {
  [[ "${VIRTIO_GPU_3D:-0}" == "1" ]] || return 0
  local trace
  trace="$(virtio_gpu_trace_path)"
  install -d "$(dirname "$trace")"
  # A trace is proof for exactly one probe generation. Appending would allow
  # stale P3 success events from an earlier boot to satisfy the current gate.
  : > "$trace"
}

run_probe_process() {
  local name
  local -a env_command=(env)
  # An installed-boot run is a closed, auditable configuration boundary.
  # Remove every inherited BridgeVM probe knob, then apply only ENV_ARGS built
  # from this wrapper's validated CLI. This prevents an old developer shell
  # from attaching a second writable disk, changing PCI topology, injecting
  # guest input, or enabling agent share/clipboard commands behind the
  # recorded policy.
  while IFS= read -r name; do
    case "$name" in
      BRIDGEVM_*) env_command+=(-u "$name") ;;
    esac
  done < <(compgen -e)
  HOST_PAUSE_RESUME_CONTROL_STATUS=0
  if [[ -n "${HOST_PAUSE_RESUME_PROOF_MS:-}" ]]; then
    : > "$(host_pause_resume_control_path)"
  fi
  prepare_virtio_gpu_trace
  set +e
  "${env_command[@]}" "${ENV_ARGS[@]}" "$BIN" > "$EVIDENCE_DIR/run.log" 2>&1 &
  PROBE_PID="$!"
  if [[ -n "${HOST_PAUSE_RESUME_PROOF_MS:-}" ]]; then
    if ! drive_host_pause_resume_proof; then
      HOST_PAUSE_RESUME_CONTROL_STATUS=1
      terminate_owned_probe
    fi
  fi
  wait "$PROBE_PID"
  RUN_STATUS="$?"
  PROBE_PID=""
  set -e
}

write_host_pause_resume_gate() {
  [[ -n "$HOST_PAUSE_RESUME_PROOF_MS" ]] || return 0

  local observation
  local service_ready="false"
  local stopped="false"
  local stable="false"
  local continued="false"
  local agent_round_trip="false"
  local guest_system_off="false"
  local nvme_writeback="false"
  local probe_status="$RUN_STATUS"
  local status="0"
  observation="$(host_pause_resume_observation_path)"

  [[ -f "$observation" ]] && grep -Eq '^service_ready=true$' "$observation" && service_ready="true"
  [[ -f "$observation" ]] && grep -Eq '^during_state=T' "$observation" && stopped="true"
  [[ -f "$observation" ]] && grep -Eq '^log_stable_while_stopped=true$' "$observation" && stable="true"
  [[ -f "$observation" ]] && grep -Eq '^continue_signal_sent=true$' "$observation" && continued="true"
  [[ -f "$observation" ]] && grep -Eq '^post_resume_command_ok=true$' "$observation" && agent_round_trip="true"
  grep -Eq '^stop: PSCI .*\(system off\)' "$EVIDENCE_DIR/run.log" && guest_system_off="true"
  grep -Eq '^NVMe (second namespace )?disk written back:' "$EVIDENCE_DIR/run.log" && nvme_writeback="true"

  if [[ "${HOST_PAUSE_RESUME_CONTROL_STATUS:-1}" != "0" || "$probe_status" != "0" || \
        "$service_ready" != "true" || "$stopped" != "true" || "$stable" != "true" || \
        "$continued" != "true" || "$agent_round_trip" != "true" || \
        "$guest_system_off" != "true" || "$nvme_writeback" != "true" ]]; then
    status="1"
  fi

  {
    printf 'scope=process-resident-host-pause-resume\n'
    printf 'disk_backed_suspend=false\n'
    printf 'configured_pause_ms=%s\n' "$HOST_PAUSE_RESUME_PROOF_MS"
    printf 'service_ready=%s\n' "$service_ready"
    printf 'process_stopped=%s\n' "$stopped"
    printf 'log_stable_while_stopped=%s\n' "$stable"
    printf 'continue_signal_sent=%s\n' "$continued"
    printf 'post_resume_agent_round_trip=%s\n' "$agent_round_trip"
    printf 'guest_system_off=%s\n' "$guest_system_off"
    printf 'nvme_writeback=%s\n' "$nvme_writeback"
    printf 'probe_status=%s\n' "$probe_status"
    printf 'status=%s\n' "$status"
  } > "$EVIDENCE_DIR/host-pause-resume-gate.txt"

  if [[ "$status" != "0" && "$RUN_STATUS" == "0" ]]; then
    RUN_STATUS="$status"
  fi
}

write_agent_shutdown_gate() {
  [[ "$SHUTDOWN_AFTER_AGENT_READY" == "1" ]] || return 0

  local ready="false"
  local system_off="false"
  local status="0"
  if grep -Eq '^BVAGENT (READY|PONG \(proactive\))' "$EVIDENCE_DIR/run.log"; then
    ready="true"
  fi
  if grep -Eq 'stop: PSCI .*\(system off\)' "$EVIDENCE_DIR/run.log"; then
    system_off="true"
  fi
  if [[ "$ready" != "true" || "$system_off" != "true" ]]; then
    status="1"
  fi

  {
    printf 'configured_command=shutdown.exe /p /f\n'
    printf 'agent_handshake=%s\n' "$ready"
    printf 'guest_system_off=%s\n' "$system_off"
    printf 'status=%s\n' "$status"
  } > "$EVIDENCE_DIR/agent-shutdown-gate.txt"

  if [[ "$status" != "0" && "$RUN_STATUS" == "0" ]]; then
    RUN_STATUS="$status"
  fi
}

write_agent_service_gate() {
  [[ -n "$AGENT_SERVICE_CONTROL" ]] || return 0

  local ready="false"
  local initial_command_exit_zero="false"
  local initial_command_complete="false"
  local service_started="false"
  local guest_system_off="false"
  local nvme_writeback="false"
  local probe_status="$RUN_STATUS"
  local status="0"
  grep -Eq '^BVAGENT (READY|PONG \(proactive\))' "$EVIDENCE_DIR/run.log" && ready="true"
  grep -Fq "BVAGENT CMD $AGENT_SERVICE_COMMAND exit=0" "$EVIDENCE_DIR/run.log" && initial_command_exit_zero="true"
  grep -Fq "BVAGENT END $AGENT_SERVICE_COMMAND" "$EVIDENCE_DIR/run.log" && initial_command_complete="true"
  grep -Eq '^BVAGENT SERVICE start' "$EVIDENCE_DIR/run.log" && service_started="true"
  grep -Eq '^stop: PSCI .*\(system off\)' "$EVIDENCE_DIR/run.log" && guest_system_off="true"
  grep -Eq '^NVMe (second namespace )?disk written back:' "$EVIDENCE_DIR/run.log" && nvme_writeback="true"

  if [[ "$probe_status" != "0" || "$ready" != "true" || \
        "$initial_command_exit_zero" != "true" || "$initial_command_complete" != "true" || \
        "$service_started" != "true" || "$guest_system_off" != "true" || \
        "$nvme_writeback" != "true" ]]; then
    status="1"
  fi

  {
    printf 'configured_command=%s\n' "$AGENT_SERVICE_COMMAND"
    printf 'control_path=%s\n' "$AGENT_SERVICE_CONTROL"
    printf 'agent_handshake=%s\n' "$ready"
    printf 'initial_command_exit_zero=%s\n' "$initial_command_exit_zero"
    printf 'initial_command_complete=%s\n' "$initial_command_complete"
    printf 'service_started=%s\n' "$service_started"
    printf 'guest_system_off=%s\n' "$guest_system_off"
    printf 'nvme_writeback=%s\n' "$nvme_writeback"
    printf 'probe_status=%s\n' "$probe_status"
    printf 'status=%s\n' "$status"
  } > "$EVIDENCE_DIR/agent-service-gate.txt"

  if [[ "$status" != "0" && "$RUN_STATUS" == "0" ]]; then
    RUN_STATUS="$status"
  fi
}

write_virtio_gpu_trace_report() {
  [[ "${VIRTIO_GPU_3D:-0}" == "1" ]] || return 0

  local trace
  local report
  local gate
  local status
  trace="$(virtio_gpu_trace_path)"
  report="$EVIDENCE_DIR/virtio-gpu-trace-report.txt"
  gate="$EVIDENCE_DIR/virtio-gpu-trace-gate.txt"

  {
    date -u
    printf 'trace=%s\n' "$trace"
    printf 'gpu_trace_protocol=%s\n' "$GPU_TRACE_PROTOCOL"
    printf 'require_gpu_trace_gate=%s\n' "$REQUIRE_GPU_TRACE_GATE"
  } > "$report"

  if [[ ! -s "$trace" ]]; then
    {
      printf 'Trace missing or empty: %s\n' "$trace"
      printf 'P3 Windows 3D trace gate: FAIL\n'
      printf 'Blocker: missing virtio-gpu JSONL trace\n'
    } >> "$report"
    status=1
  else
    local -a args=(
      cargo run -q -p bridgevm-cli --
      hvf virtio-gpu-trace-report
      --trace "$trace"
      --protocol "$GPU_TRACE_PROTOCOL"
    )
    if [[ "$REQUIRE_GPU_TRACE_GATE" == "1" ]]; then
      args+=(--require-p3-gate)
    fi
    set +e
    "${args[@]}" >> "$report" 2>&1
    status="$?"
    set -e
  fi

  {
    printf 'trace=%s\n' "$trace"
    printf 'report=%s\n' "$report"
    printf 'protocol=%s\n' "$GPU_TRACE_PROTOCOL"
    printf 'required=%s\n' "$REQUIRE_GPU_TRACE_GATE"
    printf 'status=%s\n' "$status"
  } > "$gate"

  if [[ "$REQUIRE_GPU_TRACE_GATE" == "1" && "$status" != "0" && "${RUN_STATUS:-0}" == "0" ]]; then
    RUN_STATUS="$status"
  fi
}

write_installed_boot_target_stat() {
  {
    printf 'run_status=%s\n' "$RUN_STATUS"
    date -u
    if [[ "${VIRTIO_GPU_3D:-0}" == "1" ]]; then
      printf 'virtio_gpu_trace=%s\n' "$(virtio_gpu_trace_path)"
      printf 'probe_build_capabilities=%s\n' "$EVIDENCE_DIR/probe-build-capabilities.txt"
      printf 'virtio_gpu_trace_report=%s\n' "$EVIDENCE_DIR/virtio-gpu-trace-report.txt"
      printf 'virtio_gpu_trace_gate=%s\n' "$EVIDENCE_DIR/virtio-gpu-trace-gate.txt"
      printf 'p3_gpu_readiness=%s\n' "$EVIDENCE_DIR/p3-gpu-readiness.txt"
      printf 'viogpu3d_package_manifest=%s\n' "$EVIDENCE_DIR/viogpu3d-package-manifest.txt"
    fi
    if [[ "$SHUTDOWN_AFTER_AGENT_READY" == "1" ]]; then
      printf 'agent_shutdown_gate=%s\n' "$EVIDENCE_DIR/agent-shutdown-gate.txt"
    fi
    if [[ -n "$AGENT_SERVICE_CONTROL" ]]; then
      printf 'agent_service_gate=%s\n' "$EVIDENCE_DIR/agent-service-gate.txt"
    fi
    if [[ -n "$HOST_PAUSE_RESUME_PROOF_MS" ]]; then
      printf 'host_pause_resume_gate=%s\n' "$EVIDENCE_DIR/host-pause-resume-gate.txt"
      printf 'host_pause_resume_observation=%s\n' "$(host_pause_resume_observation_path)"
    fi
    print_media_stat after_target_stat "$TARGET"
    printf 'after_vars_stat:\n'
    ls -lh "$VARS"
    printf 'ramfb_files:\n'
    find "$EVIDENCE_DIR/ramfb" -maxdepth 1 -type f -print | sort
    printf 'run_log_summary_grep:\n'
    grep -En 'Windows|Boot Manager|UEFI|EFI|Bds|Boot####|NVMe|xHCI|qemu-xhci|HID|USB|PNP|BVAGENT|INTERNAL_POWER_ERROR|DRIVER_PNP_WATCHDOG|0x1D5|bugcheck|panic|HV_DENIED|hv_vm_create|watchdog|SYSTEM_RESET|SYSTEM_OFF|PSCI|storage target effect|exact_target_storage_evidence|target_effect_class' "$EVIDENCE_DIR/run.log" || true
  } > "$EVIDENCE_DIR/target-stat.txt" 2>&1
}

run_installed_boot_probe() {
  cd "$ROOT"
  install -d "$EVIDENCE_DIR/ramfb"
  if [[ "$VIRTIO_GPU_3D" == "1" ]]; then
    install -d "$(dirname "${VIRTIO_GPU_TRACE_JSONL:-$EVIDENCE_DIR/virtio-gpu.jsonl}")"
  fi
  BOOT_MODE="target-as-only-nvme"
  if [[ -n "$PLACEHOLDER_NSID1" ]]; then
    BOOT_MODE="placeholder-nsid1-target-as-nsid2"
  fi
  RUN_STATUS=0
  PROBE_PID=""
  if [[ "$BUILD_PROFILE" == "release" ]]; then
    BIN="target/release/examples/hvf_gic_boot_probe"
  else
    BIN="target/debug/examples/hvf_gic_boot_probe"
  fi
  trap cleanup EXIT
  write_installed_boot_preflight
  if ! write_p3_gpu_readiness; then
    write_installed_boot_target_stat
    return 0
  fi
  build_and_sign_probe_if_needed
  if ! verify_probe_build_capabilities; then
    write_installed_boot_target_stat
    return 0
  fi
  write_probe_command_env
  run_probe_process
  write_agent_shutdown_gate
  write_agent_service_gate
  write_host_pause_resume_gate
  write_virtio_gpu_trace_report
  write_installed_boot_target_stat
}
