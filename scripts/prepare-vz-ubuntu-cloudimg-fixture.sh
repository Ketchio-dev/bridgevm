#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

usage() {
  cat <<'USAGE'
usage: scripts/prepare-vz-ubuntu-cloudimg-fixture.sh [options]

Prepare Ubuntu Arm64 direct-kernel Apple VZ inputs from official Ubuntu cloud
image artifacts. It downloads or copies:
  - noble-server-cloudimg-arm64-root.tar.xz
  - noble-server-cloudimg-arm64-vmlinuz-generic
  - noble-server-cloudimg-arm64-initrd-generic

Then it builds root.raw as a whole-disk ext4 image, so the matching kernel
command line is "console=hvc0 root=/dev/vda rw". This does not install Ubuntu
Desktop packages, start Apple VZ, open a GUI, or set real-start opt-ins.

Options:
  --dry-run             Print planned paths/URLs and shell exports only
  --fixture-dir DIR     Output directory (default: /tmp/bridgevm-apple-vz-ubuntu-cloudimg-fixture)
  --release NAME        Ubuntu release stream (default: noble)
  --base-url URL        Cloud image base URL (default: https://cloud-images.ubuntu.com/<release>/current)
  --root-tar PATH       Use an existing root.tar.xz instead of downloading
  --kernel PATH         Use an existing vmlinuz instead of downloading
  --initrd PATH         Use an existing initrd instead of downloading
  --disk-size SIZE      ext4 raw disk size passed to the builder (default: 32G)
  --builder NAME        auto, mkfs.ext4, virt-make-fs, or docker (default: auto)
  -h, --help            Show this help

Environment overrides:
  BRIDGEVM_UBUNTU_CLOUDIMG_FIXTURE_DIR
  BRIDGEVM_UBUNTU_CLOUDIMG_RELEASE
  BRIDGEVM_UBUNTU_CLOUDIMG_BASE_URL
  BRIDGEVM_UBUNTU_CLOUDIMG_ROOT_TAR
  BRIDGEVM_UBUNTU_CLOUDIMG_KERNEL
  BRIDGEVM_UBUNTU_CLOUDIMG_INITRD
  BRIDGEVM_UBUNTU_CLOUDIMG_RAW_DISK_SIZE
  BRIDGEVM_UBUNTU_CLOUDIMG_BUILDER
  BRIDGEVM_UBUNTU_CLOUDIMG_DOCKER_IMAGE
USAGE
}

DRY_RUN=0
FIXTURE_DIR="${BRIDGEVM_UBUNTU_CLOUDIMG_FIXTURE_DIR:-/tmp/bridgevm-apple-vz-ubuntu-cloudimg-fixture}"
RELEASE="${BRIDGEVM_UBUNTU_CLOUDIMG_RELEASE:-noble}"
BASE_URL="${BRIDGEVM_UBUNTU_CLOUDIMG_BASE_URL:-}"
ROOT_TAR_SOURCE="${BRIDGEVM_UBUNTU_CLOUDIMG_ROOT_TAR:-}"
KERNEL_SOURCE="${BRIDGEVM_UBUNTU_CLOUDIMG_KERNEL:-}"
INITRD_SOURCE="${BRIDGEVM_UBUNTU_CLOUDIMG_INITRD:-}"
DISK_SIZE="${BRIDGEVM_UBUNTU_CLOUDIMG_RAW_DISK_SIZE:-32G}"
BUILDER="${BRIDGEVM_UBUNTU_CLOUDIMG_BUILDER:-auto}"
DOCKER_IMAGE="${BRIDGEVM_UBUNTU_CLOUDIMG_DOCKER_IMAGE:-ubuntu:24.04}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --fixture-dir)
      FIXTURE_DIR="${2:?missing value for --fixture-dir}"
      shift 2
      ;;
    --release)
      RELEASE="${2:?missing value for --release}"
      shift 2
      ;;
    --base-url)
      BASE_URL="${2:?missing value for --base-url}"
      shift 2
      ;;
    --root-tar)
      ROOT_TAR_SOURCE="${2:?missing value for --root-tar}"
      shift 2
      ;;
    --kernel)
      KERNEL_SOURCE="${2:?missing value for --kernel}"
      shift 2
      ;;
    --initrd)
      INITRD_SOURCE="${2:?missing value for --initrd}"
      shift 2
      ;;
    --disk-size)
      DISK_SIZE="${2:?missing value for --disk-size}"
      shift 2
      ;;
    --builder)
      BUILDER="${2:?missing value for --builder}"
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

