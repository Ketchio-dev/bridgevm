#!/usr/bin/env bash
set -euo pipefail

# run-vz-display-demo.sh
#
# One command to see the Fast Mode (Apple VZ) EMBEDDED DISPLAY: builds + signs
# the AppleVzRunner, fetches a bootable Debian arm64 Linux fixture, stages a VM
# bundle, and opens a VZVirtualMachineView window showing the guest.
#
#   bash scripts/run-vz-display-demo.sh           # opens the on-screen window
#   bash scripts/run-vz-display-demo.sh --preflight # checks local readiness only
#   bash scripts/run-vz-display-demo.sh --check    # headless boot check (no window,
#                                                   # for CI / SSH sessions)
#   bash scripts/run-vz-display-demo.sh --width 1440 --height 900
#   bash scripts/run-vz-display-demo.sh --prove-window --evidence-dir /tmp/vz-display-proof
#   bash scripts/run-vz-display-demo.sh --prove-proxy-crop --evidence-dir /tmp/vz-proxy-crop-proof
#
# The window form must run in a GUI login session (it needs a window server).
# --preflight prints local readiness without downloading fixtures, building or
# signing helpers, launching Apple VZ, opening GUI windows, or running displayd.
# --check boots the SAME graphics configuration headless and asserts the guest
# comes up (proving everything except the on-screen pixels, which need a GUI).
# --prove-window opens the GUI window, captures that exact window with
# screencapture, and writes a small evidence bundle.
# --prove-proxy-crop does the same, then proves the app-direct Show Display
# framebuffer export can feed displayd's proxy-window crop artifact path. It
# runs the preserved evidence verifier before reporting PASS. It still does
# not prove per-guest-app live streaming or true Coherence.
#
# Requirements: macOS 14+ on Apple Silicon, swift, python3, curl. The
# --prove-proxy-crop mode also needs cargo or a prebuilt target/release/displayd.

usage() {
  echo "usage: $0 [--window|--preflight|--check|--prove-window|--prove-proxy-crop] [--width PX --height PX] [--evidence-dir DIR] [--proof-seconds N] [--capture-delay N]" >&2
}

positive_integer() {
  [[ "$1" =~ ^[1-9][0-9]*$ ]]
}

