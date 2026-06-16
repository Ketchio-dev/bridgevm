#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
usage: tests/integration/prepare-apple-vz-debian-fixture.sh [--dry-run]

Downloads Debian arm64 netboot linux/initrd fixtures for the manual Apple VZ
live boot smoke, creates a sparse raw disk, and prints shell-safe
BRIDGEVM_LIVE_VZ_* exports. It does not start Apple VZ and does not set the
real-start opt-in.

Environment overrides:
  BRIDGEVM_LIVE_VZ_FIXTURE_DIR
  BRIDGEVM_LIVE_VZ_DEBIAN_KERNEL_URL
  BRIDGEVM_LIVE_VZ_DEBIAN_INITRD_URL
  BRIDGEVM_LIVE_VZ_DEBIAN_KERNEL_SHA256
  BRIDGEVM_LIVE_VZ_DEBIAN_INITRD_SHA256
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

FIXTURE_DIR="${BRIDGEVM_LIVE_VZ_FIXTURE_DIR:-/tmp/bridgevm-apple-vz-debian-fixture}"
DEBIAN_KERNEL_URL="${BRIDGEVM_LIVE_VZ_DEBIAN_KERNEL_URL:-https://ftp.debian.org/debian/dists/stable/main/installer-arm64/current/images/netboot/debian-installer/arm64/linux}"
DEBIAN_INITRD_URL="${BRIDGEVM_LIVE_VZ_DEBIAN_INITRD_URL:-https://ftp.debian.org/debian/dists/stable/main/installer-arm64/current/images/netboot/debian-installer/arm64/initrd.gz}"
DEBIAN_KERNEL_SHA256="${BRIDGEVM_LIVE_VZ_DEBIAN_KERNEL_SHA256:-}"
DEBIAN_INITRD_SHA256="${BRIDGEVM_LIVE_VZ_DEBIAN_INITRD_SHA256:-}"
DISK_SIZE="${BRIDGEVM_LIVE_VZ_RAW_DISK_SIZE:-64m}"

KERNEL="$FIXTURE_DIR/linux"
INITRD="$FIXTURE_DIR/initrd.gz"
RAW_DISK="$FIXTURE_DIR/root.raw"
KERNEL_CMDLINE="console=hvc0 priority=low"
SERIAL_EXPECTED="Debian"

require_tool() {
  local tool="$1"
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "error: required tool is missing: $tool" >&2
    exit 1
  fi
}

require_sha256_tool() {
  if have_sha256_tool; then
    return
  fi

  echo "error: neither shasum nor sha256sum is available to verify fixture downloads" >&2
  exit 1
}

have_sha256_tool() {
  command -v shasum >/dev/null 2>&1 || command -v sha256sum >/dev/null 2>&1
}

require_sparse_disk_tool() {
  if command -v mkfile >/dev/null 2>&1 || command -v truncate >/dev/null 2>&1; then
    return
  fi

  echo "error: neither mkfile nor truncate is available to create $RAW_DISK" >&2
  exit 1
}

file_sha256() {
  local path="$1"

  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$path" | awk '{print $1}'
  else
    sha256sum "$path" | awk '{print $1}'
  fi
}

lowercase_hex() {
  printf '%s' "$1" | tr 'A-F' 'a-f'
}

verify_sha256_if_pinned() {
  local path="$1"
  local expected="$2"
  local label="$3"

  if [[ -z "$expected" ]]; then
    return
  fi

  if [[ ! "$expected" =~ ^[0-9a-fA-F]{64}$ ]]; then
    echo "error: $label SHA-256 pin must be a 64-character hex digest" >&2
    exit 2
  fi

  local actual
  actual="$(file_sha256 "$path")"
  local normalized_expected
  normalized_expected="$(lowercase_hex "$expected")"
  if [[ "$actual" != "$normalized_expected" ]]; then
    echo "error: $label SHA-256 mismatch" >&2
    echo "  expected: $normalized_expected" >&2
    echo "  actual:   $actual" >&2
    exit 1
  fi
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

print_actual_sha256_values() {
  if [[ -f "$KERNEL" ]] && have_sha256_tool; then
    printf 'export BRIDGEVM_LIVE_VZ_DEBIAN_KERNEL_SHA256=%q\n' "$(file_sha256 "$KERNEL")"
  else
    printf '# BRIDGEVM_LIVE_VZ_DEBIAN_KERNEL_SHA256 unavailable until %q exists\n' "$KERNEL"
  fi

  if [[ -f "$INITRD" ]] && have_sha256_tool; then
    printf 'export BRIDGEVM_LIVE_VZ_DEBIAN_INITRD_SHA256=%q\n' "$(file_sha256 "$INITRD")"
  else
    printf '# BRIDGEVM_LIVE_VZ_DEBIAN_INITRD_SHA256 unavailable until %q exists\n' "$INITRD"
  fi
}

if [[ "$DRY_RUN" == "1" ]]; then
  print_exports
  print_actual_sha256_values
  exit 0
fi

require_tool curl
require_tool mktemp
require_tool mv
require_tool mkdir
require_tool awk
require_tool tr
require_sha256_tool
require_sparse_disk_tool

mkdir -p "$FIXTURE_DIR"
download_if_missing "$DEBIAN_KERNEL_URL" "$KERNEL"
download_if_missing "$DEBIAN_INITRD_URL" "$INITRD"
verify_sha256_if_pinned "$KERNEL" "$DEBIAN_KERNEL_SHA256" "Debian kernel"
verify_sha256_if_pinned "$INITRD" "$DEBIAN_INITRD_SHA256" "Debian initrd"
create_sparse_disk_if_missing "$RAW_DISK" "$DISK_SIZE"
print_exports
print_actual_sha256_values
