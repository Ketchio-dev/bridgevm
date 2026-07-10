#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-installed-p3-readiness.XXXXXX")"
VENUS="$STORE/venus"
VIRGL="$STORE/virgl"

mkdir -p "$VENUS" "$VIRGL"

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

assert_file_contains() {
  local path="$1"
  local needle="$2"
  local label="$3"
  [[ -f "$path" ]] || fail "$label file missing: $path"
  grep -Fq "$needle" "$path" || fail "$label missing '$needle' in $path"
}

write_package() {
  local dir="$1"
  local protocol="$2"
  local capability="${3:-kmd-only}"

  cat >"$dir/viogpu3d.inf" <<INF
[Manufacturer]
%RedHat% = RedHat,NTarm64

[RedHat.NTarm64]
%VirtioGpu3D.DeviceDesc% = VioGpu3D_Inst, PCI\\VEN_1AF4&DEV_10F7

; BridgeVMProtocol=$protocol
INF
  write_minimal_pe "$dir/viogpu3d.sys" 144 252
  printf 'fake catalog\n' >"$dir/viogpu3d.cat"
  if [[ "$capability" == "registered" ]]; then
    cat >>"$dir/viogpu3d.inf" <<'INF'

[DestinationDirs]
VioGpu3D_Files.Usermode=11

[VioGpu3D_Inst.NT]
CopyFiles=VioGpu3D_Files.Usermode
AddReg=VioGpu3D_DeviceSettings

[VioGpu3D_Files.Usermode]
viogpu_d3d10.dll,viogpu_d3d10_arm64.dll,,0
viogpu_wgl.dll,viogpu_wgl_arm64.dll,,0

[VioGpu3D_DeviceSettings]
HKR,,UserModeDriverName,0x00010000,%11%\viogpu_d3d10.dll,%11%\viogpu_d3d10.dll,%11%\viogpu_d3d10.dll,%11%\viogpu_d3d10.dll
HKR,,OpenGLDriverName,0x00010000,%11%\viogpu_wgl.dll
HKR,,InstalledDisplayDrivers,0x00010000,viogpu_d3d10,viogpu_d3d10,viogpu_d3d10
HKR,,OpenGLVersion,%REG_DWORD%,4096
HKR,,OpenGLFlags,%REG_DWORD%,3
INF
    write_minimal_pe "$dir/viogpu_d3d10_arm64.dll" 144 252
    write_minimal_pe "$dir/viogpu_wgl_arm64.dll" 144 252
  fi
}

write_package "$VENUS" venus
write_package "$VIRGL" virgl registered

source scripts/run-hvf-windows-installed-boot-args.sh
source scripts/run-hvf-windows-installed-boot-runner.sh

EVIDENCE_DIR="$STORE/venus-evidence"
mkdir -p "$EVIDENCE_DIR"
VIRTIO_GPU_3D="1"
VIOGPU3D_DIR="$VENUS"
REQUIRE_VIOGPU3D_READINESS="1"
GPU_TRACE_PROTOCOL="venus"
RUN_STATUS="0"

if write_p3_gpu_readiness; then
  fail "KMD-only Venus package unexpectedly passed render readiness"
fi

[[ "$RUN_STATUS" != "0" ]] || fail "KMD-only Venus readiness did not raise RUN_STATUS"
assert_file_contains "$EVIDENCE_DIR/p3-gpu-readiness.txt" "host_preflight=PASS" "venus readiness"
assert_file_contains "$EVIDENCE_DIR/p3-gpu-readiness.txt" "host_expected_capset_id=4" "venus readiness"
assert_file_contains "$EVIDENCE_DIR/p3-gpu-readiness.txt" "expected_hwid=PCI\\VEN_1AF4&DEV_10F7" "venus readiness"
assert_file_contains "$EVIDENCE_DIR/p3-gpu-readiness.txt" "package_capability=kmd-only" "venus readiness"
assert_file_contains "$EVIDENCE_DIR/p3-gpu-readiness.txt" "render_candidate=false" "venus readiness"
assert_file_contains "$EVIDENCE_DIR/p3-gpu-readiness.txt" "injection-ready but not a render candidate" "venus readiness"
assert_file_contains "$EVIDENCE_DIR/p3-gpu-readiness.txt" "boot_ready=false" "venus readiness"
assert_file_contains "$EVIDENCE_DIR/p3-gpu-readiness.txt" "manifest=$EVIDENCE_DIR/viogpu3d-package-manifest.txt" "venus readiness"
assert_file_contains "$EVIDENCE_DIR/viogpu3d-package-manifest.txt" "protocol=venus" "venus package manifest"
assert_file_contains "$EVIDENCE_DIR/viogpu3d-package-manifest.txt" "expected_hwid=PCI\\VEN_1AF4&DEV_10F7" "venus package manifest"
assert_file_contains "$EVIDENCE_DIR/viogpu3d-package-manifest.txt" $'file=sys\tsha256=' "venus package manifest"

