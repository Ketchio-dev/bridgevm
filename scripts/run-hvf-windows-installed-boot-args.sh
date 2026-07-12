init_installed_boot_defaults() {
  TARGET=""
  PLACEHOLDER_NSID1=""
  VARS=""
  EVIDENCE_DIR=""
  WATCHDOG_MS="900000"
  WATCHDOG_DISABLED="0"
  MAX_REBOOTS="8"
  RAM_MIB="4096"
  RAMFB_SAMPLES="1000,5000,15000,30000,60000,90000,120000"
  ENABLE_XHCI="0"
  VIRTIO_NET="0"
  NVME_BUFFERED_IO="0"
  VIRTIO_GPU_3D="0"
  VIRTIO_GPU_PCI_DEVICE_ID=""
  VIRTIO_GPU_TRACE_JSONL=""
  GPU_TRACE_PROTOCOL="auto"
  REQUIRE_GPU_TRACE_GATE="0"
  VIOGPU3D_DIR=""
  REQUIRE_VIOGPU3D_READINESS="0"
  SETUP_INPUT_ACTIONS=""
  SETUP_INPUT_MARKER=""
  SETUP_INPUT_FIRE_DELAY_MS=""
  SETUP_INPUT_RAMFB_DELAY_MS=""
  SETUP_INPUT2_ACTIONS=""
  SETUP_INPUT2_MARKER=""
  SETUP_INPUT2_FIRE_DELAY_MS=""
  SETUP_INPUT2_RAMFB_DELAY_MS=""
  SETUP_INPUT3_ACTIONS=""
  SETUP_INPUT3_MARKER=""
  SETUP_INPUT3_FIRE_DELAY_MS=""
  SETUP_INPUT3_RAMFB_DELAY_MS=""
  POINTER_INPUT_ACTIONS=""
  POINTER_INPUT_MARKER=""
  POINTER_INPUT_FIRE_DELAY_MS=""
  POINTER_INPUT_RAMFB_DELAY_MS=""
  BUILD_PROFILE="debug"
  SKIP_BUILD="0"
  DAILY="0"
  RAM_MIB_EXPLICIT="0"
  WATCHDOG_MS_EXPLICIT="0"
  PRINT_POLICY="0"
  SMP_CPUS=""
  SMP_CPUS_EXPLICIT="0"
  BOOT_TIMER="0"
  BOOT_TIMER_RAMFB_MS=""
  BOOT_TIMER_DESKTOP_CHECKSUM64=""
  BOOT_TIMER_DESKTOP_AGENT="0"
  SHUTDOWN_AFTER_AGENT_READY="0"
  HOST_PAUSE_RESUME_PROOF_MS=""
  AGENT_SERVICE_CONTROL=""
  AGENT_SERVICE_COMMAND="whoami"
  AGENT_SERVICE_COMMAND_EXPLICIT="0"
  AGENT_CLIPBOARD_SYNC="0"
  AGENT_SHARE_HOST=""
  AGENT_SHARE_GUEST=""
  AGENT_SHARE_MS="2000"
  AGENT_SHARE_MS_EXPLICIT="0"
  AGENT_SHARE_MAX_KB="8192"
  AGENT_SHARE_MAX_KB_EXPLICIT="0"
  XHCI_POLICY=""
  XHCI_REASON=""
  TRACE_IRQ="0"
}

