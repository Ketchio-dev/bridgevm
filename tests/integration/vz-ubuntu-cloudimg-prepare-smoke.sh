#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-vz-ubuntu-cloudimg-prepare.XXXXXX")"
SOURCE_DIR="$STORE/source"
ROOTFS_DIR="$STORE/rootfs"
FIXTURE_DIR="$STORE/fixture"
STAGE_STORE="$STORE/stage-store"
FAKE_BIN="$STORE/bin"
BACKEND_LOG="$STORE/backend-launch.log"
MKFS_LOG="$STORE/mkfs-ext4.log"
VM_NAME="try-ubuntu-cloudimg-vz"
BUNDLE="$STAGE_STORE/vms/$VM_NAME.vmbridge"
LAUNCH_SPEC="$BUNDLE/metadata/apple-vz-launch.json"
ARTIFACTS_JSON="$FIXTURE_DIR/artifacts.json"

fail() {
  echo "FAIL: $*" >&2
  echo "Store preserved at $STORE" >&2
  exit 1
}

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  case "$haystack" in
    *"$needle"*) ;;
    *) fail "$label missing '$needle'; got: $haystack" ;;
  esac
}

assert_not_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  case "$haystack" in
    *"$needle"*) fail "$label unexpectedly included '$needle'; got: $haystack" ;;
    *) ;;
  esac
}

assert_file_contains() {
  local file="$1"
  local needle="$2"
  local label="$3"
  [[ -f "$file" ]] || fail "$label missing file $file"
  if ! grep -Fq "$needle" "$file"; then
    fail "$label missing '$needle' in $file"
  fi
}

mkdir -p "$SOURCE_DIR" "$ROOTFS_DIR/etc" "$ROOTFS_DIR/lib/modules/6.8.0-100-generic" "$FAKE_BIN"
cat >"$ROOTFS_DIR/etc/os-release" <<'EOF'
NAME="Ubuntu"
ID=ubuntu
VERSION_ID="24.04"
EOF
printf 'fake module metadata\n' >"$ROOTFS_DIR/lib/modules/6.8.0-100-generic/modules.dep"
tar -C "$ROOTFS_DIR" -cJf "$SOURCE_DIR/noble-server-cloudimg-arm64-root.tar.xz" .
printf 'fake Ubuntu cloudimg arm64 kernel\n' >"$SOURCE_DIR/vmlinuz"
printf 'fake Ubuntu cloudimg arm64 initrd\n' >"$SOURCE_DIR/initrd"

for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
  cat >"$FAKE_BIN/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in Ubuntu cloudimg prepare smoke: $(basename "$0")" >&2
exit 99
SH
  chmod +x "$FAKE_BIN/$backend"
done

cat >"$FAKE_BIN/mkfs.ext4" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

args=("$@")
raw_disk="${args[$((${#args[@]} - 2))]}"
disk_size="${args[$((${#args[@]} - 1))]}"
rootfs=""
for index in "${!args[@]}"; do
  if [[ "${args[$index]}" == "-d" ]]; then
    rootfs="${args[$((index + 1))]}"
  fi
done

[[ -n "$rootfs" ]] || { echo "missing -d rootfs" >&2; exit 2; }
[[ -f "$rootfs/etc/os-release" ]] || { echo "rootfs missing os-release" >&2; exit 2; }
[[ -d "$rootfs/lib/modules/6.8.0-100-generic" ]] || { echo "rootfs missing matching modules" >&2; exit 2; }

{
  printf 'mkfs.ext4 args: %s\n' "$*"
  printf 'rootfs: %s\n' "$rootfs"
  printf 'raw_disk: %s\n' "$raw_disk"
  printf 'disk_size: %s\n' "$disk_size"
} >"${BRIDGEVM_FAKE_MKFS_LOG:?}"

printf 'fake raw ext4 disk generated from %s size %s\n' "$rootfs" "$disk_size" >"$raw_disk"
SH
chmod +x "$FAKE_BIN/mkfs.ext4"

dry_output="$(
  PATH="$FAKE_BIN:$PATH" \
  BRIDGEVM_FAKE_BACKEND_LOG="$BACKEND_LOG" \
  scripts/prepare-vz-ubuntu-cloudimg-fixture.sh \
    --dry-run \
    --fixture-dir "$FIXTURE_DIR"
)"

assert_contains "$dry_output" "https://cloud-images.ubuntu.com/noble/current/noble-server-cloudimg-arm64-root.tar.xz" "dry-run"
assert_contains "$dry_output" "https://cloud-images.ubuntu.com/noble/current/unpacked/noble-server-cloudimg-arm64-vmlinuz-generic" "dry-run"
assert_contains "$dry_output" "artifacts_json: $ARTIFACTS_JSON" "dry-run"
assert_contains "$dry_output" "BRIDGEVM_UBUNTU_VZ_ARTIFACTS_JSON" "dry-run"
assert_contains "$dry_output" "root=/dev/vda" "dry-run"
assert_not_contains "$dry_output" "root=/dev/vda2" "dry-run"

