#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-p3-gpu-readiness.XXXXXX")"
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

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  case "$haystack" in
    *"$needle"*) ;;
    *) fail "$label missing '$needle'; got: $haystack" ;;
  esac
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

  cat >"$dir/viogpu3d.inf" <<INF
[Manufacturer]
%RedHat% = RedHat,NTarm64

[RedHat.NTarm64]
%VirtioGpu3D.DeviceDesc% = viogpu3d_Device, PCI\\VEN_1AF4&DEV_10F7

; BridgeVMProtocol=$protocol
INF
  write_minimal_pe "$dir/viogpu3d.sys" 144 252
  printf 'fake catalog\n' >"$dir/viogpu3d.cat"
}

write_package "$VENUS" venus
write_package "$VIRGL" virgl

FAKE_VENUS_PROBE="$STORE/fake-venus-host-probe.sh"
cat >"$FAKE_VENUS_PROBE" <<'SH'
#!/usr/bin/env bash
printf 'requested_protocol=venus\n'
printf 'renderer_available=true\n'
printf 'host_renderer_venus=AVAILABLE\n'
printf 'VENUS_CAPSET_OK ver=0 size=160\n'
SH
chmod +x "$FAKE_VENUS_PROBE"

FAKE_VIRGL_PROBE="$STORE/fake-virgl-host-probe.sh"
cat >"$FAKE_VIRGL_PROBE" <<'SH'
#!/usr/bin/env bash
printf 'requested_protocol=virgl\n'
printf 'gl_context_callbacks=cgl-opengl\n'
printf 'renderer_cookie_nonnull=true\n'
printf 'renderer_available=true\n'
printf 'virgl_renderer_init flags=0x4122 ret=0\n'
printf 'host_renderer_virgl=AVAILABLE\n'
printf 'VIRGL_CAPSET_OK ver=1 size=308\n'
SH
chmod +x "$FAKE_VIRGL_PROBE"

host_only_output="$(scripts/check-hvf-windows-p3-gpu-readiness.sh 2>&1)" ||
  fail "host-only readiness failed unexpectedly: $host_only_output"

assert_contains "$host_only_output" "host_preflight=PASS" "host-only readiness"
assert_contains "$host_only_output" "host_expected_capset_id=4" "host-only readiness"
assert_contains "$host_only_output" "driver_package=missing" "host-only readiness"
assert_contains "$host_only_output" "end_to_end_windows_3d=NOT_PASSED" "host-only readiness"
assert_contains "$host_only_output" "boot_ready=false" "host-only readiness"

missing_required_output="$(scripts/check-hvf-windows-p3-gpu-readiness.sh --require-driver-package 2>&1)" &&
  fail "missing required driver package unexpectedly passed: $missing_required_output"

assert_contains "$missing_required_output" "missing viogpu3d package" "missing required readiness"

VENUS_MANIFEST="$STORE/venus-manifest.txt"
venus_output="$(scripts/check-hvf-windows-p3-gpu-readiness.sh --driver-dir "$VENUS" --manifest "$VENUS_MANIFEST" 2>&1)" ||
  fail "venus readiness failed: $venus_output"

assert_contains "$venus_output" "host_protocol=venus" "venus readiness"
assert_contains "$venus_output" "expected_hwid=PCI\\VEN_1AF4&DEV_10F7" "venus readiness"
assert_contains "$venus_output" "manifest=$VENUS_MANIFEST" "venus readiness"
assert_contains "$venus_output" "driver_protocol=venus" "venus readiness"
assert_contains "$venus_output" "recommended_gpu_trace_protocol=venus" "venus readiness"
assert_contains "$venus_output" "package_protocol_device_model_preflight=PASS" "venus readiness"
assert_contains "$venus_output" "package_protocol_device_model_expected_capset_id=4" "venus readiness"
assert_contains "$venus_output" "host_backend_protocol=venus" "venus readiness"
assert_contains "$venus_output" "host_backend_venus_runtime=WIRED" "venus readiness"
assert_contains "$venus_output" "end_to_end_windows_3d=NOT_PASSED" "venus readiness"
assert_contains "$venus_output" "boot_ready=true" "venus readiness"
assert_contains "$venus_output" "PASS: P3 Windows GPU readiness" "venus readiness"
assert_file_contains "$VENUS_MANIFEST" "protocol=venus" "venus manifest"
assert_file_contains "$VENUS_MANIFEST" "expected_hwid=PCI\\VEN_1AF4&DEV_10F7" "venus manifest"
assert_file_contains "$VENUS_MANIFEST" $'file=sys\tsha256=' "venus manifest"

venus_probe_output="$(
  PROBE_HOST_RENDERER=1 \
    VENUS_HOST_RENDERER_PROBE="$FAKE_VENUS_PROBE" \
    scripts/check-hvf-windows-p3-gpu-readiness.sh --driver-dir "$VENUS" 2>&1
)" || fail "venus readiness with host renderer probe failed: $venus_probe_output"

assert_contains "$venus_probe_output" "host_renderer_venus_probe=REQUESTED" "venus probed readiness"
assert_contains "$venus_probe_output" "host_renderer_venus_probe_command=$FAKE_VENUS_PROBE" "venus probed readiness"
assert_contains "$venus_probe_output" "host_renderer_venus_probe_exit_status=0" "venus probed readiness"
assert_contains "$venus_probe_output" "host_renderer_venus=AVAILABLE" "venus probed readiness"
assert_contains "$venus_probe_output" "host_renderer_venus_available=true" "venus probed readiness"
assert_contains "$venus_probe_output" "boot_ready=true" "venus probed readiness"