[[ -n "$BASE_URL" ]] || BASE_URL="https://cloud-images.ubuntu.com/$RELEASE/current"

case "$BUILDER" in
  auto|mkfs.ext4|virt-make-fs|docker) ;;
  *)
    echo "error: --builder must be auto, mkfs.ext4, virt-make-fs, or docker" >&2
    exit 2
    ;;
esac

ARTIFACT_PREFIX="$RELEASE-server-cloudimg-arm64"
DOWNLOAD_DIR="$FIXTURE_DIR/downloads"
WORK_DIR="$FIXTURE_DIR/work"
ROOT_TAR="$DOWNLOAD_DIR/$ARTIFACT_PREFIX-root.tar.xz"
KERNEL="$FIXTURE_DIR/vmlinuz"
INITRD="$FIXTURE_DIR/initrd"
RAW_DISK="$FIXTURE_DIR/root.raw"
ARTIFACTS_JSON="$FIXTURE_DIR/artifacts.json"
ROOT_URL="$BASE_URL/$ARTIFACT_PREFIX-root.tar.xz"
KERNEL_URL="$BASE_URL/unpacked/$ARTIFACT_PREFIX-vmlinuz-generic"
INITRD_URL="$BASE_URL/unpacked/$ARTIFACT_PREFIX-initrd-generic"
SHA256SUMS_URL="$BASE_URL/SHA256SUMS"
UNPACKED_SHA256SUMS_URL="$BASE_URL/unpacked/SHA256SUMS"
KERNEL_CMDLINE="console=hvc0 root=/dev/vda rw"
SERIAL_EXPECTED="Ubuntu"
SELECTED_BUILDER=""

require_tool() {
  local tool="$1"
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "error: required tool is missing: $tool" >&2
    exit 1
  fi
}

have_tool() {
  command -v "$1" >/dev/null 2>&1
}