MODE="window"
DISPLAY_WIDTH=""
DISPLAY_HEIGHT=""
EVIDENCE_DIR=""
PROOF_SECONDS="18"
CAPTURE_DELAY="6"
PROOF_SECONDS_SET="0"
CAPTURE_DELAY_SET="0"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --window)
      MODE="window"
      shift
      ;;
    --preflight)
      MODE="preflight"
      shift
      ;;
    --check)
      MODE="check"
      shift
      ;;
    --prove-window)
      MODE="prove-window"
      shift
      ;;
    --prove-proxy-crop)
      MODE="prove-proxy-crop"
      shift
      ;;
    --width)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      positive_integer "$2" || { echo "FAIL: --width requires a positive integer" >&2; exit 2; }
      DISPLAY_WIDTH="$2"
      shift 2
      ;;
    --height)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      positive_integer "$2" || { echo "FAIL: --height requires a positive integer" >&2; exit 2; }
      DISPLAY_HEIGHT="$2"
      shift 2
      ;;
    --evidence-dir)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      EVIDENCE_DIR="$2"
      shift 2
      ;;
    --proof-seconds)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      positive_integer "$2" || { echo "FAIL: --proof-seconds requires a positive integer" >&2; exit 2; }
      PROOF_SECONDS="$2"
      PROOF_SECONDS_SET="1"
      shift 2
      ;;
    --capture-delay)
      [[ $# -ge 2 ]] || { usage; exit 2; }
      positive_integer "$2" || { echo "FAIL: --capture-delay requires a positive integer" >&2; exit 2; }
      CAPTURE_DELAY="$2"
      CAPTURE_DELAY_SET="1"
      shift 2
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
if [[ -n "$DISPLAY_WIDTH" || -n "$DISPLAY_HEIGHT" ]]; then
  [[ -n "$DISPLAY_WIDTH" && -n "$DISPLAY_HEIGHT" ]] || {
    echo "FAIL: --width and --height must be provided together" >&2
    exit 2
  }
fi
if [[ "$MODE" != "prove-window" && "$MODE" != "prove-proxy-crop" && -n "$EVIDENCE_DIR" ]]; then
  echo "FAIL: --evidence-dir only applies to --prove-window or --prove-proxy-crop" >&2
  exit 2
fi
if [[ "$MODE" != "prove-window" && "$MODE" != "prove-proxy-crop" ]] && {
  [[ "$PROOF_SECONDS_SET" == "1" || "$CAPTURE_DELAY_SET" == "1" ]]
}; then
  echo "FAIL: --proof-seconds and --capture-delay only apply to --prove-window or --prove-proxy-crop" >&2
  exit 2
fi
if [[ "$MODE" == "prove-window" || "$MODE" == "prove-proxy-crop" ]] && (( CAPTURE_DELAY >= PROOF_SECONDS )); then
  echo "FAIL: --capture-delay must be less than --proof-seconds" >&2
  exit 2
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

FIXTURE_DIR="${BRIDGEVM_LIVE_VZ_FIXTURE_DIR:-/tmp/bridgevm-apple-vz-debian-fixture}"
KERNEL="$FIXTURE_DIR/linux"
INITRD="$FIXTURE_DIR/initrd.gz"
RAW_DISK="$FIXTURE_DIR/root.raw"
KERNEL_CMDLINE="${BRIDGEVM_LIVE_VZ_KERNEL_CMDLINE:-console=hvc0 priority=low}"

json_string() {
  python3 -c 'import json, sys; print(json.dumps(sys.argv[1]))' "$1"
}

file_sha256() {
  shasum -a 256 "$1" | awk '{print $1}'
}

find_display_window() {
  local pid="$1"
  local min_width="$2"
  local min_height="$3"
  swift -e 'import CoreGraphics
let pid = Int32(CommandLine.arguments[1])!
let minWidth = Double(CommandLine.arguments[2])!
let minHeight = Double(CommandLine.arguments[3])!
let opts = CGWindowListOption(arrayLiteral: .optionAll)
let list = CGWindowListCopyWindowInfo(opts, kCGNullWindowID) as? [[String: Any]] ?? []
for window in list {
    guard
        let ownerPid = window[kCGWindowOwnerPID as String] as? Int32,
        ownerPid == pid,
        let layer = window[kCGWindowLayer as String] as? Int,
        layer == 0,
        let windowID = window[kCGWindowNumber as String],
        let bounds = window[kCGWindowBounds as String] as? [String: Any],
        let width = bounds["Width"] as? Double,
        let height = bounds["Height"] as? Double,
        width >= minWidth,
        height >= minHeight
    else {
        continue
    }
    let owner = window[kCGWindowOwnerName as String] ?? ""
    let name = window[kCGWindowName as String] ?? ""
    print("window_id=\(windowID)")
    print("owner=\(owner)")
    print("name=\(name)")
    print("width=\(Int(width))")
    print("height=\(Int(height))")
    exit(0)
}
exit(1)' "$pid" "$min_width" "$min_height"
}

png_dimensions() {
  sips -g pixelWidth -g pixelHeight "$1" 2>/dev/null | awk '
    /pixelWidth:/ { width=$2 }
    /pixelHeight:/ { height=$2 }
    END {
      if (width != "" && height != "") {
        print width " " height
      }
    }
  '
}

file_size_bytes() {
  stat -f%z "$1" 2>/dev/null || echo 0
}

wait_for_file_size() {
  local path="$1"
  local expected_size="$2"
  local attempts="${3:-80}"
  for _ in $(seq 1 "$attempts"); do
    if [[ -f "$path" && "$(file_size_bytes "$path")" == "$expected_size" ]]; then
      return 0
    fi
    sleep 0.25
  done
  return 1
}

displayd() {
  if [[ -x "$ROOT/target/release/displayd" ]]; then
    "$ROOT/target/release/displayd" "$@"
  else
    cargo run --quiet -p displayd -- "$@"
  fi
}

print_preflight_file_state() {
  local label="$1"
  local path="$2"
  if [[ -f "$path" ]]; then
    echo "$label: present ($path)"
  else
    echo "$label: missing ($path)"
    PREFLIGHT_BLOCKERS+=("$label missing")
  fi
}

print_preflight_command_state() {
  local command="$1"
  local required="$2"
  if command -v "$command" >/dev/null 2>&1; then
    echo "Command $command: present ($(command -v "$command"))"
  elif [[ "$required" == "required" ]]; then
    echo "Command $command: missing"
    PREFLIGHT_BLOCKERS+=("command $command missing")
  else
    echo "Command $command: missing (optional)"
  fi
}

if [[ "$MODE" == "preflight" ]]; then
  PREFLIGHT_BLOCKERS=()
  HOST_OS="$(uname -s 2>/dev/null || echo unknown)"
  HOST_ARCH="$(uname -m 2>/dev/null || echo unknown)"
  echo "BridgeVM Apple VZ display demo preflight"
  echo "No downloads, signing, Apple VZ launch, GUI capture, or displayd run performed."
  echo "Host OS: $HOST_OS"
  echo "Host arch: $HOST_ARCH"
  if [[ "$HOST_OS" != "Darwin" ]]; then
    PREFLIGHT_BLOCKERS+=("host is not macOS")
  fi
  case "$HOST_ARCH" in
    arm64|aarch64) ;;
    *) PREFLIGHT_BLOCKERS+=("host is not Apple Silicon arm64") ;;
  esac

  echo "Fixture dir: $FIXTURE_DIR"
  print_preflight_file_state "Kernel" "$KERNEL"
  print_preflight_file_state "Initrd" "$INITRD"
  print_preflight_file_state "Raw disk" "$RAW_DISK"
  echo "Kernel command line: $KERNEL_CMDLINE"

  print_preflight_command_state python3 required
  print_preflight_command_state swift required
  print_preflight_command_state codesign required
  print_preflight_command_state curl optional
  print_preflight_command_state screencapture optional
  print_preflight_command_state cargo optional

  if [[ -n "${BRIDGEVM_APPLE_VZ_RUNNER:-}" ]]; then
    if [[ -x "$BRIDGEVM_APPLE_VZ_RUNNER" ]]; then
      echo "AppleVzRunner: configured ($BRIDGEVM_APPLE_VZ_RUNNER)"
    else
      echo "AppleVzRunner: configured but not executable ($BRIDGEVM_APPLE_VZ_RUNNER)"
      PREFLIGHT_BLOCKERS+=("configured AppleVzRunner is not executable")
    fi
  elif [[ -x "$ROOT/apps/macos/scripts/build-sign-apple-vz-runner.sh" ]]; then
    echo "AppleVzRunner: not configured; demo will build/sign it with apps/macos/scripts/build-sign-apple-vz-runner.sh"
  else
    echo "AppleVzRunner: missing BRIDGEVM_APPLE_VZ_RUNNER and build-sign script"
    PREFLIGHT_BLOCKERS+=("AppleVzRunner unavailable")
  fi

  echo "Default display size: ${DISPLAY_WIDTH:-1280}x${DISPLAY_HEIGHT:-800}"
  if [[ "${#PREFLIGHT_BLOCKERS[@]}" -eq 0 ]]; then
    echo "Preflight ready without setup: true"
  else
    echo "Preflight ready without setup: false"
    local_blocker=""
    for local_blocker in "${PREFLIGHT_BLOCKERS[@]}"; do
      echo "Blocker: $local_blocker"
    done
  fi
  exit 0