id_mismatch_output="$(
  scripts/check-hvf-windows-p3-gpu-readiness.sh --driver-dir "$VENUS" --pci-device-id 1050 2>&1
)" && fail "readiness unexpectedly accepted mismatched DEV_1050: $id_mismatch_output"

assert_contains "$id_mismatch_output" "expected_hwid=PCI\\VEN_1AF4&DEV_1050" "HWID mismatch readiness"
assert_contains "$id_mismatch_output" "does not advertise expected PCI\\VEN_1AF4&DEV_1050" "HWID mismatch readiness"

virgl_output="$(scripts/check-hvf-windows-p3-gpu-readiness.sh --driver-dir "$VIRGL" 2>&1)" &&
  fail "virgl readiness unexpectedly passed: $virgl_output"

assert_contains "$virgl_output" "driver_protocol=virgl" "virgl readiness"
assert_contains "$virgl_output" "package_protocol_device_model_preflight=PASS" "virgl readiness"
assert_contains "$virgl_output" "package_protocol_device_model_expected_capset_id=1" "virgl readiness"
assert_contains "$virgl_output" "host_backend_protocol=venus" "virgl readiness"
assert_contains "$virgl_output" "host_backend_virgl_runtime=NOT_WIRED" "virgl readiness"
assert_contains "$virgl_output" "host_renderer_virgl=NOT_PROBED" "virgl readiness"
assert_contains "$virgl_output" "end_to_end_windows_3d=NOT_PASSED" "virgl readiness"
assert_contains "$virgl_output" "boot_ready=false" "virgl readiness"
assert_contains "$virgl_output" "synthetic device-model preflight PASS" "virgl readiness"

virgl_probe_output="$(
  PROBE_HOST_RENDERER=1 \
    VIRGL_HOST_RENDERER_PROBE="$FAKE_VIRGL_PROBE" \
    scripts/check-hvf-windows-p3-gpu-readiness.sh --driver-dir "$VIRGL" 2>&1
)" && fail "virgl readiness with unwired host renderer unexpectedly passed: $virgl_probe_output"

assert_contains "$virgl_probe_output" "driver_protocol=virgl" "virgl probed readiness"
assert_contains "$virgl_probe_output" "package_protocol_device_model_preflight=PASS" "virgl probed readiness"
assert_contains "$virgl_probe_output" "package_protocol_device_model_expected_capset_id=1" "virgl probed readiness"
assert_contains "$virgl_probe_output" "host_backend_protocol=venus" "virgl probed readiness"
assert_contains "$virgl_probe_output" "host_backend_virgl_runtime=NOT_WIRED" "virgl probed readiness"
assert_contains "$virgl_probe_output" "host_renderer_virgl_probe=REQUESTED" "virgl probed readiness"
assert_contains "$virgl_probe_output" "host_renderer_virgl_probe_command=$FAKE_VIRGL_PROBE" "virgl probed readiness"
assert_contains "$virgl_probe_output" "host_renderer_virgl_probe_exit_status=0" "virgl probed readiness"
assert_contains "$virgl_probe_output" "host_renderer_virgl_cookie_nonnull=true" "virgl probed readiness"
assert_contains "$virgl_probe_output" "host_renderer_virgl_gl_context_callbacks=cgl-opengl" "virgl probed readiness"
assert_contains "$virgl_probe_output" "host_renderer_virgl_init=virgl_renderer_init flags=0x4122 ret=0" "virgl probed readiness"
assert_contains "$virgl_probe_output" "host_renderer_virgl=AVAILABLE" "virgl probed readiness"
assert_contains "$virgl_probe_output" "host_renderer_virgl_available=true" "virgl probed readiness"
assert_contains "$virgl_probe_output" "live host renderer" "virgl probed readiness"
assert_contains "$virgl_probe_output" "matching runtime backend is wired" "virgl probed readiness"

virgl_runtime_output="$(
  BRIDGEVM_VIRTIO_GPU_3D_PROTOCOL=virgl \
    PROBE_HOST_RENDERER=1 \
    VIRGL_HOST_RENDERER_PROBE="$FAKE_VIRGL_PROBE" \
    scripts/check-hvf-windows-p3-gpu-readiness.sh --driver-dir "$VIRGL" 2>&1
)" || fail "virgl readiness with wired runtime failed: $virgl_runtime_output"

assert_contains "$virgl_runtime_output" "host_protocol=virgl" "virgl runtime readiness"
assert_contains "$virgl_runtime_output" "driver_protocol=virgl" "virgl runtime readiness"
assert_contains "$virgl_runtime_output" "host_backend_protocol=virgl" "virgl runtime readiness"
assert_contains "$virgl_runtime_output" "host_backend_virgl_runtime=WIRED" "virgl runtime readiness"
assert_contains "$virgl_runtime_output" "host_renderer_virgl=AVAILABLE" "virgl runtime readiness"
assert_contains "$virgl_runtime_output" "end_to_end_windows_3d=NOT_PASSED" "virgl runtime readiness"
assert_contains "$virgl_runtime_output" "boot_ready=true" "virgl runtime readiness"
assert_contains "$virgl_runtime_output" "PASS: P3 Windows GPU readiness" "virgl runtime readiness"

echo "PASS: Windows P3 GPU readiness smoke ($STORE)"
