#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

usage() {
  cat <<'USAGE'
usage: scripts/stage-vz-linux-demo-vm.sh [options]

Create a launch-ready Apple Virtualization.framework Linux demo VM bundle using
the official BridgeVM create/prepare-run path. This stages kernel/initrd/raw disk
fixtures only; it does not launch Apple VZ or open a GUI window.

Options:
  --name NAME              VM name (default: vz-linux-demo)
  --store DIR              BridgeVM store (default: BRIDGEVM_HOME or ~/.bridgevm)
  --fixture-dir DIR        Directory containing linux, initrd.gz, and root.raw
  --kernel PATH            Source Linux arm64 kernel image
  --initrd PATH            Source initrd image
  --raw-disk PATH          Source raw disk image
  --disk SIZE              Manifest disk size (default: 64MiB)
  --kernel-command-line X  Kernel command line (default: console=hvc0 priority=low)
  --prepare-fixture        Download/stage the Debian fixture first using the
                           existing opt-in fixture helper
  -h, --help               Show this help

Fixture defaults:
  If --kernel/--initrd/--raw-disk are omitted, this script reads
  BRIDGEVM_LIVE_VZ_KERNEL, BRIDGEVM_LIVE_VZ_INITRD, and BRIDGEVM_LIVE_VZ_RAW_DISK.
  With --fixture-dir, it uses DIR/linux, DIR/initrd.gz, and DIR/root.raw.

Next manual GUI step after staging:
  export BRIDGEVM_APPLE_VZ_RUNNER="$(apps/macos/scripts/build-sign-apple-vz-runner.sh)"
  cargo run --quiet -p bridgevm-cli -- --store <store> display <name> --width 1280 --height 800
USAGE
}

VM_NAME="vz-linux-demo"
STORE="${BRIDGEVM_HOME:-${HOME:-.}/.bridgevm}"
FIXTURE_DIR=""
KERNEL="${BRIDGEVM_LIVE_VZ_KERNEL:-}"
INITRD="${BRIDGEVM_LIVE_VZ_INITRD:-}"
RAW_DISK="${BRIDGEVM_LIVE_VZ_RAW_DISK:-}"
DISK_SIZE="${BRIDGEVM_LIVE_VZ_RAW_DISK_SIZE:-64MiB}"
KERNEL_CMDLINE="${BRIDGEVM_LIVE_VZ_KERNEL_CMDLINE:-console=hvc0 priority=low}"
PREPARE_FIXTURE=0
EXPLICIT_KERNEL=0
EXPLICIT_INITRD=0
EXPLICIT_RAW_DISK=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --name)
      VM_NAME="${2:?missing value for --name}"
      shift 2
      ;;
    --store)
      STORE="${2:?missing value for --store}"
      shift 2
      ;;
    --fixture-dir)
      FIXTURE_DIR="${2:?missing value for --fixture-dir}"
      shift 2
      ;;
    --kernel)
      KERNEL="${2:?missing value for --kernel}"
      EXPLICIT_KERNEL=1
      shift 2
      ;;
    --initrd)
      INITRD="${2:?missing value for --initrd}"
      EXPLICIT_INITRD=1
      shift 2
      ;;
    --raw-disk)
      RAW_DISK="${2:?missing value for --raw-disk}"
      EXPLICIT_RAW_DISK=1
      shift 2
      ;;
    --disk)
      DISK_SIZE="${2:?missing value for --disk}"
      shift 2
      ;;
    --kernel-command-line)
      KERNEL_CMDLINE="${2:?missing value for --kernel-command-line}"
      shift 2
      ;;
    --prepare-fixture)
      PREPARE_FIXTURE=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

fail() {
  echo "error: $*" >&2
  exit 1
}

bridgevm() {
  cargo run --quiet -p bridgevm-cli -- --store "$STORE" "$@"
}

require_regular_file() {
  local path="$1"
  local label="$2"
  [[ -n "$path" ]] || fail "$label path is required"
  [[ ! -L "$path" ]] || fail "$label must not be a symlink: $path"
  [[ -f "$path" ]] || fail "$label does not exist: $path"
  [[ -r "$path" ]] || fail "$label is not readable: $path"
}

if [[ "$PREPARE_FIXTURE" == "1" ]]; then
  eval "$(tests/integration/prepare-apple-vz-debian-fixture.sh)"
  [[ "$EXPLICIT_KERNEL" == "1" ]] || KERNEL="${BRIDGEVM_LIVE_VZ_KERNEL:-$KERNEL}"
  [[ "$EXPLICIT_INITRD" == "1" ]] || INITRD="${BRIDGEVM_LIVE_VZ_INITRD:-$INITRD}"
  [[ "$EXPLICIT_RAW_DISK" == "1" ]] || RAW_DISK="${BRIDGEVM_LIVE_VZ_RAW_DISK:-$RAW_DISK}"
  KERNEL_CMDLINE="${BRIDGEVM_LIVE_VZ_KERNEL_CMDLINE:-$KERNEL_CMDLINE}"
fi

if [[ -n "$FIXTURE_DIR" ]]; then
  [[ "$EXPLICIT_KERNEL" == "1" ]] || KERNEL="$FIXTURE_DIR/linux"
  [[ "$EXPLICIT_INITRD" == "1" ]] || INITRD="$FIXTURE_DIR/initrd.gz"
  [[ "$EXPLICIT_RAW_DISK" == "1" ]] || RAW_DISK="$FIXTURE_DIR/root.raw"
fi

require_regular_file "$KERNEL" "kernel"
require_regular_file "$INITRD" "initrd"
require_regular_file "$RAW_DISK" "raw disk"

BUNDLE="$STORE/vms/$VM_NAME.vmbridge"
[[ ! -e "$BUNDLE" ]] || fail "VM bundle already exists: $BUNDLE"

create_output="$(
  bridgevm create "$VM_NAME" \
    --os debian \
    --arch arm64 \
    --mode fast \
    --boot-mode linux-kernel \
    --kernel-path boot/vmlinuz \
    --initrd-path boot/initrd \
    --kernel-command-line "$KERNEL_CMDLINE" \
    --disk "$DISK_SIZE" \
    --disk-format raw
)"

mkdir -p "$BUNDLE/boot" "$BUNDLE/disks"
cp "$KERNEL" "$BUNDLE/boot/vmlinuz"
cp "$INITRD" "$BUNDLE/boot/initrd"
cp "$RAW_DISK" "$BUNDLE/disks/root.raw"
chmod u+rw "$BUNDLE/disks/root.raw"

prepare_output="$(bridgevm prepare-run "$VM_NAME")"
runner_status="$(bridgevm runner-status "$VM_NAME")"

case "$prepare_output" in
  *"Launch ready: true"*) ;;
  *)
    printf '%s\n' "$create_output"
    printf '%s\n' "$prepare_output"
    printf '%s\n' "$runner_status"
    fail "staged VM is not launch-ready"
    ;;
esac

cat <<EOF
Apple VZ Linux VM staged: $VM_NAME
Store: $STORE
Bundle: $BUNDLE
Kernel: $BUNDLE/boot/vmlinuz
Initrd: $BUNDLE/boot/initrd
Raw disk: $BUNDLE/disks/root.raw

$prepare_output

Next GUI command:
  export BRIDGEVM_APPLE_VZ_RUNNER="\$(apps/macos/scripts/build-sign-apple-vz-runner.sh)"
  cargo run --quiet -p bridgevm-cli -- --store "$STORE" display "$VM_NAME" --width 1280 --height 800
EOF