fi

# 1. Fixture (kernel + initrd + raw disk). Build it if it is not already present.
if [[ ! -f "$KERNEL" || ! -f "$INITRD" || ! -f "$RAW_DISK" ]]; then
  echo "==> Building the Debian arm64 VZ fixture (downloads ~80 MB)..."
  bash tests/integration/prepare-apple-vz-debian-fixture.sh >/dev/null
fi
[[ -f "$KERNEL" && -f "$INITRD" && -f "$RAW_DISK" ]] || {
  echo "FAIL: fixture not available under $FIXTURE_DIR" >&2
  exit 1
}

# 2. Signed AppleVzRunner (com.apple.security.virtualization entitlement).
RUNNER="${BRIDGEVM_APPLE_VZ_RUNNER:-}"
if [[ -z "$RUNNER" ]]; then
  echo "==> Building + signing AppleVzRunner..."
  RUNNER="$(apps/macos/scripts/build-sign-apple-vz-runner.sh | tail -1)"
fi
[[ -x "$RUNNER" ]] || { echo "FAIL: AppleVzRunner not found at '$RUNNER'" >&2; exit 1; }

# 3. Stage a VM bundle through the same create/prepare-run path used by the CLI
# and app. The display demo keeps the launch step manual so GUI/proof modes can
# choose whether to open a window, run headless, or capture evidence.
DEMO_DIR="$(mktemp -d /tmp/bvm-vz-display.XXXXXX)"
VM_NAME="vz-display-demo"
BUNDLE="$DEMO_DIR/vms/$VM_NAME.vmbridge"
SERIAL_LOG="$BUNDLE/logs/serial.log"
LAUNCH_SPEC="$BUNDLE/metadata/apple-vz-launch.json"
HANDOFF="$BUNDLE/metadata/handoff.json"

