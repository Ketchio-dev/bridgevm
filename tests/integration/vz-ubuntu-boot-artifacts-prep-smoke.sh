#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-vz-ubuntu-boot-artifacts.XXXXXX")"
FAKE_BIN="$STORE/bin"
SOURCE_IMAGE="$STORE/noble-server-cloudimg-arm64.img"
OUTPUT_DIR="$STORE/artifacts"
STAGE_STORE="$STORE/stage-store"
VM_NAME="try-ubuntu-boot-artifacts-vz"
BUNDLE="$STAGE_STORE/vms/$VM_NAME.vmbridge"
LAUNCH_SPEC="$BUNDLE/metadata/apple-vz-launch.json"
BACKEND_LOG="$STORE/backend-launch.log"
QEMU_IMG_LOG="$STORE/qemu-img.log"
DOCKER_LOG="$STORE/docker.log"
ARTIFACTS_JSON="$OUTPUT_DIR/artifacts.json"

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
  if ! grep -Fq -- "$needle" "$file"; then
    fail "$label missing '$needle' in $file"
  fi
}

mkdir -p "$FAKE_BIN"
printf 'fake qcow2 ubuntu image\n' >"$SOURCE_IMAGE"

for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
  cat >"$FAKE_BIN/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in Ubuntu boot-artifacts prep smoke: $(basename "$0")" >&2
exit 99
SH
  chmod +x "$FAKE_BIN/$backend"
done

cat >"$FAKE_BIN/qemu-img" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf 'qemu-img %s\n' "$*" >>"${BRIDGEVM_FAKE_QEMU_IMG_LOG:?}"

case "${1:-}" in
  info)
    path="${@: -1}"
    if [[ "$path" == *root.raw ]]; then
      cat <<'JSON'
{
  "filename": "root.raw",
  "format": "raw",
  "virtual-size": 8589934592
}
JSON
    else
      cat <<'JSON'
{
  "filename": "noble-server-cloudimg-arm64.img",
  "format": "qcow2",
  "virtual-size": 3758096384
}
JSON
    fi
    ;;
  convert)
    [[ "${2:-}" == "-O" && "${3:-}" == "raw" ]] || {
      echo "unexpected convert args: $*" >&2
      exit 2
    }
    source="${4:?missing source}"
    destination="${5:?missing destination}"
    printf 'fake raw disk converted from %s\n' "$source" >"$destination"
    ;;
  resize)
    [[ "${2:-}" == "-f" && "${3:-}" == "raw" ]] || {
      echo "unexpected resize args: $*" >&2
      exit 2
    }
    printf 'resized to %s\n' "${5:?missing size}" >>"${4:?missing raw disk}"
    ;;
  *)
    echo "unexpected qemu-img command: $*" >&2
    exit 2
    ;;
esac
SH
chmod +x "$FAKE_BIN/qemu-img"

cat >"$FAKE_BIN/docker" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf 'docker %s\n' "$*" >>"${BRIDGEVM_FAKE_DOCKER_LOG:?}"

if [[ "${1:-}" == "image" && "${2:-}" == "inspect" ]]; then
  printf '[{"Id":"sha256:fake-ubuntu"}]\n'
  exit 0
fi

work=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    -v)
      mount_spec="${2:?missing volume value}"
      if [[ "$mount_spec" == *":/work" ]]; then
        work="${mount_spec%:/work}"
      fi
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done

[[ -n "$work" ]] || {
  echo "fake docker did not receive an output-dir:/work mount" >&2
  exit 2
}
[[ -f "$work/root.raw" ]] || {
  echo "fake docker expected root.raw in $work" >&2
  exit 2
}

printf 'fake extracted Ubuntu kernel from root.raw\n' | gzip -c >"$work/vmlinuz"
printf 'fake extracted Ubuntu initrd from root.raw\n' >"$work/initrd"
cat >"$work/extraction.json" <<'JSON'
{
  "root_device_host": "/dev/loop7p1",
  "root_partition": "1",
  "root_uuid": "11111111-2222-3333-4444-555555555555",
  "root_label": "cloudimg-rootfs",
  "root_fstype": "ext4",
  "kernel_version": "6.8.0-100-generic",
  "kernel_path_in_guest": "/boot/vmlinuz-6.8.0-100-generic",
  "initrd_path_in_guest": "/boot/initrd.img-6.8.0-100-generic",
  "modules_match": true,
  "desktop_stack_detected": false
}
JSON
SH
chmod +x "$FAKE_BIN/docker"

dry_output="$(
  PATH="$FAKE_BIN:$PATH" \
  BRIDGEVM_FAKE_BACKEND_LOG="$BACKEND_LOG" \
  BRIDGEVM_FAKE_QEMU_IMG_LOG="$QEMU_IMG_LOG" \
  BRIDGEVM_FAKE_DOCKER_LOG="$DOCKER_LOG" \
  scripts/prepare-vz-ubuntu-arm64-boot-artifacts.sh \
    --dry-run \
    --source-image "$SOURCE_IMAGE" \
    --output-dir "$OUTPUT_DIR"
)"

assert_contains "$dry_output" "Ubuntu Arm64 Apple VZ boot artifact plan" "dry-run"
assert_contains "$dry_output" "source_format: qcow2" "dry-run"
assert_contains "$dry_output" "selected_backend: docker-offline" "dry-run"
assert_contains "$dry_output" "qemu_img_use: offline inspection/conversion only" "dry-run"
assert_contains "$dry_output" "qemu_system_runtime: false" "dry-run"