parse_installed_boot_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --target) [[ $# -ge 2 ]] || { usage; exit 2; }; TARGET="$2"; shift 2 ;;
      --placeholder-nsid1) [[ $# -ge 2 ]] || { usage; exit 2; }; PLACEHOLDER_NSID1="$2"; shift 2 ;;
      --vars) [[ $# -ge 2 ]] || { usage; exit 2; }; VARS="$2"; shift 2 ;;
      --evidence-dir) [[ $# -ge 2 ]] || { usage; exit 2; }; EVIDENCE_DIR="$2"; shift 2 ;;
      --watchdog-ms)
        [[ $# -ge 2 ]] || { usage; exit 2; }
        positive_integer "$2" || { echo "FAIL: --watchdog-ms requires a positive integer" >&2; exit 2; }
        WATCHDOG_MS="$2"; WATCHDOG_MS_EXPLICIT="1"; shift 2
        ;;
      --no-watchdog) WATCHDOG_DISABLED="1"; shift ;;
      --max-reboots)
        [[ $# -ge 2 ]] || { usage; exit 2; }
        nonnegative_integer "$2" || { echo "FAIL: --max-reboots requires a non-negative integer" >&2; exit 2; }
        MAX_REBOOTS="$2"; shift 2
        ;;
      --ram-mib)
        [[ $# -ge 2 ]] || { usage; exit 2; }
        positive_integer "$2" || { echo "FAIL: --ram-mib requires a positive integer" >&2; exit 2; }
        RAM_MIB="$2"; RAM_MIB_EXPLICIT="1"; shift 2
        ;;
      --smp-cpus)
        [[ $# -ge 2 ]] || { usage; exit 2; }
        smp_cpu_count "$2" || { echo "FAIL: --smp-cpus requires an integer from 1 to 123" >&2; exit 2; }
        SMP_CPUS="$2"; SMP_CPUS_EXPLICIT="1"; shift 2
        ;;
      --ramfb-samples)
        [[ $# -ge 2 ]] || { usage; exit 2; }
        ramfb_sample_list "$2" || { echo "FAIL: --ramfb-samples requires 1-16 positive comma-separated integers, each <= 120000" >&2; exit 2; }
        RAMFB_SAMPLES="$2"; shift 2
        ;;
      --boot-timer) BOOT_TIMER="1"; shift ;;
      --boot-timer-ramfb-ms)
        [[ $# -ge 2 ]] || { usage; exit 2; }
        boot_timer_ramfb_ms "$2" || { echo "FAIL: --boot-timer-ramfb-ms requires an integer from 100 to 60000" >&2; exit 2; }
        BOOT_TIMER="1"; BOOT_TIMER_RAMFB_MS="$2"; shift 2
        ;;
      --boot-timer-desktop-checksum64)
        [[ $# -ge 2 ]] || { usage; exit 2; }
        u64_literal "$2" || { echo "FAIL: --boot-timer-desktop-checksum64 requires a u64 decimal or 0x-prefixed hex value" >&2; exit 2; }
        BOOT_TIMER="1"; BOOT_TIMER_DESKTOP_CHECKSUM64="$2"; shift 2
        ;;
      --boot-timer-desktop-agent)
        BOOT_TIMER="1"; BOOT_TIMER_DESKTOP_AGENT="1"; shift
        ;;
      --shutdown-after-agent-ready) SHUTDOWN_AFTER_AGENT_READY="1"; shift ;;
      --host-pause-resume-proof-ms)
        [[ $# -ge 2 ]] || { usage; exit 2; }
        host_pause_resume_proof_ms "$2" || { echo "FAIL: --host-pause-resume-proof-ms requires an integer from 100 to 60000" >&2; exit 2; }
        HOST_PAUSE_RESUME_PROOF_MS="$2"; shift 2
        ;;
      --agent-service-control)
        [[ $# -ge 2 ]] || { usage; exit 2; }
        agent_service_control_path_value "$2" || { echo "FAIL: --agent-service-control requires a non-empty path of at most 4096 bytes without CR or LF" >&2; exit 2; }
        AGENT_SERVICE_CONTROL="$2"; shift 2
        ;;
      --agent-service-command)
        [[ $# -ge 2 ]] || { usage; exit 2; }
        agent_service_command_value "$2" || { echo "FAIL: --agent-service-command requires 1-1024 bytes without CR, LF, or |" >&2; exit 2; }
        AGENT_SERVICE_COMMAND="$2"; AGENT_SERVICE_COMMAND_EXPLICIT="1"; shift 2
        ;;
      --agent-clipboard-sync) AGENT_CLIPBOARD_SYNC="1"; shift ;;
      --agent-share-host)
        [[ $# -ge 2 ]] || { usage; exit 2; }
        agent_share_path_value "$2" || { echo "FAIL: --agent-share-host requires a non-empty path of at most 4096 bytes without CR, LF, or ::" >&2; exit 2; }
        AGENT_SHARE_HOST="$2"; shift 2
        ;;
      --agent-share-guest)
        [[ $# -ge 2 ]] || { usage; exit 2; }
        agent_share_path_value "$2" || { echo "FAIL: --agent-share-guest requires a non-empty path of at most 4096 bytes without CR, LF, or ::" >&2; exit 2; }
        AGENT_SHARE_GUEST="$2"; shift 2
        ;;
      --agent-share-ms)
        [[ $# -ge 2 ]] || { usage; exit 2; }
        agent_share_interval_ms "$2" || { echo "FAIL: --agent-share-ms requires an integer from 500 to 60000" >&2; exit 2; }
        AGENT_SHARE_MS="$2"; AGENT_SHARE_MS_EXPLICIT="1"; shift 2
        ;;
      --agent-share-max-kb)
        [[ $# -ge 2 ]] || { usage; exit 2; }
        agent_share_max_kb "$2" || { echo "FAIL: --agent-share-max-kb requires an integer from 1 to 1048576" >&2; exit 2; }
        AGENT_SHARE_MAX_KB="$2"; AGENT_SHARE_MAX_KB_EXPLICIT="1"; shift 2
        ;;
      --enable-xhci) ENABLE_XHCI="1"; shift ;;
      --virtio-net) VIRTIO_NET="1"; shift ;;
      --nvme-buffered-io) NVME_BUFFERED_IO="1"; shift ;;
      --trace-irq) TRACE_IRQ="1"; shift ;;
      --virtio-gpu-3d) VIRTIO_GPU_3D="1"; shift ;;
      --virtio-gpu-device-id)
        [[ $# -ge 2 ]] || { usage; exit 2; }
        normalize_virtio_gpu_device_id "$2" >/dev/null || { echo "FAIL: --virtio-gpu-device-id must be 1050 or 10f7" >&2; exit 2; }
        VIRTIO_GPU_PCI_DEVICE_ID="$(normalize_virtio_gpu_device_id "$2")"
        shift 2
        ;;
      --require-gpu-trace-gate) REQUIRE_GPU_TRACE_GATE="1"; shift ;;
      --gpu-trace)
        [[ $# -ge 2 ]] || { usage; exit 2; }
        [[ -n "$2" ]] || { echo "FAIL: --gpu-trace requires a non-empty path" >&2; exit 2; }
        VIRTIO_GPU_TRACE_JSONL="$2"; shift 2
        ;;
      --gpu-trace-protocol)
        [[ $# -ge 2 ]] || { usage; exit 2; }
        case "$2" in
          auto|venus|virgl) GPU_TRACE_PROTOCOL="$2" ;;
          *) echo "FAIL: --gpu-trace-protocol must be auto, venus, or virgl" >&2; exit 2 ;;
        esac
        shift 2
        ;;
      --viogpu3d-dir)
        [[ $# -ge 2 ]] || { usage; exit 2; }
        [[ -n "$2" ]] || { echo "FAIL: --viogpu3d-dir requires a non-empty path" >&2; exit 2; }
        VIOGPU3D_DIR="$2"; shift 2
        ;;
      --require-viogpu3d-readiness) REQUIRE_VIOGPU3D_READINESS="1"; shift ;;
      --setup-input-actions)
        [[ $# -ge 2 ]] || { usage; exit 2; }
        setup_input_actions_list "$2" || { echo "FAIL: --setup-input-actions requires 1-32 comma-separated actions from: tab, enter, space, win+r, lgui+r, text:<[a-z0-9/.-]+>" >&2; exit 2; }
        SETUP_INPUT_ACTIONS="$2"; shift 2
        ;;
      --setup-input-marker) [[ $# -ge 2 ]] || { usage; exit 2; }; setup_input_marker_value "$2" || { echo "FAIL: --setup-input-marker requires 1-96 bytes" >&2; exit 2; }; SETUP_INPUT_MARKER="$2"; shift 2 ;;
      --setup-input-fire-delay-ms) [[ $# -ge 2 ]] || { usage; exit 2; }; setup_input_fire_delay_ms "$2" || { echo "FAIL: --setup-input-fire-delay-ms requires an integer <= 600000" >&2; exit 2; }; SETUP_INPUT_FIRE_DELAY_MS="$2"; shift 2 ;;
      --setup-input-ramfb-delay-ms) [[ $# -ge 2 ]] || { usage; exit 2; }; ramfb_sample_list "$2" || { echo "FAIL: --setup-input-ramfb-delay-ms requires 1-16 positive comma-separated integers, each <= 120000" >&2; exit 2; }; SETUP_INPUT_RAMFB_DELAY_MS="$2"; shift 2 ;;
      --setup-input2-actions) [[ $# -ge 2 ]] || { usage; exit 2; }; setup_input_actions_list "$2" || { echo "FAIL: --setup-input2-actions requires 1-32 comma-separated actions from: tab, enter, space, win+r, lgui+r, text:<[a-z0-9/.-]+>" >&2; exit 2; }; SETUP_INPUT2_ACTIONS="$2"; shift 2 ;;
      --setup-input2-marker) [[ $# -ge 2 ]] || { usage; exit 2; }; setup_input_marker_value "$2" || { echo "FAIL: --setup-input2-marker requires 1-96 bytes" >&2; exit 2; }; SETUP_INPUT2_MARKER="$2"; shift 2 ;;
      --setup-input2-fire-delay-ms) [[ $# -ge 2 ]] || { usage; exit 2; }; setup_input_fire_delay_ms "$2" || { echo "FAIL: --setup-input2-fire-delay-ms requires an integer <= 600000" >&2; exit 2; }; SETUP_INPUT2_FIRE_DELAY_MS="$2"; shift 2 ;;
      --setup-input2-ramfb-delay-ms) [[ $# -ge 2 ]] || { usage; exit 2; }; ramfb_sample_list "$2" || { echo "FAIL: --setup-input2-ramfb-delay-ms requires 1-16 positive comma-separated integers, each <= 120000" >&2; exit 2; }; SETUP_INPUT2_RAMFB_DELAY_MS="$2"; shift 2 ;;
      --setup-input3-actions) [[ $# -ge 2 ]] || { usage; exit 2; }; setup_input_actions_list "$2" || { echo "FAIL: --setup-input3-actions requires 1-32 comma-separated actions from: tab, enter, space, win+r, lgui+r, text:<[a-z0-9/.-]+>" >&2; exit 2; }; SETUP_INPUT3_ACTIONS="$2"; shift 2 ;;
      --setup-input3-marker) [[ $# -ge 2 ]] || { usage; exit 2; }; setup_input_marker_value "$2" || { echo "FAIL: --setup-input3-marker requires 1-96 bytes" >&2; exit 2; }; SETUP_INPUT3_MARKER="$2"; shift 2 ;;
      --setup-input3-fire-delay-ms) [[ $# -ge 2 ]] || { usage; exit 2; }; setup_input_fire_delay_ms "$2" || { echo "FAIL: --setup-input3-fire-delay-ms requires an integer <= 600000" >&2; exit 2; }; SETUP_INPUT3_FIRE_DELAY_MS="$2"; shift 2 ;;
      --setup-input3-ramfb-delay-ms) [[ $# -ge 2 ]] || { usage; exit 2; }; ramfb_sample_list "$2" || { echo "FAIL: --setup-input3-ramfb-delay-ms requires 1-16 positive comma-separated integers, each <= 120000" >&2; exit 2; }; SETUP_INPUT3_RAMFB_DELAY_MS="$2"; shift 2 ;;
      --pointer-input-actions) [[ $# -ge 2 ]] || { usage; exit 2; }; pointer_input_actions_list "$2" || { echo "FAIL: --pointer-input-actions requires 1-16 actions from: move:<x>x<y>, click:<x>x<y>, click:center with decimal coordinates <= 32767" >&2; exit 2; }; POINTER_INPUT_ACTIONS="$2"; shift 2 ;;
      --pointer-input-marker) [[ $# -ge 2 ]] || { usage; exit 2; }; setup_input_marker_value "$2" || { echo "FAIL: --pointer-input-marker requires 1-96 bytes" >&2; exit 2; }; POINTER_INPUT_MARKER="$2"; shift 2 ;;
      --pointer-input-fire-delay-ms) [[ $# -ge 2 ]] || { usage; exit 2; }; setup_input_fire_delay_ms "$2" || { echo "FAIL: --pointer-input-fire-delay-ms requires an integer <= 600000" >&2; exit 2; }; POINTER_INPUT_FIRE_DELAY_MS="$2"; shift 2 ;;
      --pointer-input-ramfb-delay-ms) [[ $# -ge 2 ]] || { usage; exit 2; }; ramfb_sample_list "$2" || { echo "FAIL: --pointer-input-ramfb-delay-ms requires 1-16 positive comma-separated integers, each <= 120000" >&2; exit 2; }; POINTER_INPUT_RAMFB_DELAY_MS="$2"; shift 2 ;;
      --release) BUILD_PROFILE="release"; shift ;;
      --skip-build) SKIP_BUILD="1"; shift ;;
      --daily) DAILY="1"; shift ;;
      --print-policy) PRINT_POLICY="1"; shift ;;
      -h|--help) usage; exit 0 ;;
      *) usage; exit 2 ;;
    esac
  done
  apply_installed_boot_daily_defaults
}

apply_installed_boot_daily_defaults() {
  [[ "$DAILY" == "1" ]] || return 0
  [[ "$RAM_MIB_EXPLICIT" == "1" ]] || RAM_MIB="6144"
  if [[ "$WATCHDOG_MS_EXPLICIT" != "1" && "$WATCHDOG_DISABLED" != "1" ]]; then
    WATCHDOG_MS="86400000"
  fi
  [[ "$SMP_CPUS_EXPLICIT" == "1" ]] || SMP_CPUS="4"
  if [[ "$SKIP_BUILD" != "1" ]]; then
    BUILD_PROFILE="release"
  fi
}

validate_installed_boot_option_combinations() {
  if [[ "$WATCHDOG_DISABLED" == "1" && "$WATCHDOG_MS_EXPLICIT" == "1" ]]; then
    echo "FAIL: --no-watchdog cannot be combined with --watchdog-ms" >&2
    exit 2
  fi
  if [[ -n "$SETUP_INPUT_ACTIONS" && "$ENABLE_XHCI" != "1" ]]; then
    echo "FAIL: --setup-input-actions requires --enable-xhci" >&2
    exit 2
  fi
  if [[ -n "$SETUP_INPUT2_ACTIONS" && "$ENABLE_XHCI" != "1" ]]; then
    echo "FAIL: --setup-input2-actions requires --enable-xhci" >&2
    exit 2
  fi
  if [[ -n "$SETUP_INPUT3_ACTIONS" && "$ENABLE_XHCI" != "1" ]]; then
    echo "FAIL: --setup-input3-actions requires --enable-xhci" >&2
    exit 2
  fi
  if [[ -n "$POINTER_INPUT_ACTIONS" && "$ENABLE_XHCI" != "1" ]]; then
    echo "FAIL: --pointer-input-actions requires --enable-xhci" >&2
    exit 2
  fi
  if [[ -n "$VIRTIO_GPU_TRACE_JSONL" && "$VIRTIO_GPU_3D" != "1" ]]; then
    echo "FAIL: --gpu-trace requires --virtio-gpu-3d" >&2
    exit 2
  fi
  if [[ "$REQUIRE_GPU_TRACE_GATE" == "1" && "$VIRTIO_GPU_3D" != "1" ]]; then
    echo "FAIL: --require-gpu-trace-gate requires --virtio-gpu-3d" >&2
    exit 2
  fi
  if [[ "$GPU_TRACE_PROTOCOL" != "auto" && "$VIRTIO_GPU_3D" != "1" ]]; then
    echo "FAIL: --gpu-trace-protocol requires --virtio-gpu-3d" >&2
    exit 2
  fi
  if [[ -n "$VIRTIO_GPU_PCI_DEVICE_ID" && "$VIRTIO_GPU_3D" != "1" ]]; then
    echo "FAIL: --virtio-gpu-device-id requires --virtio-gpu-3d" >&2
    exit 2
  fi
  if [[ -n "$VIOGPU3D_DIR" && "$VIRTIO_GPU_3D" != "1" ]]; then
    echo "FAIL: --viogpu3d-dir requires --virtio-gpu-3d" >&2
    exit 2
  fi
  if [[ "$REQUIRE_VIOGPU3D_READINESS" == "1" && "$VIRTIO_GPU_3D" != "1" ]]; then
    echo "FAIL: --require-viogpu3d-readiness requires --virtio-gpu-3d" >&2
    exit 2
  fi
  if [[ "$BOOT_TIMER" == "1" && "${BRIDGEVM_SMP_TRACE+x}" == "x" ]] && truthy_env_value "$BRIDGEVM_SMP_TRACE"; then
    echo "FAIL: --boot-timer cannot be measured with BRIDGEVM_SMP_TRACE=$BRIDGEVM_SMP_TRACE; unset it or set it to 0" >&2
    exit 2
  fi
  if [[ "$BOOT_TIMER_DESKTOP_AGENT" == "1" && -n "$BOOT_TIMER_DESKTOP_CHECKSUM64" ]]; then
    echo "FAIL: choose exactly one BOOT_TIMER desktop oracle: --boot-timer-desktop-agent or --boot-timer-desktop-checksum64" >&2
    exit 2
  fi
  if [[ -n "$HOST_PAUSE_RESUME_PROOF_MS" && "$SHUTDOWN_AFTER_AGENT_READY" == "1" ]]; then
    echo "FAIL: --host-pause-resume-proof-ms controls its own post-resume shutdown and cannot be combined with --shutdown-after-agent-ready" >&2
    exit 2
  fi
  if [[ -z "$AGENT_SERVICE_CONTROL" && ( "$AGENT_SERVICE_COMMAND_EXPLICIT" == "1" || "$AGENT_CLIPBOARD_SYNC" == "1" || -n "$AGENT_SHARE_HOST" || -n "$AGENT_SHARE_GUEST" || "$AGENT_SHARE_MS_EXPLICIT" == "1" || "$AGENT_SHARE_MAX_KB_EXPLICIT" == "1" ) ]]; then
    echo "FAIL: agent command, clipboard, and share options require --agent-service-control" >&2
    exit 2
  fi
  if [[ -n "$AGENT_SERVICE_CONTROL" && ( "$SHUTDOWN_AFTER_AGENT_READY" == "1" || -n "$HOST_PAUSE_RESUME_PROOF_MS" ) ]]; then
    echo "FAIL: --agent-service-control cannot be combined with one-shot shutdown or host pause/resume proof controls" >&2
    exit 2
  fi
  if [[ -n "$AGENT_SHARE_HOST" && -z "$AGENT_SHARE_GUEST" ]] || [[ -z "$AGENT_SHARE_HOST" && -n "$AGENT_SHARE_GUEST" ]]; then
    echo "FAIL: --agent-share-host and --agent-share-guest must be provided together" >&2
    exit 2
  fi
  if [[ "$AGENT_SHARE_MS_EXPLICIT" == "1" && -z "$AGENT_SHARE_HOST" ]]; then
    echo "FAIL: --agent-share-ms requires --agent-share-host and --agent-share-guest" >&2
    exit 2
  fi
  if [[ "$AGENT_SHARE_MAX_KB_EXPLICIT" == "1" && -z "$AGENT_SHARE_HOST" ]]; then
    echo "FAIL: --agent-share-max-kb requires --agent-share-host and --agent-share-guest" >&2
    exit 2
  fi
  if [[ -z "$SETUP_INPUT_ACTIONS" && ( -n "$SETUP_INPUT_MARKER" || -n "$SETUP_INPUT_FIRE_DELAY_MS" || -n "$SETUP_INPUT_RAMFB_DELAY_MS" ) ]]; then
    echo "FAIL: setup-input marker/delay options require --setup-input-actions" >&2
    exit 2
  fi
  if [[ -z "$SETUP_INPUT2_ACTIONS" && ( -n "$SETUP_INPUT2_MARKER" || -n "$SETUP_INPUT2_FIRE_DELAY_MS" || -n "$SETUP_INPUT2_RAMFB_DELAY_MS" ) ]]; then
    echo "FAIL: setup-input2 marker/delay options require --setup-input2-actions" >&2
    exit 2
  fi
  if [[ -z "$SETUP_INPUT3_ACTIONS" && ( -n "$SETUP_INPUT3_MARKER" || -n "$SETUP_INPUT3_FIRE_DELAY_MS" || -n "$SETUP_INPUT3_RAMFB_DELAY_MS" ) ]]; then
    echo "FAIL: setup-input3 marker/delay options require --setup-input3-actions" >&2
    exit 2
  fi
  if [[ -z "$POINTER_INPUT_ACTIONS" && ( -n "$POINTER_INPUT_MARKER" || -n "$POINTER_INPUT_FIRE_DELAY_MS" || -n "$POINTER_INPUT_RAMFB_DELAY_MS" ) ]]; then
    echo "FAIL: pointer-input marker/delay options require --pointer-input-actions" >&2
    exit 2
  fi
}

virtio_gpu_3d_runtime_protocol() {
  if [[ "${GPU_TRACE_PROTOCOL:-auto}" == "virgl" ]]; then
    printf 'virgl\n'
  else
    printf 'venus\n'
  fi
}

configure_installed_boot_xhci_policy() {
  XHCI_POLICY="BRIDGEVM_DISABLE_XHCI=1"
  XHCI_REASON="xHCI disabled by default for the proven install/desktop-safe path"
  if [[ "$ENABLE_XHCI" == "1" ]]; then
    XHCI_POLICY="BRIDGEVM_DISABLE_XHCI=<unset> (--enable-xhci)"
    XHCI_REASON="xHCI enabled for Workstream D desktop input diagnosis"
  fi
}

validate_installed_boot_required_paths() {
  [[ -n "$TARGET" && -n "$VARS" && -n "$EVIDENCE_DIR" ]] || { usage; exit 2; }
  [[ -f "$TARGET" ]] || { echo "FAIL: target image not found: $TARGET" >&2; exit 1; }
  if [[ -n "$PLACEHOLDER_NSID1" ]]; then
    [[ -f "$PLACEHOLDER_NSID1" ]] || { echo "FAIL: placeholder NSID-1 image not found: $PLACEHOLDER_NSID1" >&2; exit 1; }
  fi
  [[ -f "$VARS" ]] || { echo "FAIL: vars file not found: $VARS" >&2; exit 1; }
  if [[ -n "$VIOGPU3D_DIR" ]]; then
    [[ -d "$VIOGPU3D_DIR" ]] || { echo "FAIL: viogpu3d driver directory not found: $VIOGPU3D_DIR" >&2; exit 1; }
  fi
  if [[ -n "$AGENT_SHARE_HOST" ]]; then
    [[ -d "$AGENT_SHARE_HOST" ]] || { echo "FAIL: agent share host directory not found: $AGENT_SHARE_HOST" >&2; exit 1; }
  fi
  require_not_preserved_source_media target "$TARGET"
  require_not_preserved_source_media vars "$VARS"
  if [[ -n "$PLACEHOLDER_NSID1" ]]; then
    require_not_preserved_source_media placeholder-nsid1 "$PLACEHOLDER_NSID1"
  fi
}

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
    "DAILY_PRESET=$DAILY" \
    "BRIDGEVM_RAM_MIB=$RAM_MIB" \
    "BRIDGEVM_BOOT_PROBE_WATCHDOG_MS=$WATCHDOG_MS" \
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