echo "==> Staging Apple VZ Linux VM through bridgevm create/prepare-run..."
scripts/stage-vz-linux-demo-vm.sh \
  --store "$DEMO_DIR" \
  --name "$VM_NAME" \
  --kernel "$KERNEL" \
  --initrd "$INITRD" \
  --raw-disk "$RAW_DISK" \
  --disk "${BRIDGEVM_LIVE_VZ_RAW_DISK_SIZE:-64MiB}" \
  --kernel-command-line "$KERNEL_CMDLINE" >/dev/null
mkdir -p "$BUNDLE/logs" "$BUNDLE/metadata"

# 4. Generate the handoff from the launch spec written by prepare-run.
cargo run --quiet -p lightvm-runner -- \
  --launch-spec "$LAUNCH_SPEC" \
  --require-ready \
  --print-handoff >"$HANDOFF"

echo "==> Bundle staged at $BUNDLE"
echo "==> Runner: $RUNNER"
if [[ -n "$DISPLAY_WIDTH" ]]; then
  echo "==> Display size: ${DISPLAY_WIDTH}x${DISPLAY_HEIGHT}"
fi

# Share a host folder into the guest (mount in-guest with:
#   mount -t virtiofs hostshare /mnt/hostshare).
SHARE_DIR="$DEMO_DIR/hostshare"
mkdir -p "$SHARE_DIR"
echo "hello from the host" > "$SHARE_DIR/README.txt"

# 5. Launch.
if [[ "$MODE" == "check" ]]; then
  echo "==> Headless graphics + shared-folder boot check (12s, no window)..."
  DISPLAY_ARGS=(--display-width "${DISPLAY_WIDTH:-1280}" --display-height "${DISPLAY_HEIGHT:-800}")
  BRIDGEVM_APPLE_VZ_ALLOW_REAL_START=1 "$RUNNER" \
    --graphics "${DISPLAY_ARGS[@]}" --share-dir "$SHARE_DIR" --share-tag hostshare \
    --allow-real-vz-start --stop-after-seconds 12 \
    --handoff-json "$HANDOFF" >"$BUNDLE/logs/runner.log" 2>&1 || true
  if grep -aqE 'Run /init|Debian|installer' "$SERIAL_LOG" 2>/dev/null; then
    echo "PASS: the guest booted with the Virtio GPU graphics device attached (headless)."
    echo "      Serial log: $SERIAL_LOG"
    echo "      Run without --check, in a GUI session, to see the window."
  else
    echo "FAIL: guest did not reach init with the graphics device; see $SERIAL_LOG" >&2
    exit 1
  fi
