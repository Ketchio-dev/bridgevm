#!/usr/bin/env bash
set -euo pipefail

# run-vz-display-demo.sh
#
# One command to see the Fast Mode (Apple VZ) EMBEDDED DISPLAY: builds + signs
# the AppleVzRunner, fetches a bootable Debian arm64 Linux fixture, stages a VM
# bundle, and opens a VZVirtualMachineView window showing the guest.
#
#   bash scripts/run-vz-display-demo.sh           # opens the on-screen window
#   bash scripts/run-vz-display-demo.sh --check    # headless boot check (no window,
#                                                   # for CI / SSH sessions)
#
# The window form must run in a GUI login session (it needs a window server).
# --check boots the SAME graphics configuration headless and asserts the guest
# comes up (proving everything except the on-screen pixels, which need a GUI).
#
# Requirements: macOS 14+ on Apple Silicon, swift, python3, curl.

MODE="window"
if [[ "${1:-}" == "--check" ]]; then
  MODE="check"
elif [[ -n "${1:-}" && "${1:-}" != "--window" ]]; then
  echo "usage: $0 [--window|--check]" >&2
  exit 2
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

FIXTURE_DIR="${BRIDGEVM_LIVE_VZ_FIXTURE_DIR:-/tmp/bridgevm-apple-vz-debian-fixture}"
KERNEL="$FIXTURE_DIR/linux"
INITRD="$FIXTURE_DIR/initrd.gz"
RAW_DISK="$FIXTURE_DIR/root.raw"
KERNEL_CMDLINE="${BRIDGEVM_LIVE_VZ_KERNEL_CMDLINE:-console=hvc0 priority=low}"

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

# 3. Stage a VM bundle with the fixture's kernel/initrd/disk.
DEMO_DIR="$(mktemp -d /tmp/bvm-vz-display.XXXXXX)"
VM_NAME="vz-display-demo"
BUNDLE="$DEMO_DIR/vms/$VM_NAME.vmbridge"
mkdir -p "$BUNDLE/boot" "$BUNDLE/disks" "$BUNDLE/logs" "$BUNDLE/metadata"
cp "$KERNEL" "$BUNDLE/boot/vmlinuz"
cp "$INITRD" "$BUNDLE/boot/initrd"
cp "$RAW_DISK" "$BUNDLE/disks/root.raw"
SERIAL_LOG="$BUNDLE/logs/serial.log"
LAUNCH_SPEC="$BUNDLE/metadata/apple-vz-launch.json"
HANDOFF="$BUNDLE/metadata/handoff.json"

# 4. Generate the launch spec + handoff for the staged bundle.
python3 - "$BUNDLE" "$VM_NAME" "$SERIAL_LOG" "$KERNEL_CMDLINE" "$LAUNCH_SPEC" "$HANDOFF" <<'PY'
import json, sys
bundle, vm, serial_log, cmdline, spec_path, handoff_path = sys.argv[1:7]
spec = {
    "vm_name": vm,
    "bundle_path": bundle,
    "guest": {"os": "debian", "arch": "arm64"},
    "boot": {
        "mode": "linux-kernel",
        "installer_image": None,
        "kernel": {"path": f"{bundle}/boot/vmlinuz", "exists": True},
        "initrd": {"path": f"{bundle}/boot/initrd", "exists": True},
        "kernel_command_line": cmdline,
        "macos_restore_image": None,
    },
    "disk": {"path": f"{bundle}/disks/root.raw", "format": "raw", "read_only": False},
    "resources": {
        "memory": "4096", "cpu": "2", "display_fps_cap": "60",
        "rationale": "Embedded display demo.", "balloon_device": True,
    },
    "devices": {"entropy_device": True, "network": "nat", "serial_log_path": serial_log},
    "integration": {
        "clipboard": True, "dynamic_resolution": True,
        "shared_folders": True, "virtiofs": True,
    },
    "logs": {"runner_log_path": f"{bundle}/logs/runner.log"},
    "readiness": {"ready": True, "blockers": []},
}
json.dump(spec, open(spec_path, "w"), indent=2)
handoff = {
    "backend": "apple-virtualization-framework",
    "vm_name": vm,
    "bundle_path": bundle,
    "launch_spec_path": spec_path,
    "guest": spec["guest"],
    "boot_mode": "linux-kernel",
    "disk": spec["disk"],
    "resources": spec["resources"],
    "runner_log_path": f"{bundle}/logs/runner.log",
    "serial_log_path": serial_log,
    "integration": spec["integration"],
    "readiness": spec["readiness"],
}
json.dump(handoff, open(handoff_path, "w"), indent=2)
PY

echo "==> Bundle staged at $BUNDLE"
echo "==> Runner: $RUNNER"

# 5. Launch.
if [[ "$MODE" == "check" ]]; then
  echo "==> Headless graphics boot check (12s, no window)..."
  BRIDGEVM_APPLE_VZ_ALLOW_REAL_START=1 "$RUNNER" \
    --graphics --allow-real-vz-start --stop-after-seconds 12 \
    --handoff-json "$HANDOFF" >"$BUNDLE/logs/runner.log" 2>&1 || true
  if grep -aqE 'Run /init|Debian|installer' "$SERIAL_LOG" 2>/dev/null; then
    echo "PASS: the guest booted with the Virtio GPU graphics device attached (headless)."
    echo "      Serial log: $SERIAL_LOG"
    echo "      Run without --check, in a GUI session, to see the window."
  else
    echo "FAIL: guest did not reach init with the graphics device; see $SERIAL_LOG" >&2
    exit 1
  fi
else
  echo "==> Opening the embedded display window (close it to stop the VM)..."
  echo "    (must be a GUI login session; for headless/SSH use --check)"
  BRIDGEVM_APPLE_VZ_ALLOW_REAL_START=1 exec "$RUNNER" \
    --display --allow-real-vz-start --handoff-json "$HANDOFF"
fi
