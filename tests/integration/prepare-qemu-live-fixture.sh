#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
usage: tests/integration/prepare-qemu-live-fixture.sh [--dry-run] [options]

Prepares shell-safe BRIDGEVM_LIVE_QEMU_* exports for the manual QEMU live boot
smoke using an operator-supplied bootable qcow2 disk. It creates only metadata
and evidence directories; it does not require QEMU/qemu-img, does not create a
disk image, does not start QEMU, and does not set the real-start opt-in.

Options:
  --qcow2 <path>         Bootable qcow2 disk fixture to copy into a disposable VM.
  --arch <arch>          Disposable VM guest arch. Defaults to x86_64.
  --store <path>         BridgeVM store for the smoke. Defaults to a /tmp path.
  --vm <name>            VM name for the smoke. Defaults to live-qemu.
  --evidence-dir <path>  Evidence output directory. Defaults under the store.
  --sentinel <text>      Serial log text required as live boot proof.
  --timeout <seconds>    Positive integer timeout. Defaults to 60.
  --dry-run              Print exports without creating directories or checking paths.

Environment defaults:
  BRIDGEVM_LIVE_QEMU_QCOW2_DISK
  BRIDGEVM_LIVE_QEMU_ARCH
  BRIDGEVM_LIVE_QEMU_STORE
  BRIDGEVM_LIVE_QEMU_VM
  BRIDGEVM_LIVE_QEMU_EVIDENCE_DIR
  BRIDGEVM_LIVE_QEMU_SERIAL_EXPECTED
  BRIDGEVM_LIVE_QEMU_TIMEOUT_SECONDS
USAGE
}

DRY_RUN=0
QCOW2="${BRIDGEVM_LIVE_QEMU_QCOW2_DISK:-}"
VM_ARCH="${BRIDGEVM_LIVE_QEMU_ARCH:-x86_64}"
STORE="${BRIDGEVM_LIVE_QEMU_STORE:-/tmp/bridgevm-live-qemu-fixture}"
VM_NAME="${BRIDGEVM_LIVE_QEMU_VM:-live-qemu}"
EVIDENCE_DIR="${BRIDGEVM_LIVE_QEMU_EVIDENCE_DIR:-}"
SERIAL_EXPECTED="${BRIDGEVM_LIVE_QEMU_SERIAL_EXPECTED:-}"
TIMEOUT_SECONDS="${BRIDGEVM_LIVE_QEMU_TIMEOUT_SECONDS:-60}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --qcow2)
      [[ $# -ge 2 ]] || { echo "error: --qcow2 requires a path" >&2; exit 2; }
      QCOW2="$2"
      shift 2
      ;;
    --store)
      [[ $# -ge 2 ]] || { echo "error: --store requires a path" >&2; exit 2; }
      STORE="$2"
      shift 2
      ;;
    --arch)
      [[ $# -ge 2 ]] || { echo "error: --arch requires an architecture" >&2; exit 2; }
      VM_ARCH="$2"
      shift 2
      ;;
    --vm)
      [[ $# -ge 2 ]] || { echo "error: --vm requires a name" >&2; exit 2; }
      VM_NAME="$2"
      shift 2
      ;;
    --evidence-dir)
      [[ $# -ge 2 ]] || { echo "error: --evidence-dir requires a path" >&2; exit 2; }
      EVIDENCE_DIR="$2"
      shift 2
      ;;
    --sentinel)
      [[ $# -ge 2 ]] || { echo "error: --sentinel requires text" >&2; exit 2; }
      SERIAL_EXPECTED="$2"
      shift 2
      ;;
    --timeout)
      [[ $# -ge 2 ]] || { echo "error: --timeout requires seconds" >&2; exit 2; }
      TIMEOUT_SECONDS="$2"
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

if [[ -z "$EVIDENCE_DIR" ]]; then
  EVIDENCE_DIR="$STORE/evidence"
fi

if [[ ! "$TIMEOUT_SECONDS" =~ ^[1-9][0-9]*$ ]]; then
  echo "error: --timeout/BRIDGEVM_LIVE_QEMU_TIMEOUT_SECONDS must be a positive integer" >&2
  exit 2
fi
case "$VM_ARCH" in
  x86_64|amd64|arm64|aarch64) ;;
  *)
    echo "error: --arch/BRIDGEVM_LIVE_QEMU_ARCH must be x86_64, amd64, arm64, or aarch64" >&2
    exit 2
    ;;
esac

if [[ "$DRY_RUN" != "1" ]]; then
  if [[ -n "$QCOW2" && ! -f "$QCOW2" ]]; then
    echo "error: qcow2 fixture does not exist: $QCOW2" >&2
    exit 1
  fi

  mkdir -p "$STORE" "$EVIDENCE_DIR"
fi

print_exports() {
  printf 'export BRIDGEVM_LIVE_QEMU_STORE=%q\n' "$STORE"
  printf 'export BRIDGEVM_LIVE_QEMU_VM=%q\n' "$VM_NAME"
  printf 'export BRIDGEVM_LIVE_QEMU_ARCH=%q\n' "$VM_ARCH"
  printf 'export BRIDGEVM_LIVE_QEMU_EVIDENCE_DIR=%q\n' "$EVIDENCE_DIR"
  printf 'export BRIDGEVM_LIVE_QEMU_TIMEOUT_SECONDS=%q\n' "$TIMEOUT_SECONDS"

  if [[ -n "$QCOW2" ]]; then
    printf 'export BRIDGEVM_LIVE_QEMU_QCOW2_DISK=%q\n' "$QCOW2"
  else
    printf '# export BRIDGEVM_LIVE_QEMU_QCOW2_DISK=/path/to/bootable-root.qcow2\n'
  fi

  if [[ -n "$SERIAL_EXPECTED" ]]; then
    printf 'export BRIDGEVM_LIVE_QEMU_SERIAL_EXPECTED=%q\n' "$SERIAL_EXPECTED"
  else
    printf '# export BRIDGEVM_LIVE_QEMU_SERIAL_EXPECTED=bridgevm-qemu-ready\n'
  fi

  printf '# export BRIDGEVM_LIVE_QEMU_ALLOW_REAL_START=1\n'
}

print_exports
