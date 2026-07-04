init_installed_boot_defaults() {
  TARGET=""
  PLACEHOLDER_NSID1=""
  VARS=""
  EVIDENCE_DIR=""
  WATCHDOG_MS="900000"
  MAX_REBOOTS="8"
  RAM_MIB="4096"
  RAMFB_SAMPLES="1000,5000,15000,30000,60000,90000,120000"
  ENABLE_XHCI="0"
  VIRTIO_NET="0"
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
  SKIP_BUILD="0"
  PRINT_POLICY="0"
  XHCI_POLICY=""
  XHCI_REASON=""
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
        WATCHDOG_MS="$2"; shift 2
        ;;
      --max-reboots)
        [[ $# -ge 2 ]] || { usage; exit 2; }
        nonnegative_integer "$2" || { echo "FAIL: --max-reboots requires a non-negative integer" >&2; exit 2; }
        MAX_REBOOTS="$2"; shift 2
        ;;
      --ram-mib)
        [[ $# -ge 2 ]] || { usage; exit 2; }
        positive_integer "$2" || { echo "FAIL: --ram-mib requires a positive integer" >&2; exit 2; }
        RAM_MIB="$2"; shift 2
        ;;
      --ramfb-samples)
        [[ $# -ge 2 ]] || { usage; exit 2; }
        ramfb_sample_list "$2" || { echo "FAIL: --ramfb-samples requires 1-16 positive comma-separated integers, each <= 120000" >&2; exit 2; }
        RAMFB_SAMPLES="$2"; shift 2
        ;;
      --enable-xhci) ENABLE_XHCI="1"; shift ;;
      --virtio-net) VIRTIO_NET="1"; shift ;;
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
      --skip-build) SKIP_BUILD="1"; shift ;;
      --print-policy) PRINT_POLICY="1"; shift ;;
      -h|--help) usage; exit 0 ;;
      *) usage; exit 2 ;;
    esac
  done
}

validate_installed_boot_option_combinations() {
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
  require_not_preserved_source_media target "$TARGET"
  require_not_preserved_source_media vars "$VARS"
  if [[ -n "$PLACEHOLDER_NSID1" ]]; then
    require_not_preserved_source_media placeholder-nsid1 "$PLACEHOLDER_NSID1"
  fi
}

print_installed_boot_policy() {
  printf '%s\n' \
    "$XHCI_POLICY" \
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
    'BRIDGEVM_NVME_DISK_WRITABLE=1 when booting target as only NVMe' \
    'BRIDGEVM_NVME_DISK2_WRITABLE=1 when --placeholder-nsid1 is set' \
    "reason=$XHCI_REASON; C4/D1 boots the installed target without the installer disk and supports the proven NSID-2 target position"
}
