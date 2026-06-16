#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
usage: tests/integration/prepare-apple-vz-alpine-fixture.sh [--dry-run]

Downloads Alpine arm64 netboot vmlinuz/initramfs fixtures for the manual Apple
VZ live boot smoke, creates a sparse raw disk, and prints shell-safe
BRIDGEVM_LIVE_VZ_* exports. It does not start Apple VZ and does not set the
real-start opt-in.

Warning: Alpine netboot vmlinuz artifacts may be PE32+ EFI applications, which
VZLinuxBootLoader may reject. Prefer the Debian helper for known-good Apple VZ
LinuxBootLoader live boot fixtures.

Environment overrides:
  BRIDGEVM_LIVE_VZ_FIXTURE_DIR
  BRIDGEVM_LIVE_VZ_ALPINE_BASE_URL
  BRIDGEVM_LIVE_VZ_ALPINE_REPO_URL
  BRIDGEVM_LIVE_VZ_ALPINE_FLAVOR
  BRIDGEVM_LIVE_VZ_RAW_DISK_SIZE
USAGE
}

DRY_RUN=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)
      DRY_RUN=1
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

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

FIXTURE_DIR="${BRIDGEVM_LIVE_VZ_FIXTURE_DIR:-/tmp/bridgevm-apple-vz-alpine-fixture}"
ALPINE_BASE_URL="${BRIDGEVM_LIVE_VZ_ALPINE_BASE_URL:-https://dl-cdn.alpinelinux.org/alpine/latest-stable/releases/aarch64/netboot}"
ALPINE_REPO_URL="${BRIDGEVM_LIVE_VZ_ALPINE_REPO_URL:-https://dl-cdn.alpinelinux.org/alpine/latest-stable/main}"
ALPINE_FLAVOR="${BRIDGEVM_LIVE_VZ_ALPINE_FLAVOR:-virt}"
DISK_SIZE="${BRIDGEVM_LIVE_VZ_RAW_DISK_SIZE:-64m}"

KERNEL="$FIXTURE_DIR/vmlinuz-$ALPINE_FLAVOR"
INITRD="$FIXTURE_DIR/initramfs-$ALPINE_FLAVOR"
RAW_DISK="$FIXTURE_DIR/root.raw"
KERNEL_CMDLINE="console=hvc0 ip=dhcp alpine_repo=$ALPINE_REPO_URL modloop=$ALPINE_BASE_URL/modloop-$ALPINE_FLAVOR"
SERIAL_EXPECTED="Alpine Linux"

require_tool() {
  local tool="$1"
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "error: required tool is missing: $tool" >&2
    exit 1
  fi
}

require_sparse_disk_tool() {
  if command -v mkfile >/dev/null 2>&1 || command -v truncate >/dev/null 2>&1; then
    return
  fi

  echo "error: neither mkfile nor truncate is available to create $RAW_DISK" >&2
  exit 1
}

warn_vzlinuxbootloader_compatibility() {
  cat >&2 <<'WARNING'
warning: Alpine netboot vmlinuz artifacts may be PE32+ EFI applications and may
warning: be rejected by VZLinuxBootLoader. Prefer the Debian helper for a
warning: known-good Apple VZ LinuxBootLoader live boot fixture.
WARNING
}

download_if_missing() {
  local url="$1"
  local destination="$2"

  if [[ -f "$destination" ]]; then
    return
  fi

  local partial
  partial="$(mktemp "$destination.tmp.XXXXXX")"
  if ! curl -fL "$url" -o "$partial"; then
    rm -f "$partial"
    return 1
  fi
  mv "$partial" "$destination"
}

create_sparse_disk_if_missing() {
  local destination="$1"
  local size="$2"

  if [[ -f "$destination" ]]; then
    return
  fi

  if command -v mkfile >/dev/null 2>&1; then
    mkfile -n "$size" "$destination"
  else
    truncate -s "$size" "$destination"
  fi
}

print_exports() {
  printf 'export BRIDGEVM_LIVE_VZ_KERNEL=%q\n' "$KERNEL"
  printf 'export BRIDGEVM_LIVE_VZ_INITRD=%q\n' "$INITRD"
  printf 'export BRIDGEVM_LIVE_VZ_RAW_DISK=%q\n' "$RAW_DISK"
  printf 'export BRIDGEVM_LIVE_VZ_KERNEL_CMDLINE=%q\n' "$KERNEL_CMDLINE"
  printf 'export BRIDGEVM_LIVE_VZ_SERIAL_EXPECTED=%q\n' "$SERIAL_EXPECTED"
}

warn_vzlinuxbootloader_compatibility

if [[ "$DRY_RUN" == "1" ]]; then
  print_exports
  exit 0
fi

require_tool curl
require_tool mktemp
require_tool mv
require_tool mkdir
require_sparse_disk_tool

mkdir -p "$FIXTURE_DIR"
download_if_missing "$ALPINE_BASE_URL/vmlinuz-$ALPINE_FLAVOR" "$KERNEL"
download_if_missing "$ALPINE_BASE_URL/initramfs-$ALPINE_FLAVOR" "$INITRD"
create_sparse_disk_if_missing "$RAW_DISK" "$DISK_SIZE"
print_exports