elif [[ "$MODE" == "prove-window" || "$MODE" == "prove-proxy-crop" ]]; then
  command -v screencapture >/dev/null || {
    echo "FAIL: screencapture is required for --prove-window or --prove-proxy-crop" >&2
    exit 1
  }
  if [[ "$MODE" == "prove-proxy-crop" && ! -x "$ROOT/target/release/displayd" ]]; then
    command -v cargo >/dev/null || {
      echo "FAIL: --prove-proxy-crop requires cargo or target/release/displayd" >&2
      exit 1
    }
  fi
  [[ -n "$EVIDENCE_DIR" ]] || EVIDENCE_DIR="$DEMO_DIR/display-window-evidence"
  mkdir -p "$EVIDENCE_DIR"

  if [[ "$MODE" == "prove-proxy-crop" ]]; then
    echo "==> Proving embedded display window + app-direct proxy crop (${PROOF_SECONDS}s run, capture after ${CAPTURE_DELAY}s)..."
  else
    echo "==> Proving embedded display window (${PROOF_SECONDS}s run, capture after ${CAPTURE_DELAY}s)..."
  fi
  echo "    Evidence: $EVIDENCE_DIR"
  DISPLAY_ARGS=(--display-width "${DISPLAY_WIDTH:-1280}" --display-height "${DISPLAY_HEIGHT:-800}")
  REQUESTED_WIDTH="${DISPLAY_WIDTH:-1280}"
  REQUESTED_HEIGHT="${DISPLAY_HEIGHT:-800}"
  WINDOW_MIN_WIDTH="$REQUESTED_WIDTH"
  WINDOW_MIN_HEIGHT="$REQUESTED_HEIGHT"
  RUNNER_OUTPUT="$EVIDENCE_DIR/apple-vz-display-runner.output"
  FRAME="$EVIDENCE_DIR/viewer-frame.png"
  WINDOW_INFO="$EVIDENCE_DIR/window-info.txt"
  SERIAL_EVIDENCE="$EVIDENCE_DIR/serial.log"
  FRAMEBUFFER_RGBA="$EVIDENCE_DIR/app-direct-framebuffer.rgba"
  WINDOW_CROP_JSON="$EVIDENCE_DIR/app-direct-window-crop.json"
  WINDOW_CROP_RGBA="$EVIDENCE_DIR/app-direct-window-crop.rgba"
  PROXY_CROP_VERIFIER_OUTPUT="$EVIDENCE_DIR/app-direct-proxy-crop-verifier.output"
  RUNNER_PID=""
  PROXY_FRAMEBUFFER_ARGS=()
  if [[ "$MODE" == "prove-proxy-crop" ]]; then
    PROXY_CROP_VERIFIER="$ROOT/tests/integration/verify-vz-proxy-crop-evidence.sh"
    [[ -x "$PROXY_CROP_VERIFIER" ]] || {
      echo "FAIL: --prove-proxy-crop requires $PROXY_CROP_VERIFIER" >&2
      exit 1
    }
    PROXY_FRAMEBUFFER_ARGS=(
      --proxy-framebuffer-rgba-file "$FRAMEBUFFER_RGBA"
      --proxy-framebuffer-capture-interval-ms 250
    )
  fi

  BRIDGEVM_APPLE_VZ_ALLOW_REAL_START=1 "$RUNNER" \
    --display "${DISPLAY_ARGS[@]}" --share-dir "$SHARE_DIR" --share-tag hostshare \
    "${PROXY_FRAMEBUFFER_ARGS[@]}" \
    --allow-real-vz-start --stop-after-seconds "$PROOF_SECONDS" --force-stop-grace-seconds 4 \
    --handoff-json "$HANDOFF" >"$RUNNER_OUTPUT" 2>&1 &
  RUNNER_PID="$!"
  RUNNER_PID_FOR_EVIDENCE="$RUNNER_PID"

  cleanup_proof_runner() {
    if [[ -n "$RUNNER_PID" ]] && kill -0 "$RUNNER_PID" 2>/dev/null; then
      kill -TERM "$RUNNER_PID" 2>/dev/null || true
      wait "$RUNNER_PID" 2>/dev/null || true
    fi
  }
  trap cleanup_proof_runner EXIT

  sleep "$CAPTURE_DELAY"
  if ! find_display_window "$RUNNER_PID" "$WINDOW_MIN_WIDTH" "$WINDOW_MIN_HEIGHT" >"$WINDOW_INFO"; then
    echo "FAIL: could not find the Apple VZ display window for pid $RUNNER_PID" >&2
    echo "      Runner output: $RUNNER_OUTPUT" >&2
    exit 1
  fi
  WINDOW_ID="$(awk -F= '/^window_id=/ {print $2; exit}' "$WINDOW_INFO")"
  [[ -n "$WINDOW_ID" ]] || {
    echo "FAIL: display window id was not recorded" >&2
    exit 1
  }

  screencapture -x -l "$WINDOW_ID" "$FRAME"
  [[ -s "$FRAME" ]] || {
    echo "FAIL: window capture is empty: $FRAME" >&2
    exit 1
  }
  read -r FRAME_WIDTH FRAME_HEIGHT < <(png_dimensions "$FRAME")
  positive_integer "${FRAME_WIDTH:-}" || { echo "FAIL: captured PNG width is unavailable" >&2; exit 1; }
  positive_integer "${FRAME_HEIGHT:-}" || { echo "FAIL: captured PNG height is unavailable" >&2; exit 1; }

  if [[ "$MODE" == "prove-proxy-crop" ]]; then
    EXPECTED_FRAMEBUFFER_BYTES=$(( REQUESTED_WIDTH * REQUESTED_HEIGHT * 4 ))
    if ! wait_for_file_size "$FRAMEBUFFER_RGBA" "$EXPECTED_FRAMEBUFFER_BYTES"; then
      echo "FAIL: app-direct RGBA framebuffer was not exported at the expected size" >&2
      echo "      Expected: $EXPECTED_FRAMEBUFFER_BYTES bytes (${REQUESTED_WIDTH}x${REQUESTED_HEIGHT}x4)" >&2
      echo "      Actual: $(file_size_bytes "$FRAMEBUFFER_RGBA") bytes at $FRAMEBUFFER_RGBA" >&2
      echo "      Runner output: $RUNNER_OUTPUT" >&2
      exit 1
    fi

    CROP_WIDTH=$(( REQUESTED_WIDTH / 2 ))
    CROP_HEIGHT=$(( REQUESTED_HEIGHT / 2 ))
    (( CROP_WIDTH > 0 )) || CROP_WIDTH=1
    (( CROP_HEIGHT > 0 )) || CROP_HEIGHT=1
    CROP_X=$(( (REQUESTED_WIDTH - CROP_WIDTH) / 2 ))
    CROP_Y=$(( (REQUESTED_HEIGHT - CROP_HEIGHT) / 2 ))
    EXPECTED_CROP_BYTES=$(( CROP_WIDTH * CROP_HEIGHT * 4 ))

    displayd \
      --print-plan \
      --visibility foreground \
      --dirty-regions 4 \
      --framebuffer-width "$REQUESTED_WIDTH" \
      --framebuffer-height "$REQUESTED_HEIGHT" \
      --scale 1 \
      --window-id app-direct-proof \
      --window-title "App Direct Crop Proof" \
      --window-x "$CROP_X" \
      --window-y "$CROP_Y" \
      --window-width "$CROP_WIDTH" \
      --window-height "$CROP_HEIGHT" \
      --framebuffer-rgba-file "$FRAMEBUFFER_RGBA" \
      --window-crop-rgba-file "$WINDOW_CROP_RGBA" \
      >"$WINDOW_CROP_JSON"

    [[ -s "$WINDOW_CROP_JSON" ]] || {
      echo "FAIL: displayd did not write crop summary JSON: $WINDOW_CROP_JSON" >&2
      exit 1
    }
    if [[ "$(file_size_bytes "$WINDOW_CROP_RGBA")" != "$EXPECTED_CROP_BYTES" ]]; then
      echo "FAIL: app-direct crop artifact has the wrong byte size" >&2
      echo "      Expected: $EXPECTED_CROP_BYTES bytes (${CROP_WIDTH}x${CROP_HEIGHT}x4)" >&2
      echo "      Actual: $(file_size_bytes "$WINDOW_CROP_RGBA") bytes at $WINDOW_CROP_RGBA" >&2
      exit 1
    fi
  fi

  wait "$RUNNER_PID" || true
  RUNNER_PID=""
  trap - EXIT
  cp "$SERIAL_LOG" "$SERIAL_EVIDENCE" 2>/dev/null || : >"$SERIAL_EVIDENCE"

  cat >"$EVIDENCE_DIR/viewer-evidence.json" <<EOF
{
  "proven": true,
  "kind": "graphical-viewer",
  "artifact": "viewer-frame.png",
  "width": $FRAME_WIDTH,
  "height": $FRAME_HEIGHT,
  "sha256": "$(file_sha256 "$FRAME")",
  "observation": "screencapture captured the BridgeVM Apple VZ display window"
}
EOF
  cat >"$EVIDENCE_DIR/display-window-proof.json" <<EOF
{
  "proven": true,
  "mode": "$(if [[ "$MODE" == "prove-proxy-crop" ]]; then echo "apple-vz-display-window-proxy-crop"; else echo "apple-vz-display-window"; fi)",
  "runner": $(json_string "$RUNNER"),
  "runner_pid": $RUNNER_PID_FOR_EVIDENCE,
  "window_id": $(json_string "$WINDOW_ID"),
  "requested_width": $REQUESTED_WIDTH,
  "requested_height": $REQUESTED_HEIGHT,
  "captured_width": $FRAME_WIDTH,
  "captured_height": $FRAME_HEIGHT,
  "proof_seconds": $PROOF_SECONDS,
  "capture_delay_seconds": $CAPTURE_DELAY,
  "viewer_frame": {
    "artifact": "viewer-frame.png",
    "sha256": "$(file_sha256 "$FRAME")"
  },
  "serial_log": {
    "artifact": "serial.log",
    "sha256": "$(if [[ -s "$SERIAL_EVIDENCE" ]]; then file_sha256 "$SERIAL_EVIDENCE"; fi)"
  },
  "runner_output": {
    "artifact": "apple-vz-display-runner.output",
    "sha256": "$(file_sha256 "$RUNNER_OUTPUT")"
  }
}
EOF
  if [[ "$MODE" == "prove-proxy-crop" ]]; then
    cat >"$EVIDENCE_DIR/app-direct-proxy-crop-proof.json" <<EOF
{
  "proven": true,
  "kind": "app-direct-proxy-crop",
  "framebuffer": {
    "artifact": "app-direct-framebuffer.rgba",
    "width": $REQUESTED_WIDTH,
    "height": $REQUESTED_HEIGHT,
    "bytes": $EXPECTED_FRAMEBUFFER_BYTES,
    "sha256": "$(file_sha256 "$FRAMEBUFFER_RGBA")"
  },
  "crop": {
    "summary_artifact": "app-direct-window-crop.json",
    "rgba_artifact": "app-direct-window-crop.rgba",
    "x": $CROP_X,
    "y": $CROP_Y,
    "width": $CROP_WIDTH,
    "height": $CROP_HEIGHT,
    "bytes": $EXPECTED_CROP_BYTES,
    "summary_sha256": "$(file_sha256 "$WINDOW_CROP_JSON")",
    "rgba_sha256": "$(file_sha256 "$WINDOW_CROP_RGBA")"
  },
  "observation": "AppleVzRunner exported the app-direct VZVirtualMachineView RGBA file and displayd materialized a crop artifact from it"
}
EOF
  fi
  cat >"$EVIDENCE_DIR/SUMMARY.txt" <<EOF
