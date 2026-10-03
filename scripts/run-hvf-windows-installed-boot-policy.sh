print_installed_boot_policy() {
  local gpu_enabled_policy="<unset>"
  local gpu_bind_policy="<unset>"
  local gpu_trace_policy="<unset>"
  local gpu_3d_protocol="<unset>"
  local virtio_console_policy="<unset>"
  local console_test_policy="<unset>"
  local console_test_periodic_policy="<unset>"
  local console_commands_policy="<unset>"
  local console_timeout_policy="<unset>"
  local console_service_policy="<unset>"
  local console_control_policy="<unset>"
  local console_clipsync_policy="<unset>"
  local console_share_policy="<unset>"
  local console_share_ms_policy="<unset>"
  local console_share_max_kb_policy="<unset>"
  local title_manifests_policy="<unset>"
  if (( TITLE_MANIFEST_COUNT > 0 )); then
    title_manifests_policy="${TITLE_MANIFESTS[*]}"
  fi
  if [[ "$VIRTIO_GPU_3D" == "1" ]]; then
    gpu_enabled_policy="1"
    gpu_3d_protocol="$(virtio_gpu_3d_runtime_protocol)"
    if [[ -n "$VIRTIO_GPU_PCI_DEVICE_ID" ]]; then
      gpu_bind_policy="<unset> (explicit device id 0x$VIRTIO_GPU_PCI_DEVICE_ID)"
    else
      gpu_bind_policy="1"
    fi
    gpu_trace_policy="${VIRTIO_GPU_TRACE_JSONL:-$EVIDENCE_DIR/virtio-gpu.jsonl}"
  fi
  if [[ "$BOOT_TIMER_DESKTOP_AGENT" == "1" || "$SHUTDOWN_AFTER_AGENT_READY" == "1" || -n "$HOST_PAUSE_RESUME_PROOF_MS" || -n "$AGENT_SERVICE_CONTROL" ]]; then
    virtio_console_policy="1"
  fi
  if [[ "$SHUTDOWN_AFTER_AGENT_READY" == "1" ]]; then
    console_test_policy="1"
    console_test_periodic_policy="1"
    console_commands_policy="shutdown.exe /p /f"
    console_timeout_policy="$WATCHDOG_MS"
  fi
  if [[ -n "$HOST_PAUSE_RESUME_PROOF_MS" ]]; then
    console_test_policy="1"
    console_test_periodic_policy="1"
    console_commands_policy="ver"
    console_timeout_policy="$WATCHDOG_MS"
    console_service_policy="1"
    console_control_policy="$EVIDENCE_DIR/host-pause-resume-control.txt"
  fi
  if [[ -n "$AGENT_SERVICE_CONTROL" ]]; then
    console_test_policy="1"
    console_test_periodic_policy="1"
    console_commands_policy="$AGENT_SERVICE_COMMAND"
    console_timeout_policy="$WATCHDOG_MS"
    console_service_policy="1"
    console_control_policy="$AGENT_SERVICE_CONTROL"
    [[ "$AGENT_CLIPBOARD_SYNC" == "1" ]] && console_clipsync_policy="1"
    if [[ -n "$AGENT_SHARE_HOST" ]]; then
      console_share_policy="$AGENT_SHARE_HOST::$AGENT_SHARE_GUEST"
      console_share_ms_policy="$AGENT_SHARE_MS"
      console_share_max_kb_policy="$AGENT_SHARE_MAX_KB"
    fi
  fi
  printf '%s\n' \
    "$XHCI_POLICY" \
    "BRIDGEVM_AARCH64_UEFI_CODE=$FIRMWARE_CODE" \
    "DAILY_PRESET=$DAILY" \
    "BRIDGEVM_RAM_MIB=$RAM_MIB" \
    "BRIDGEVM_BOOT_PROBE_WATCHDOG_MS=$WATCHDOG_MS" \
    "BRIDGEVM_BOOT_PROBE_MAX_EXITS=$MAX_EXITS" \
    "BRIDGEVM_BOOT_PROBE_WATCHDOG_DISABLED=${WATCHDOG_DISABLED/0/<unset>}" \
    "BRIDGEVM_SMP_CPUS=${SMP_CPUS:-<unset> (probe default 1)}" \
    "BRIDGEVM_XHCI_REPORT_INTERVAL_MS=$([[ "$DAILY" == "1" ]] && printf '30' || printf '<probe-default 30>')" \
    "BRIDGEVM_BOOT_TIMER=${BOOT_TIMER/0/<unset>}" \
    "BRIDGEVM_BOOT_TIMER_RAMFB_MS=${BOOT_TIMER_RAMFB_MS:-<probe-default 1000>}" \
    "BRIDGEVM_BOOT_TIMER_DESKTOP_CHECKSUM64=${BOOT_TIMER_DESKTOP_CHECKSUM64:-<unset>}" \
    "BRIDGEVM_BOOT_TIMER_DESKTOP_AGENT=${BOOT_TIMER_DESKTOP_AGENT/0/<unset>}" \
    "SHUTDOWN_AFTER_AGENT_READY=$SHUTDOWN_AFTER_AGENT_READY" \
    "HOST_PAUSE_RESUME_PROOF_MS=${HOST_PAUSE_RESUME_PROOF_MS:-<unset>}" \
    "BRIDGEVM_VIRTIO_CONSOLE=$virtio_console_policy" \
    "BRIDGEVM_VIRTIO_CONSOLE_TEST=$console_test_policy" \
    "BRIDGEVM_VIRTIO_CONSOLE_TEST_PERIODIC=$console_test_periodic_policy" \
    "BRIDGEVM_VIRTIO_CONSOLE_CMDS=$console_commands_policy" \
    "BRIDGEVM_VIRTIO_CONSOLE_TEST_TIMEOUT_MS=$console_timeout_policy" \
    "BRIDGEVM_VIRTIO_CONSOLE_SERVICE=$console_service_policy" \
    "BRIDGEVM_VIRTIO_CONSOLE_CTL=$console_control_policy" \
    "BRIDGEVM_VIRTIO_CONSOLE_CLIPSYNC=$console_clipsync_policy" \
    "BRIDGEVM_VIRTIO_CONSOLE_SHARE=$console_share_policy" \
    "BRIDGEVM_VIRTIO_CONSOLE_SHARE_MS=$console_share_ms_policy" \
    "BRIDGEVM_VIRTIO_CONSOLE_SHARE_MAX_KB=$console_share_max_kb_policy" \
    "BRIDGEVM_NVME_BUFFERED_IO=${NVME_BUFFERED_IO/0/<unset>}" \
    "BRIDGEVM_VTPM_STATE_DIR=${VTPM_STATE_DIR:-<unset>}" \
    "BRIDGEVM_SWTPM_BIN=$([[ -n "$VTPM_STATE_DIR" ]] && printf '%s' "$SWTPM_BIN" || printf '<unset>')" \
    "BRIDGEVM_SWTPM_STATE_ENCRYPTION=$([[ "$SWTPM_KEY_STDIN" == "1" ]] && printf 'aes-256-cbc-etm/key-fd' || printf 'disabled')" \
    "BRIDGEVM_PERFORMANCE_RISK=$PERFORMANCE_RISK" \
    "BRIDGEVM_VIRTIO_GPU_DIRECT_RENDERER=$([[ "$PERFORMANCE_RISK" == "aggressive" ]] && printf '1' || printf '%s' "${BRIDGEVM_VIRTIO_GPU_DIRECT_RENDERER:-0}")" \
    "BRIDGEVM_VIRTIO_GPU_ASYNC_SCANOUT=$([[ "$PERFORMANCE_RISK" == "aggressive" ]] && printf '1' || printf '%s' "${BRIDGEVM_VIRTIO_GPU_ASYNC_SCANOUT:-0}")" \
    "BRIDGEVM_VIRTIO_GPU_IOSURFACE_SCANOUT=$([[ "$PERFORMANCE_RISK" == "aggressive" ]] && printf '1' || printf '%s' "${BRIDGEVM_VIRTIO_GPU_IOSURFACE_SCANOUT:-0}")" \
    "BRIDGEVM_VIRTIO_GPU=$gpu_enabled_policy" \
    "BRIDGEVM_VIRTIO_GPU_3D=$VIRTIO_GPU_3D" \
    "BRIDGEVM_VIRTIO_GPU_3D_PROTOCOL=$gpu_3d_protocol" \
    "BRIDGEVM_VIRTIO_GPU_3D_BIND_ID=$gpu_bind_policy" \
    "BRIDGEVM_VIRTIO_GPU_PCI_DEVICE_ID=${VIRTIO_GPU_PCI_DEVICE_ID:+0x$VIRTIO_GPU_PCI_DEVICE_ID}" \
    "BRIDGEVM_VIRTIO_GPU_TRACE_JSONL=$gpu_trace_policy" \
    "BRIDGEVM_GPU_TRACE_PROTOCOL=$GPU_TRACE_PROTOCOL" \
    "BRIDGEVM_REQUIRE_GPU_TRACE_GATE=$REQUIRE_GPU_TRACE_GATE" \
    "BRIDGEVM_VIOGPU3D_DIR=${VIOGPU3D_DIR:-<unset>}" \
    "BRIDGEVM_REQUIRE_VIOGPU3D_READINESS=$REQUIRE_VIOGPU3D_READINESS" \
    "BRIDGEVM_REQUIRE_REAL_TITLE_GATE=$REQUIRE_REAL_TITLE_GATE" \
    "BRIDGEVM_TITLE_MANIFESTS=$title_manifests_policy" \
    "BRIDGEVM_REQUIRE_TITLE_GATES=$REQUIRE_TITLE_GATES" \
    "GUEST_DISK_HARVEST=$GUEST_DISK_HARVEST" \
    "DIAGNOSTIC_OUTPUT_BOUNDS=$DIAGNOSTIC_OUTPUT_BOUNDS" \
    "BRIDGEVM_RAMFB_SAMPLE_MS=$RAMFB_SAMPLES" \
    "BRIDGEVM_XHCI_SETUP_INPUT_ACTIONS=${SETUP_INPUT_ACTIONS:-<unset>}" \
    "BRIDGEVM_XHCI_SETUP_INPUT_SERIAL_MARKER=${SETUP_INPUT_MARKER:-<probe-default>}" \
    "BRIDGEVM_XHCI_SETUP_INPUT_FIRE_DELAY_MS=${SETUP_INPUT_FIRE_DELAY_MS:-<unset>}" \
    "BRIDGEVM_XHCI_SETUP_INPUT_RAMFB_DELAY_MS=${SETUP_INPUT_RAMFB_DELAY_MS:-<probe-default>}" \
    "BRIDGEVM_XHCI_SETUP_INPUT2_ACTIONS=${SETUP_INPUT2_ACTIONS:-<unset>}" \
    "BRIDGEVM_XHCI_SETUP_INPUT2_SERIAL_MARKER=${SETUP_INPUT2_MARKER:-<probe-default>}" \
    "BRIDGEVM_XHCI_SETUP_INPUT2_FIRE_DELAY_MS=${SETUP_INPUT2_FIRE_DELAY_MS:-<unset>}" \
    "BRIDGEVM_XHCI_SETUP_INPUT2_RAMFB_DELAY_MS=${SETUP_INPUT2_RAMFB_DELAY_MS:-<probe-default>}" \
    "BRIDGEVM_XHCI_SETUP_INPUT3_ACTIONS=${SETUP_INPUT3_ACTIONS:-<unset>}" \
    "BRIDGEVM_XHCI_SETUP_INPUT3_SERIAL_MARKER=${SETUP_INPUT3_MARKER:-<probe-default>}" \
    "BRIDGEVM_XHCI_SETUP_INPUT3_FIRE_DELAY_MS=${SETUP_INPUT3_FIRE_DELAY_MS:-<unset>}" \
    "BRIDGEVM_XHCI_SETUP_INPUT3_RAMFB_DELAY_MS=${SETUP_INPUT3_RAMFB_DELAY_MS:-<probe-default>}" \
    "BRIDGEVM_XHCI_POINTER_INPUT_ACTIONS=${POINTER_INPUT_ACTIONS:-<unset>}" \
    "BRIDGEVM_XHCI_POINTER_INPUT_SERIAL_MARKER=${POINTER_INPUT_MARKER:-<probe-default>}" \
    "BRIDGEVM_XHCI_POINTER_INPUT_FIRE_DELAY_MS=${POINTER_INPUT_FIRE_DELAY_MS:-<unset>}" \
    "BRIDGEVM_XHCI_POINTER_INPUT_RAMFB_DELAY_MS=${POINTER_INPUT_RAMFB_DELAY_MS:-<probe-default>}" \
    "BUILD_PROFILE=$BUILD_PROFILE" \
    'BRIDGEVM_NVME_DISK_WRITABLE=1 when booting target as only NVMe' \
    'BRIDGEVM_NVME_DISK2_WRITABLE=1 when --placeholder-nsid1 is set' \
    "reason=$XHCI_REASON; C4/D1 boots the installed target without the installer disk and supports the proven NSID-2 target position"
}
