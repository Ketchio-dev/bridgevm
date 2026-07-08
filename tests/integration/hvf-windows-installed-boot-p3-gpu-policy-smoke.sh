#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-installed-p3-gpu-policy.XXXXXX")"
TARGET="$STORE/windows-target.raw"
VARS="$STORE/vars.fd"
EVIDENCE="$STORE/evidence"
TRACE="$STORE/evidence/p3-gpu.jsonl"
VIOGPU3D="$STORE/viogpu3d"

touch "$TARGET" "$VARS"
mkdir -p "$EVIDENCE" "$VIOGPU3D"

write_minimal_pe() {
  local path="$1"
  local machine_low_octal="$2"
  local machine_high_octal="$3"

  dd if=/dev/zero of="$path" bs=512 count=1 >/dev/null 2>&1
  printf 'MZ' | dd of="$path" bs=1 seek=0 conv=notrunc >/dev/null 2>&1
  printf '\200\000\000\000' | dd of="$path" bs=1 seek=60 conv=notrunc >/dev/null 2>&1
  printf "PE\000\000\\$machine_low_octal\\$machine_high_octal" |
    dd of="$path" bs=1 seek=128 conv=notrunc >/dev/null 2>&1
}

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

cat >"$VIOGPU3D/viogpu3d.inf" <<'INF'
[Manufacturer]
%RedHat% = RedHat,NTarm64

[RedHat.NTarm64]
%VirtioGpu3D.DeviceDesc% = viogpu3d_Device, PCI\VEN_1AF4&DEV_10F7

; BridgeVMProtocol=venus
INF
write_minimal_pe "$VIOGPU3D/viogpu3d.sys" 144 252
printf 'fake catalog\n' >"$VIOGPU3D/viogpu3d.cat"

output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --virtio-gpu-3d \
    --gpu-trace "$TRACE" \
    --gpu-trace-protocol venus \
    --require-gpu-trace-gate \
    --viogpu3d-dir "$VIOGPU3D" \
    --require-viogpu3d-readiness \
    --print-policy 2>&1
)" || fail "installed boot P3 GPU policy failed: $output"

assert_contains "$output" "BRIDGEVM_VIRTIO_GPU_3D=1" "installed boot policy"
assert_contains "$output" "BRIDGEVM_VIRTIO_GPU_3D_PROTOCOL=venus" "installed boot policy"
assert_contains "$output" "BRIDGEVM_VIRTIO_GPU=1" "installed boot policy"
assert_contains "$output" "BRIDGEVM_VIRTIO_GPU_3D_BIND_ID=1" "installed boot policy"
assert_contains "$output" "BRIDGEVM_VIRTIO_GPU_PCI_DEVICE_ID=" "installed boot policy"
assert_contains "$output" "BRIDGEVM_VIRTIO_GPU_TRACE_JSONL=$TRACE" "installed boot policy"
assert_contains "$output" "BRIDGEVM_GPU_TRACE_PROTOCOL=venus" "installed boot policy"
assert_contains "$output" "BRIDGEVM_REQUIRE_GPU_TRACE_GATE=1" "installed boot policy"
assert_contains "$output" "BRIDGEVM_VIOGPU3D_DIR=$VIOGPU3D" "installed boot policy"
assert_contains "$output" "BRIDGEVM_REQUIRE_VIOGPU3D_READINESS=1" "installed boot policy"
assert_contains "$output" "BUILD_PROFILE=debug" "installed boot policy"
assert_contains "$output" "BRIDGEVM_NVME_DISK_WRITABLE=1 when booting target as only NVMe" "installed boot policy"

explicit_id_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --virtio-gpu-3d \
    --virtio-gpu-device-id 1050 \
    --print-policy 2>&1
)" || fail "installed boot explicit GPU PCI ID policy failed: $explicit_id_output"

assert_contains "$explicit_id_output" "BRIDGEVM_VIRTIO_GPU_3D_BIND_ID=<unset> (explicit device id 0x1050)" "installed boot explicit PCI ID policy"
assert_contains "$explicit_id_output" "BRIDGEVM_VIRTIO_GPU_3D_PROTOCOL=venus" "installed boot explicit PCI ID policy"
assert_contains "$explicit_id_output" "BRIDGEVM_VIRTIO_GPU_PCI_DEVICE_ID=0x1050" "installed boot explicit PCI ID policy"

id_without_gpu_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --virtio-gpu-device-id 1050 \
    --print-policy 2>&1
)" && fail "installed boot unexpectedly accepted GPU PCI ID without --virtio-gpu-3d: $id_without_gpu_output"

assert_contains "$id_without_gpu_output" "--virtio-gpu-device-id requires --virtio-gpu-3d" "GPU PCI ID without GPU policy"

virgl_output="$(
  scripts/run-hvf-windows-installed-boot.sh \
    --target "$TARGET" \
    --vars "$VARS" \
    --evidence-dir "$EVIDENCE" \
    --virtio-gpu-3d \
    --gpu-trace-protocol virgl \
    --print-policy 2>&1
)" || fail "installed boot unexpectedly rejected virgl protocol: $virgl_output"

assert_contains "$virgl_output" "BRIDGEVM_VIRTIO_GPU_3D_PROTOCOL=virgl" "virgl policy"
assert_contains "$virgl_output" "BRIDGEVM_GPU_TRACE_PROTOCOL=virgl" "virgl policy"

echo "PASS: installed Windows P3 GPU policy smoke ($STORE)"