Fast Mode embedded display window proof: passed
Evidence directory: $EVIDENCE_DIR
Window info: $(tr '\n' ' ' <"$WINDOW_INFO")
Captured frame: viewer-frame.png (${FRAME_WIDTH}x${FRAME_HEIGHT})
Runner output: apple-vz-display-runner.output
Serial log: serial.log
EOF
  if [[ "$MODE" == "prove-proxy-crop" ]]; then
    cat >>"$EVIDENCE_DIR/SUMMARY.txt" <<EOF
App-direct framebuffer: app-direct-framebuffer.rgba (${REQUESTED_WIDTH}x${REQUESTED_HEIGHT}, ${EXPECTED_FRAMEBUFFER_BYTES} bytes)
Displayd crop summary: app-direct-window-crop.json
Displayd crop RGBA: app-direct-window-crop.rgba (${CROP_WIDTH}x${CROP_HEIGHT}, ${EXPECTED_CROP_BYTES} bytes)
EOF
    "$PROXY_CROP_VERIFIER" "$EVIDENCE_DIR" >"$PROXY_CROP_VERIFIER_OUTPUT"
    cat >>"$EVIDENCE_DIR/SUMMARY.txt" <<EOF
Proxy crop verifier: app-direct-proxy-crop-verifier.output
EOF
  fi

  if [[ "$MODE" == "prove-proxy-crop" ]]; then
    echo "PASS: captured the Fast Mode Apple VZ display window and materialized an app-direct proxy crop."
    echo "      Crop: $WINDOW_CROP_RGBA (${CROP_WIDTH}x${CROP_HEIGHT})"
    echo "      Verifier: $PROXY_CROP_VERIFIER_OUTPUT"
  else
    echo "PASS: captured the Fast Mode Apple VZ display window."
  fi
  echo "      Evidence: $EVIDENCE_DIR"
  echo "      Frame: $FRAME (${FRAME_WIDTH}x${FRAME_HEIGHT})"
else
  echo "==> Opening the embedded display window (close it to stop the VM)..."
  echo "    (must be a GUI login session; for headless/SSH use --check)"
  echo "    In the guest: mount -t virtiofs hostshare /mnt && cat /mnt/README.txt"
  DISPLAY_ARGS=(--display-width "${DISPLAY_WIDTH:-1280}" --display-height "${DISPLAY_HEIGHT:-800}")
  BRIDGEVM_APPLE_VZ_ALLOW_REAL_START=1 exec "$RUNNER" \
    --display "${DISPLAY_ARGS[@]}" --share-dir "$SHARE_DIR" --share-tag hostshare \
    --allow-real-vz-start --handoff-json "$HANDOFF"
fi
