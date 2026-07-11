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
    printf 'evidence_dir=%s\n' "$EVIDENCE_DIR"
    printf 'build_profile=%s\n' "$BUILD_PROFILE"
    printf 'daily_preset=%s\n' "$DAILY"
    printf 'ram_mib=%s\n' "$RAM_MIB"
    printf 'watchdog_ms=%s\n' "$WATCHDOG_MS"
    printf 'smp_cpus=%s\n' "${SMP_CPUS:-<unset>}"
    printf 'xhci_report_interval_ms=%s\n' "$([[ "$DAILY" == "1" ]] && printf '30' || printf '<probe-default 30>')"
    printf 'boot_timer=%s\n' "$BOOT_TIMER"
    printf 'boot_timer_ramfb_ms=%s\n' "${BOOT_TIMER_RAMFB_MS:-<probe-default 1000>}"
    printf 'boot_timer_desktop_checksum64=%s\n' "${BOOT_TIMER_DESKTOP_CHECKSUM64:-<unset>}"
    printf 'boot_timer_desktop_agent=%s\n' "$BOOT_TIMER_DESKTOP_AGENT"
    printf 'shutdown_after_agent_ready=%s\n' "$SHUTDOWN_AFTER_AGENT_READY"
    printf 'virtio_console_test_periodic=%s\n' "$SHUTDOWN_AFTER_AGENT_READY"
    if [[ "$BOOT_TIMER_DESKTOP_AGENT" == "1" || "$SHUTDOWN_AFTER_AGENT_READY" == "1" ]]; then
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

build_installed_boot_env_args() {
  COMMON_ENV=(
    "BRIDGEVM_RAM_MIB=$RAM_MIB" 'BRIDGEVM_RAMFB=1'
    "BRIDGEVM_RAMFB_DUMP_DIR=$EVIDENCE_DIR/ramfb"
    "BRIDGEVM_RAMFB_SAMPLE_MS=$RAMFB_SAMPLES"
    "BRIDGEVM_AARCH64_UEFI_VARS=$VARS" 'BRIDGEVM_AARCH64_UEFI_VARS_WRITABLE=1'
    "BRIDGEVM_BOOT_PROBE_WATCHDOG_MS=$WATCHDOG_MS"
    "BRIDGEVM_BOOT_PROBE_MAX_REBOOTS=$MAX_REBOOTS"
    'BRIDGEVM_RECENT_NVME_COMMANDS=4096' 'BRIDGEVM_RECENT_PCIE_MMIO=2048'
    'BRIDGEVM_RECENT_PCIE_PIO=1024'
  )
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
  if [[ "$BOOT_TIMER_DESKTOP_AGENT" == "1" || "$SHUTDOWN_AFTER_AGENT_READY" == "1" ]]; then
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
  append_input_env_args
  if [[ "$ENABLE_XHCI" != "1" ]]; then
    ENV_ARGS=('BRIDGEVM_DISABLE_XHCI=1' "${ENV_ARGS[@]}")
  fi
  if [[ "${VIRTIO_NET:-0}" == "1" ]]; then
    ENV_ARGS+=('BRIDGEVM_VIRTIO_NET=1' 'BRIDGEVM_VIRTIO_NET_BACKEND=nat')
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
  set +e
  "${env_command[@]}" "${ENV_ARGS[@]}" "$BIN" > "$EVIDENCE_DIR/run.log" 2>&1 &
  PROBE_PID="$!"
  wait "$PROBE_PID"
  RUN_STATUS="$?"
  PROBE_PID=""
  set -e
}

write_agent_shutdown_gate() {
  [[ "$SHUTDOWN_AFTER_AGENT_READY" == "1" ]] || return 0

  local ready="false"
  local system_off="false"
  local status="0"
  if rg -q '^BVAGENT (READY|PONG \(proactive\))' "$EVIDENCE_DIR/run.log"; then
    ready="true"
  fi
  if rg -q 'stop: PSCI .*\(system off\)' "$EVIDENCE_DIR/run.log"; then
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
      printf 'virtio_gpu_trace_report=%s\n' "$EVIDENCE_DIR/virtio-gpu-trace-report.txt"
      printf 'virtio_gpu_trace_gate=%s\n' "$EVIDENCE_DIR/virtio-gpu-trace-gate.txt"
      printf 'p3_gpu_readiness=%s\n' "$EVIDENCE_DIR/p3-gpu-readiness.txt"
      printf 'viogpu3d_package_manifest=%s\n' "$EVIDENCE_DIR/viogpu3d-package-manifest.txt"
    fi
    if [[ "$SHUTDOWN_AFTER_AGENT_READY" == "1" ]]; then
      printf 'agent_shutdown_gate=%s\n' "$EVIDENCE_DIR/agent-shutdown-gate.txt"
    fi
    print_media_stat after_target_stat "$TARGET"
    printf 'after_vars_stat:\n'
    ls -lh "$VARS"
    printf 'ramfb_files:\n'
    find "$EVIDENCE_DIR/ramfb" -maxdepth 1 -type f -print | sort
    printf 'run_log_summary_grep:\n'
    rg -n 'Windows|Boot Manager|UEFI|EFI|Bds|Boot####|NVMe|xHCI|qemu-xhci|HID|USB|PNP|BVAGENT|INTERNAL_POWER_ERROR|DRIVER_PNP_WATCHDOG|0x1D5|bugcheck|panic|HV_DENIED|hv_vm_create|watchdog|SYSTEM_RESET|SYSTEM_OFF|PSCI|storage target effect|exact_target_storage_evidence|target_effect_class' "$EVIDENCE_DIR/run.log" || true
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
  write_probe_command_env
  run_probe_process
  write_agent_shutdown_gate
  write_virtio_gpu_trace_report
  write_installed_boot_target_stat
}
