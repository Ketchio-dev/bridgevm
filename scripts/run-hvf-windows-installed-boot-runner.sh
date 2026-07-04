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

build_and_sign_probe_if_needed() {
  [[ "$SKIP_BUILD" != "1" ]] || return 0
  {
    printf '\ncargo_build:\n'
    cargo build -p bridgevm-hvf --example hvf_gic_boot_probe
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
    'BRIDGEVM_RECENT_PCIE_PIO=1024' 'BRIDGEVM_TRACE_MSIX=1' 'BRIDGEVM_TRACE_SPI=1'
  )
  if [[ -n "$PLACEHOLDER_NSID1" ]]; then
    DISK_ENV=("BRIDGEVM_NVME_DISK=$PLACEHOLDER_NSID1" "BRIDGEVM_NVME_DISK2=$TARGET" 'BRIDGEVM_NVME_DISK2_WRITABLE=1')
  else
    DISK_ENV=("BRIDGEVM_NVME_DISK=$TARGET" 'BRIDGEVM_NVME_DISK_WRITABLE=1')
  fi
  ENV_ARGS=("${COMMON_ENV[@]}" "${DISK_ENV[@]}")
  append_input_env_args
  if [[ "$ENABLE_XHCI" != "1" ]]; then
    ENV_ARGS=('BRIDGEVM_DISABLE_XHCI=1' "${ENV_ARGS[@]}")
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

run_probe_process() {
  set +e
  env "${ENV_ARGS[@]}" "$BIN" > "$EVIDENCE_DIR/run.log" 2>&1 &
  PROBE_PID="$!"
  wait "$PROBE_PID"
  RUN_STATUS="$?"
  PROBE_PID=""
  set -e
}

write_installed_boot_target_stat() {
  {
    printf 'run_status=%s\n' "$RUN_STATUS"
    date -u
    print_media_stat after_target_stat "$TARGET"
    printf 'after_vars_stat:\n'
    ls -lh "$VARS"
    printf 'ramfb_files:\n'
    find "$EVIDENCE_DIR/ramfb" -maxdepth 1 -type f -print | sort
    printf 'run_log_summary_grep:\n'
    rg -n 'Windows|Boot Manager|UEFI|EFI|Bds|Boot####|NVMe|xHCI|qemu-xhci|HID|USB|PNP|INTERNAL_POWER_ERROR|DRIVER_PNP_WATCHDOG|0x1D5|bugcheck|panic|HV_DENIED|hv_vm_create|watchdog|SYSTEM_RESET|SYSTEM_OFF|PSCI|storage target effect|exact_target_storage_evidence|target_effect_class' "$EVIDENCE_DIR/run.log" || true
  } > "$EVIDENCE_DIR/target-stat.txt" 2>&1
}

run_installed_boot_probe() {
  cd "$ROOT"
  install -d "$EVIDENCE_DIR/ramfb"
  BOOT_MODE="target-as-only-nvme"
  if [[ -n "$PLACEHOLDER_NSID1" ]]; then
    BOOT_MODE="placeholder-nsid1-target-as-nsid2"
  fi
  RUN_STATUS=0
  PROBE_PID=""
  BIN="target/debug/examples/hvf_gic_boot_probe"
  trap cleanup EXIT
  write_installed_boot_preflight
  build_and_sign_probe_if_needed
  write_probe_command_env
  run_probe_process
  write_installed_boot_target_stat
}