EVIDENCE_DIR="$STORE/virgl-evidence"
mkdir -p "$EVIDENCE_DIR"
VIOGPU3D_DIR="$VIRGL"
GPU_TRACE_PROTOCOL="auto"
RUN_STATUS="0"

if write_p3_gpu_readiness; then
  fail "virgl readiness unexpectedly passed"
fi

[[ "$RUN_STATUS" != "0" ]] || fail "virgl readiness did not raise RUN_STATUS"
assert_file_contains "$EVIDENCE_DIR/p3-gpu-readiness.txt" "driver_protocol=virgl" "virgl readiness"
assert_file_contains "$EVIDENCE_DIR/p3-gpu-readiness.txt" "package_protocol_device_model_preflight=PASS" "virgl readiness"
assert_file_contains "$EVIDENCE_DIR/p3-gpu-readiness.txt" "package_protocol_device_model_expected_capset_id=1" "virgl readiness"
assert_file_contains "$EVIDENCE_DIR/p3-gpu-readiness.txt" "host_backend_protocol=venus" "virgl readiness"
assert_file_contains "$EVIDENCE_DIR/p3-gpu-readiness.txt" "host_backend_virgl_runtime=NOT_WIRED" "virgl readiness"
assert_file_contains "$EVIDENCE_DIR/p3-gpu-readiness.txt" "host_renderer_virgl=NOT_PROBED" "virgl readiness"
assert_file_contains "$EVIDENCE_DIR/p3-gpu-readiness.txt" "driver_render_candidate=true" "virgl readiness"
assert_file_contains "$EVIDENCE_DIR/p3-gpu-readiness.txt" "end_to_end_windows_3d=NOT_PASSED" "virgl readiness"
assert_file_contains "$EVIDENCE_DIR/p3-gpu-readiness.txt" "boot_ready=false" "virgl readiness"
assert_file_contains "$EVIDENCE_DIR/p3-gpu-readiness.txt" "synthetic device-model preflight PASS" "virgl readiness"

EVIDENCE_DIR="$STORE/virgl-wired-evidence"
mkdir -p "$EVIDENCE_DIR"
VIOGPU3D_DIR="$VIRGL"
GPU_TRACE_PROTOCOL="virgl"
RUN_STATUS="0"

write_p3_gpu_readiness || fail "virgl wired readiness unexpectedly failed"

[[ "$RUN_STATUS" == "0" ]] || fail "virgl wired readiness changed RUN_STATUS to $RUN_STATUS"
assert_file_contains "$EVIDENCE_DIR/p3-gpu-readiness.txt" "host_protocol=virgl" "virgl wired readiness"
assert_file_contains "$EVIDENCE_DIR/p3-gpu-readiness.txt" "driver_protocol=virgl" "virgl wired readiness"
assert_file_contains "$EVIDENCE_DIR/p3-gpu-readiness.txt" "host_backend_protocol=virgl" "virgl wired readiness"
assert_file_contains "$EVIDENCE_DIR/p3-gpu-readiness.txt" "host_backend_virgl_runtime=WIRED" "virgl wired readiness"
assert_file_contains "$EVIDENCE_DIR/p3-gpu-readiness.txt" "driver_render_candidate=true" "virgl wired readiness"
assert_file_contains "$EVIDENCE_DIR/p3-gpu-readiness.txt" "end_to_end_windows_3d=NOT_PASSED" "virgl wired readiness"
assert_file_contains "$EVIDENCE_DIR/p3-gpu-readiness.txt" "boot_ready=true" "virgl wired readiness"
assert_file_contains "$EVIDENCE_DIR/p3-gpu-readiness.txt" "status=0" "virgl wired readiness"

EVIDENCE_DIR="$STORE/missing-evidence"
mkdir -p "$EVIDENCE_DIR"
VIOGPU3D_DIR=""
GPU_TRACE_PROTOCOL="venus"
RUN_STATUS="0"

if write_p3_gpu_readiness; then
  fail "missing required package readiness unexpectedly passed"
fi

[[ "$RUN_STATUS" != "0" ]] || fail "missing package readiness did not raise RUN_STATUS"
assert_file_contains "$EVIDENCE_DIR/p3-gpu-readiness.txt" "driver_package=missing" "missing readiness"
assert_file_contains "$EVIDENCE_DIR/p3-gpu-readiness.txt" "missing viogpu3d package" "missing readiness"

echo "PASS: installed Windows P3 readiness runner smoke ($STORE)"