prepare_output="$(
  PATH="$FAKE_BIN:$PATH" \
  BRIDGEVM_FAKE_BACKEND_LOG="$BACKEND_LOG" \
  BRIDGEVM_FAKE_QEMU_IMG_LOG="$QEMU_IMG_LOG" \
  BRIDGEVM_FAKE_DOCKER_LOG="$DOCKER_LOG" \
  scripts/prepare-vz-ubuntu-arm64-boot-artifacts.sh \
    --source-image "$SOURCE_IMAGE" \
    --output-dir "$OUTPUT_DIR" \
    --disk-size 8G \
    --prep-backend docker-offline
)"

assert_contains "$prepare_output" "Prepared Ubuntu Arm64 Apple VZ boot artifacts" "prepare output"
assert_contains "$prepare_output" "Source format: qcow2" "prepare output"
assert_contains "$prepare_output" "Backend: docker-offline" "prepare output"
assert_contains "$prepare_output" "Kernel transform: gzip-decompressed" "prepare output"
assert_contains "$prepare_output" "root=UUID=11111111-2222-3333-4444-555555555555" "prepare output"
assert_not_contains "$prepare_output" "root=/dev/vda2" "prepare output"
assert_not_contains "$prepare_output" "systemd.unit=graphical.target" "prepare output"

[[ -s "$OUTPUT_DIR/root.raw" ]] || fail "root.raw was not prepared"
assert_file_contains "$OUTPUT_DIR/vmlinuz" "fake extracted Ubuntu kernel" "prepared kernel"
assert_file_contains "$OUTPUT_DIR/initrd" "fake extracted Ubuntu initrd" "prepared initrd"
assert_file_contains "$QEMU_IMG_LOG" "qemu-img info --output=json $SOURCE_IMAGE" "qemu-img log"
assert_file_contains "$QEMU_IMG_LOG" "qemu-img convert -O raw $SOURCE_IMAGE $OUTPUT_DIR/root.raw" "qemu-img log"
assert_file_contains "$QEMU_IMG_LOG" "qemu-img resize -f raw $OUTPUT_DIR/root.raw 8G" "qemu-img log"
assert_file_contains "$DOCKER_LOG" "--privileged" "docker log"
assert_file_contains "$DOCKER_LOG" "image inspect ubuntu:24.04" "docker log"

assert_file_contains "$ARTIFACTS_JSON" '"format": "qcow2"' "artifacts"
assert_file_contains "$ARTIFACTS_JSON" '"apple_vz_transform": "gzip-decompressed"' "artifacts"
assert_file_contains "$ARTIFACTS_JSON" '"qemu_img_offline_only": true' "artifacts"
assert_file_contains "$ARTIFACTS_JSON" '"qemu_system_used": false' "artifacts"
assert_file_contains "$ARTIFACTS_JSON" '"apple_vz_started": false' "artifacts"
assert_file_contains "$ARTIFACTS_JSON" '"gui_spawned": false' "artifacts"
assert_file_contains "$ARTIFACTS_JSON" '"root_partition": "1"' "artifacts"
assert_file_contains "$ARTIFACTS_JSON" '"root_uuid": "11111111-2222-3333-4444-555555555555"' "artifacts"
assert_file_contains "$ARTIFACTS_JSON" '"modules_match": true' "artifacts"
assert_file_contains "$ARTIFACTS_JSON" '"desktop_stack_detected": false' "artifacts"
assert_file_contains "$ARTIFACTS_JSON" '"kernel_command_line": "console=hvc0 root=UUID=11111111-2222-3333-4444-555555555555 rw"' "artifacts"

kernel_cmdline="$(jq -r '.kernel_command_line' "$ARTIFACTS_JSON")"
stage_output="$(
  PATH="$FAKE_BIN:$PATH" \
  BRIDGEVM_FAKE_BACKEND_LOG="$BACKEND_LOG" \
  scripts/stage-vz-ubuntu-desktop-vm.sh \
    --store "$STAGE_STORE" \
    --name "$VM_NAME" \
    --fixture-dir "$OUTPUT_DIR" \
    --kernel-command-line "$kernel_cmdline" \
    --disk 8G
)"

assert_contains "$stage_output" "Ubuntu Apple VZ Desktop VM staged: $VM_NAME" "stage output"
assert_contains "$stage_output" "Launch ready: true" "stage output"
assert_contains "$stage_output" "Command: lightvm-runner --launch-spec $LAUNCH_SPEC" "stage output"
assert_not_contains "$stage_output" "root=/dev/vda2" "stage output"
assert_file_contains "$BUNDLE/manifest.yaml" "kernelCommandLine: $kernel_cmdline" "manifest"
assert_file_contains "$LAUNCH_SPEC" '"mode": "linux-kernel"' "launch spec"
assert_file_contains "$LAUNCH_SPEC" '"format": "raw"' "launch spec"
assert_file_contains "$LAUNCH_SPEC" '"ready": true' "launch spec"
assert_file_contains "$LAUNCH_SPEC" "$kernel_cmdline" "launch spec"

if [[ -s "$BACKEND_LOG" ]]; then
  fail "backend or GUI launch attempted: $(cat "$BACKEND_LOG")"
fi

echo "PASS: Ubuntu boot-artifacts Apple VZ prep smoke ($STORE)"
