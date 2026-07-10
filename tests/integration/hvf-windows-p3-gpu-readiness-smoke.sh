#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-p3-gpu-readiness.XXXXXX")"
VENUS="$STORE/venus"
VIRGL="$STORE/virgl"
UNREGISTERED="$STORE/virgl-unregistered"

mkdir -p "$VENUS" "$VIRGL" "$UNREGISTERED"

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
  if [[ "$capability" != "kmd-only" ]]; then
    cat >>"$dir/viogpu3d.inf" <<'INF'

[DestinationDirs]
VioGpu3D_Files.Usermode=11

[VioGpu3D_Inst.NT]
CopyFiles=VioGpu3D_Files.Usermode
AddReg=VioGpu3D_DeviceSettings

[VioGpu3D_Files.Usermode]
viogpu_d3d10.dll,viogpu_d3d10_arm64.dll,,0
viogpu_wgl.dll,viogpu_wgl_arm64.dll,,0
INF
    if [[ "$capability" == "registered" ]]; then
      cat >>"$dir/viogpu3d.inf" <<'INF'

[VioGpu3D_DeviceSettings]
HKR,,UserModeDriverName,0x00010000,%11%\viogpu_d3d10.dll,%11%\viogpu_d3d10.dll,%11%\viogpu_d3d10.dll,%11%\viogpu_d3d10.dll
HKR,,OpenGLDriverName,0x00010000,%11%\viogpu_wgl.dll
HKR,,InstalledDisplayDrivers,0x00010000,viogpu_d3d10,viogpu_d3d10,viogpu_d3d10
HKR,,OpenGLVersion,%REG_DWORD%,4096
HKR,,OpenGLFlags,%REG_DWORD%,3
INF
    fi
    write_minimal_pe "$dir/viogpu_d3d10_arm64.dll" 144 252
    write_minimal_pe "$dir/viogpu_wgl_arm64.dll" 144 252
  fi
}

write_package "$VENUS" venus
write_package "$VIRGL" virgl registered
write_package "$UNREGISTERED" virgl unregistered

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
venus_output="$(scripts/check-hvf-windows-p3-gpu-readiness.sh --driver-dir "$VENUS" --manifest "$VENUS_MANIFEST" 2>&1)" &&
  fail "KMD-only Venus package unexpectedly passed render readiness: $venus_output"

assert_contains "$venus_output" "host_protocol=venus" "venus readiness"
assert_contains "$venus_output" "expected_hwid=PCI\\VEN_1AF4&DEV_10F7" "venus readiness"
assert_contains "$venus_output" "manifest=$VENUS_MANIFEST" "venus readiness"
assert_contains "$venus_output" "package_capability=kmd-only" "venus readiness"
assert_contains "$venus_output" "render_candidate=false" "venus readiness"
assert_contains "$venus_output" "injection-ready but not a render candidate" "venus readiness"
assert_contains "$venus_output" "boot_ready=false" "venus readiness"
assert_file_contains "$VENUS_MANIFEST" "protocol=venus" "venus manifest"
assert_file_contains "$VENUS_MANIFEST" "expected_hwid=PCI\\VEN_1AF4&DEV_10F7" "venus manifest"
assert_file_contains "$VENUS_MANIFEST" $'file=sys\tsha256=' "venus manifest"

id_mismatch_output="$(
  scripts/check-hvf-windows-p3-gpu-readiness.sh --driver-dir "$VENUS" --pci-device-id 1050 2>&1
)" && fail "readiness unexpectedly accepted mismatched DEV_1050: $id_mismatch_output"

assert_contains "$id_mismatch_output" "expected_hwid=PCI\\VEN_1AF4&DEV_1050" "HWID mismatch readiness"
assert_contains "$id_mismatch_output" "does not advertise expected PCI\\VEN_1AF4&DEV_1050" "HWID mismatch readiness"

unregistered_output="$(
  BRIDGEVM_VIRTIO_GPU_3D_PROTOCOL=virgl \
    scripts/check-hvf-windows-p3-gpu-readiness.sh --driver-dir "$UNREGISTERED" 2>&1
)" && fail "unregistered UMD package unexpectedly passed render readiness: $unregistered_output"

assert_contains "$unregistered_output" "package_capability=umd-payload-unregistered" "unregistered readiness"
assert_contains "$unregistered_output" "render_candidate=false" "unregistered readiness"
assert_contains "$unregistered_output" "user-mode-dlls-present-but-inf-registration-missing" "unregistered readiness"
assert_contains "$unregistered_output" "injection-ready but not a render candidate" "unregistered readiness"
assert_contains "$unregistered_output" "boot_ready=false" "unregistered readiness"

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
assert_contains "$virgl_runtime_output" "driver_render_candidate=true" "virgl runtime readiness"
assert_contains "$virgl_runtime_output" "end_to_end_windows_3d=NOT_PASSED" "virgl runtime readiness"
assert_contains "$virgl_runtime_output" "boot_ready=true" "virgl runtime readiness"
assert_contains "$virgl_runtime_output" "PASS: P3 Windows GPU readiness" "virgl runtime readiness"

echo "PASS: Windows P3 GPU readiness smoke ($STORE)"