file_sha256() {
  local path="$1"
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$path" | awk '{print $1}'
  else
    sha256sum "$path" | awk '{print $1}'
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

copy_or_download() {
  local source_path="$1"
  local url="$2"
  local destination="$3"
  local label="$4"

  if [[ -n "$source_path" ]]; then
    [[ -f "$source_path" ]] || {
      echo "error: $label source does not exist: $source_path" >&2
      exit 1
    }
    if [[ "$source_path" != "$destination" ]]; then
      cp "$source_path" "$destination"
    fi
  else
    download_if_missing "$url" "$destination"
  fi
}

expected_sha256_from_sums() {
  local sums_file="$1"
  local filename="$2"
  awk -v name="$filename" '
    {
      candidate=$2
      sub(/^\*/, "", candidate)
      if (candidate == name) {
        print $1
        exit
      }
    }
  ' "$sums_file"
}

verify_sha256_from_sums() {
  local path="$1"
  local sums_file="$2"
  local filename="$3"
  local label="$4"

  [[ -f "$sums_file" ]] || return 0
  local expected
  expected="$(expected_sha256_from_sums "$sums_file" "$filename")"
  [[ -n "$expected" ]] || return 0
  local actual
  actual="$(file_sha256 "$path")"
  if [[ "$actual" != "$expected" ]]; then
    echo "error: $label SHA-256 mismatch" >&2
    echo "  expected: $expected" >&2
    echo "  actual:   $actual" >&2
    exit 1
  fi
}

select_builder() {
  if [[ "$BUILDER" != "auto" ]]; then
    printf '%s\n' "$BUILDER"
    return
  fi
  if have_tool mkfs.ext4; then
    echo "mkfs.ext4"
  elif have_tool virt-make-fs; then
    echo "virt-make-fs"
  elif have_tool docker; then
    echo "docker"
  else
    echo "none"
  fi
}

build_with_mkfs_ext4() {
  require_tool tar
  require_tool mkfs.ext4
  rm -rf "$WORK_DIR/rootfs"
  mkdir -p "$WORK_DIR/rootfs"
  tar -xJf "$ROOT_TAR" -C "$WORK_DIR/rootfs"
  mkfs.ext4 -F -L cloudimg-rootfs -d "$WORK_DIR/rootfs" "$RAW_DISK" "$DISK_SIZE"
}

build_with_virt_make_fs() {
  require_tool virt-make-fs
  virt-make-fs --format=raw --type=ext4 --size="$DISK_SIZE" "$ROOT_TAR" "$RAW_DISK"
}

build_with_docker() {
  require_tool docker
  local root_tar_basename
  root_tar_basename="$(basename "$ROOT_TAR")"
  docker run --rm \
    -e DISK_SIZE="$DISK_SIZE" \
    -e ROOT_TAR="/work/downloads/$root_tar_basename" \
    -e RAW_DISK="/work/root.raw" \
    -v "$FIXTURE_DIR:/work" \
    "$DOCKER_IMAGE" \
    bash -lc '
      set -euo pipefail
      export DEBIAN_FRONTEND=noninteractive
      if ! command -v mkfs.ext4 >/dev/null 2>&1; then
        apt-get update
        apt-get install -y --no-install-recommends e2fsprogs xz-utils tar
      fi
      rm -rf /tmp/bridgevm-rootfs
      mkdir -p /tmp/bridgevm-rootfs
      tar -xJf "$ROOT_TAR" -C /tmp/bridgevm-rootfs
      mkfs.ext4 -F -L cloudimg-rootfs -d /tmp/bridgevm-rootfs "$RAW_DISK" "$DISK_SIZE"
    '
}

root_tar_listing() {
  tar -tJf "$ROOT_TAR" | sed -E 's#^\./##; s#^/##'
}

first_module_version() {
  root_tar_listing | awk -F/ '
    $1 == "lib" && $2 == "modules" && $3 != "" {
      print $3
      exit
    }
  '
}

desktop_stack_detected() {
  if root_tar_listing | grep -Eq '^(usr/sbin/(gdm3|lightdm|sddm)|usr/bin/startplasma-wayland|etc/systemd/system/display-manager\.service)$'; then
    echo "true"
  else
    echo "false"
  fi
}

json_escape() {
  awk '
    BEGIN { ORS = "" }
    {
      gsub(/\\/, "\\\\")
      gsub(/"/, "\\\"")
      if (NR > 1) {
        printf "\\n"
      }
      printf "%s", $0
    }
  ' <<<"$1"
}

json_string() {
  printf '"%s"' "$(json_escape "$1")"
}

file_size_bytes() {
  wc -c <"$1" | tr -d ' '
}

write_artifacts_json() {
  local kernel_version
  kernel_version="$(first_module_version)"
  local desktop_detected
  desktop_detected="$(desktop_stack_detected)"
  local root_tar_sha
  root_tar_sha="$(file_sha256 "$ROOT_TAR")"
  local kernel_sha
  kernel_sha="$(file_sha256 "$KERNEL")"
  local initrd_sha
  initrd_sha="$(file_sha256 "$INITRD")"
  local raw_disk_sha
  raw_disk_sha="$(file_sha256 "$RAW_DISK")"
  local raw_disk_size
  raw_disk_size="$(file_size_bytes "$RAW_DISK")"
  local tmp
  tmp="$(mktemp "$ARTIFACTS_JSON.tmp.XXXXXX")"

  {
    printf '{\n'
    printf '  "schema_version": 1,\n'
    printf '  "guest": { "os": "ubuntu", "arch": "arm64" },\n'
    printf '  "release": %s,\n' "$(json_string "$RELEASE")"
    printf '  "source_family": "ubuntu-cloudimg-arm64-root-tar",\n'
    printf '  "source_format": "root.tar.xz",\n'
    printf '  "base_url": %s,\n' "$(json_string "$BASE_URL")"
    printf '  "root_tar": { "path": %s, "sha256": %s },\n' "$(json_string "$ROOT_TAR")" "$(json_string "$root_tar_sha")"
    printf '  "kernel": { "path": %s, "sha256": %s },\n' "$(json_string "$KERNEL")" "$(json_string "$kernel_sha")"
    printf '  "initrd": { "path": %s, "sha256": %s },\n' "$(json_string "$INITRD")" "$(json_string "$initrd_sha")"
    printf '  "raw_disk": { "path": %s, "sha256": %s, "size_bytes": %s, "format": "raw" },\n' "$(json_string "$RAW_DISK")" "$(json_string "$raw_disk_sha")" "$raw_disk_size"
    printf '  "rootfs_layout": "whole-disk-ext4",\n'
    printf '  "root_device": "/dev/vda",\n'
    printf '  "root_partition": null,\n'
    printf '  "root_label": "cloudimg-rootfs",\n'
    if [[ -n "$kernel_version" ]]; then
      printf '  "kernel_version": %s,\n' "$(json_string "$kernel_version")"
    else
      printf '  "kernel_version": null,\n'
    fi
    printf '  "kernel_command_line": %s,\n' "$(json_string "$KERNEL_CMDLINE")"
    printf '  "desktop_stack_detected": %s,\n' "$desktop_detected"
    printf '  "builder": %s,\n' "$(json_string "${SELECTED_BUILDER:-existing-root.raw}")"
    printf '  "notes": "Ubuntu cloud/server rootfs prepared for Apple Virtualization.framework linux-kernel/raw boot; desktop provisioning is separate."\n'
    printf '}\n'
  } >"$tmp"
  mv "$tmp" "$ARTIFACTS_JSON"
}

print_exports() {
  printf 'export BRIDGEVM_UBUNTU_VZ_KERNEL=%q\n' "$KERNEL"
  printf 'export BRIDGEVM_UBUNTU_VZ_INITRD=%q\n' "$INITRD"
  printf 'export BRIDGEVM_UBUNTU_VZ_RAW_DISK=%q\n' "$RAW_DISK"
  printf 'export BRIDGEVM_UBUNTU_VZ_RAW_DISK_SIZE=%q\n' "$DISK_SIZE"
  printf 'export BRIDGEVM_UBUNTU_VZ_KERNEL_CMDLINE=%q\n' "$KERNEL_CMDLINE"
  printf 'export BRIDGEVM_UBUNTU_VZ_ARTIFACTS_JSON=%q\n' "$ARTIFACTS_JSON"
  printf 'export BRIDGEVM_LIVE_VZ_KERNEL=%q\n' "$KERNEL"
  printf 'export BRIDGEVM_LIVE_VZ_INITRD=%q\n' "$INITRD"
  printf 'export BRIDGEVM_LIVE_VZ_RAW_DISK=%q\n' "$RAW_DISK"
  printf 'export BRIDGEVM_LIVE_VZ_KERNEL_CMDLINE=%q\n' "$KERNEL_CMDLINE"
  printf 'export BRIDGEVM_LIVE_VZ_ARTIFACTS_JSON=%q\n' "$ARTIFACTS_JSON"
  printf 'export BRIDGEVM_LIVE_VZ_SERIAL_EXPECTED=%q\n' "$SERIAL_EXPECTED"
}

if [[ "$DRY_RUN" == "1" ]]; then
  cat <<EOF
# Ubuntu cloud image Apple VZ fixture plan
# release: $RELEASE
# base_url: $BASE_URL
# root_tar_url: $ROOT_URL
# kernel_url: $KERNEL_URL
# initrd_url: $INITRD_URL
# fixture_dir: $FIXTURE_DIR
# artifacts_json: $ARTIFACTS_JSON
# selected_builder: $(select_builder)
# note: this prepares Ubuntu cloud/server rootfs inputs; Desktop package provisioning is separate.
EOF
  print_exports
  exit 0
fi

require_tool mkdir
require_tool cp
require_tool mktemp
require_tool mv
require_tool tar
if [[ -z "$ROOT_TAR_SOURCE" || -z "$KERNEL_SOURCE" || -z "$INITRD_SOURCE" ]]; then
  require_tool curl
fi
if ! have_tool shasum && ! have_tool sha256sum; then
  echo "error: neither shasum nor sha256sum is available to verify downloads" >&2
  exit 1
fi

mkdir -p "$DOWNLOAD_DIR" "$WORK_DIR"
copy_or_download "$ROOT_TAR_SOURCE" "$ROOT_URL" "$ROOT_TAR" "root tar"
copy_or_download "$KERNEL_SOURCE" "$KERNEL_URL" "$KERNEL" "kernel"
copy_or_download "$INITRD_SOURCE" "$INITRD_URL" "$INITRD" "initrd"

if [[ -z "$ROOT_TAR_SOURCE" ]]; then
  download_if_missing "$SHA256SUMS_URL" "$DOWNLOAD_DIR/SHA256SUMS"
  verify_sha256_from_sums "$ROOT_TAR" "$DOWNLOAD_DIR/SHA256SUMS" "$(basename "$ROOT_TAR")" "root tar"
fi
if [[ -z "$KERNEL_SOURCE" || -z "$INITRD_SOURCE" ]]; then
  download_if_missing "$UNPACKED_SHA256SUMS_URL" "$DOWNLOAD_DIR/unpacked-SHA256SUMS"
  [[ -n "$KERNEL_SOURCE" ]] || verify_sha256_from_sums "$KERNEL" "$DOWNLOAD_DIR/unpacked-SHA256SUMS" "$ARTIFACT_PREFIX-vmlinuz-generic" "kernel"
  [[ -n "$INITRD_SOURCE" ]] || verify_sha256_from_sums "$INITRD" "$DOWNLOAD_DIR/unpacked-SHA256SUMS" "$ARTIFACT_PREFIX-initrd-generic" "initrd"
fi

if [[ ! -f "$RAW_DISK" ]]; then
  SELECTED_BUILDER="$(select_builder)"
  case "$SELECTED_BUILDER" in
    mkfs.ext4)
      build_with_mkfs_ext4
      ;;
    virt-make-fs)
      build_with_virt_make_fs
      ;;
    docker)
      build_with_docker
      ;;
    none)
      echo "error: cannot build root.raw; install mkfs.ext4/e2fsprogs, virt-make-fs/libguestfs, or Docker" >&2
      exit 1
      ;;
  esac
fi

[[ -s "$KERNEL" ]] || { echo "error: kernel output is missing or empty: $KERNEL" >&2; exit 1; }
[[ -s "$INITRD" ]] || { echo "error: initrd output is missing or empty: $INITRD" >&2; exit 1; }
[[ -s "$RAW_DISK" ]] || { echo "error: raw disk output is missing or empty: $RAW_DISK" >&2; exit 1; }
write_artifacts_json

cat <<EOF
# Prepared Ubuntu cloud image Apple VZ fixture in $FIXTURE_DIR
# Builder: ${SELECTED_BUILDER:-existing-root.raw}
# This is an Ubuntu cloud/server rootfs tuple. Desktop package provisioning is separate.
# Artifacts: $ARTIFACTS_JSON
EOF
print_exports
