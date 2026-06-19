#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

usage() {
  cat <<'USAGE'
usage: scripts/stage-vz-ubuntu-desktop-vm.sh [options]

Create a launch-ready Ubuntu Arm64 Apple Virtualization.framework VM bundle
using the BridgeVM Ubuntu linux-kernel/raw-disk template. This stages an already
bootable Ubuntu raw disk plus its matching kernel/initrd; it does not install
Ubuntu, launch Apple VZ, or open a GUI window.

Options:
  --name NAME              VM name (default: ubuntu-desktop-vz)
  --store DIR              BridgeVM store (default: BRIDGEVM_HOME or ~/.bridgevm)
  --fixture-dir DIR        Directory containing vmlinuz, initrd, and root.raw
  --kernel PATH            Source Ubuntu arm64 kernel image
  --initrd PATH            Source Ubuntu initrd image
  --raw-disk PATH          Source bootable Ubuntu raw disk image
  --disk SIZE              Manifest disk size (default: 32GiB)
  --kernel-command-line X  Kernel command line
                           (default: console=hvc0 root=/dev/vda2 rw systemd.unit=graphical.target)
  -h, --help               Show this help

Environment defaults:
  BRIDGEVM_UBUNTU_VZ_KERNEL, BRIDGEVM_UBUNTU_VZ_INITRD,
  BRIDGEVM_UBUNTU_VZ_RAW_DISK, BRIDGEVM_UBUNTU_VZ_RAW_DISK_SIZE, and
  BRIDGEVM_UBUNTU_VZ_KERNEL_CMDLINE.

Next manual GUI step after staging:
  export BRIDGEVM_APPLE_VZ_RUNNER="$(apps/macos/scripts/build-sign-apple-vz-runner.sh)"
  cargo run --quiet -p bridgevm-cli -- --store <store> display <name> --width 1440 --height 900
USAGE
}

VM_NAME="ubuntu-desktop-vz"
STORE="${BRIDGEVM_HOME:-${HOME:-.}/.bridgevm}"
FIXTURE_DIR=""
KERNEL="${BRIDGEVM_UBUNTU_VZ_KERNEL:-}"
INITRD="${BRIDGEVM_UBUNTU_VZ_INITRD:-}"
RAW_DISK="${BRIDGEVM_UBUNTU_VZ_RAW_DISK:-}"
DISK_SIZE="${BRIDGEVM_UBUNTU_VZ_RAW_DISK_SIZE:-32GiB}"
KERNEL_CMDLINE="${BRIDGEVM_UBUNTU_VZ_KERNEL_CMDLINE:-console=hvc0 root=/dev/vda2 rw systemd.unit=graphical.target}"
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

if [[ -n "$FIXTURE_DIR" ]]; then
  [[ "$EXPLICIT_KERNEL" == "1" ]] || KERNEL="$FIXTURE_DIR/vmlinuz"
  [[ "$EXPLICIT_INITRD" == "1" ]] || INITRD="$FIXTURE_DIR/initrd"
  [[ "$EXPLICIT_RAW_DISK" == "1" ]] || RAW_DISK="$FIXTURE_DIR/root.raw"
fi

require_regular_file "$KERNEL" "kernel"
require_regular_file "$INITRD" "initrd"
require_regular_file "$RAW_DISK" "raw disk"

BUNDLE="$STORE/vms/$VM_NAME.vmbridge"
[[ ! -e "$BUNDLE" ]] || fail "VM bundle already exists: $BUNDLE"

create_output="$(
  bridgevm create "$VM_NAME" \
    --template ubuntu-arm64-apple-vz-linux-kernel-raw \
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
    fail "staged Ubuntu VM is not launch-ready"
    ;;
esac

cat <<EOF
Ubuntu Apple VZ Desktop VM staged: $VM_NAME
Store: $STORE
Bundle: $BUNDLE
Kernel: $BUNDLE/boot/vmlinuz
Initrd: $BUNDLE/boot/initrd
Raw disk: $BUNDLE/disks/root.raw
Kernel command line: $KERNEL_CMDLINE

$prepare_output

Next GUI command:
  export BRIDGEVM_APPLE_VZ_RUNNER="\$(apps/macos/scripts/build-sign-apple-vz-runner.sh)"
  cargo run --quiet -p bridgevm-cli -- --store "$STORE" display "$VM_NAME" --width 1440 --height 900
EOF