prepare_output="$(
  PATH="$FAKE_BIN:$PATH" \
  BRIDGEVM_FAKE_BACKEND_LOG="$BACKEND_LOG" \
  BRIDGEVM_FAKE_MKFS_LOG="$MKFS_LOG" \
  scripts/prepare-vz-ubuntu-cloudimg-fixture.sh \
    --fixture-dir "$FIXTURE_DIR" \
    --root-tar "$SOURCE_DIR/noble-server-cloudimg-arm64-root.tar.xz" \
    --kernel "$SOURCE_DIR/vmlinuz" \
    --initrd "$SOURCE_DIR/initrd" \
    --disk-size 8M \
    --builder mkfs.ext4
)"

assert_contains "$prepare_output" "Prepared Ubuntu cloud image Apple VZ fixture" "prepare output"
assert_contains "$prepare_output" "Builder: mkfs.ext4" "prepare output"
assert_contains "$prepare_output" "Desktop package provisioning is separate" "prepare output"
assert_contains "$prepare_output" "BRIDGEVM_UBUNTU_VZ_KERNEL_CMDLINE=console=hvc0\\ root=/dev/vda\\ rw" "prepare output"
assert_not_contains "$prepare_output" "systemd.unit=graphical.target" "prepare output"
assert_not_contains "$prepare_output" "root=/dev/vda2" "prepare output"

[[ -f "$FIXTURE_DIR/vmlinuz" ]] || fail "kernel was not prepared"
[[ -f "$FIXTURE_DIR/initrd" ]] || fail "initrd was not prepared"
[[ -s "$FIXTURE_DIR/root.raw" ]] || fail "root.raw was not prepared"
assert_file_contains "$FIXTURE_DIR/vmlinuz" "fake Ubuntu cloudimg arm64 kernel" "prepared kernel"
assert_file_contains "$FIXTURE_DIR/initrd" "fake Ubuntu cloudimg arm64 initrd" "prepared initrd"
assert_file_contains "$MKFS_LOG" "disk_size: 8M" "fake mkfs"
assert_file_contains "$MKFS_LOG" "rootfs:" "fake mkfs"

assert_file_contains "$ARTIFACTS_JSON" '"source_family": "ubuntu-cloudimg-arm64-root-tar"' "artifacts"
assert_file_contains "$ARTIFACTS_JSON" '"source_format": "root.tar.xz"' "artifacts"
assert_file_contains "$ARTIFACTS_JSON" '"rootfs_layout": "whole-disk-ext4"' "artifacts"
assert_file_contains "$ARTIFACTS_JSON" '"root_device": "/dev/vda"' "artifacts"
assert_file_contains "$ARTIFACTS_JSON" '"root_partition": null' "artifacts"
assert_file_contains "$ARTIFACTS_JSON" '"kernel_version": "6.8.0-100-generic"' "artifacts"
assert_file_contains "$ARTIFACTS_JSON" '"kernel_command_line": "console=hvc0 root=/dev/vda rw"' "artifacts"
assert_file_contains "$ARTIFACTS_JSON" '"desktop_stack_detected": false' "artifacts"
assert_file_contains "$ARTIFACTS_JSON" '"builder": "mkfs.ext4"' "artifacts"

stage_output="$(
  PATH="$FAKE_BIN:$PATH" \
  BRIDGEVM_FAKE_BACKEND_LOG="$BACKEND_LOG" \
  scripts/stage-vz-ubuntu-desktop-vm.sh \
    --store "$STAGE_STORE" \
    --name "$VM_NAME" \
    --fixture-dir "$FIXTURE_DIR" \
    --kernel-command-line "console=hvc0 root=/dev/vda rw" \
    --disk 8M
)"

assert_contains "$stage_output" "Ubuntu Apple VZ Desktop VM staged: $VM_NAME" "stage output"
assert_contains "$stage_output" "Launch ready: true" "stage output"
assert_contains "$stage_output" "Command: lightvm-runner --launch-spec $LAUNCH_SPEC" "stage output"
assert_not_contains "$stage_output" "systemd.unit=graphical.target" "stage output"
assert_not_contains "$stage_output" "root=/dev/vda2" "stage output"

assert_file_contains "$BUNDLE/manifest.yaml" "kernelCommandLine: console=hvc0 root=/dev/vda rw" "staged manifest"
assert_file_contains "$LAUNCH_SPEC" '"os": "ubuntu"' "launch spec"
assert_file_contains "$LAUNCH_SPEC" '"mode": "linux-kernel"' "launch spec"
assert_file_contains "$LAUNCH_SPEC" '"format": "raw"' "launch spec"
assert_file_contains "$LAUNCH_SPEC" '"ready": true' "launch spec"
assert_file_contains "$LAUNCH_SPEC" 'root=/dev/vda' "launch spec"

if [[ -s "$BACKEND_LOG" ]]; then
  fail "backend or GUI launch attempted: $(cat "$BACKEND_LOG")"
fi

echo "PASS: Ubuntu cloudimg Apple VZ prepare smoke ($STORE)"
