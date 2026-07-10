#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-viogpu3d-package-check.XXXXXX")"
VIOGPU3D="$STORE/viogpu3d"
VIOGPU3D_1050="$STORE/viogpu3d-1050"
UNKNOWN="$STORE/unknown"
UNREGISTERED="$STORE/virgl-full-unregistered"
REGISTERED="$STORE/virgl-full-registered"
ORPHAN_REGISTERED="$STORE/virgl-full-orphan-registered"
MISSING_REGISTERED_DLL="$STORE/virgl-full-missing-registered-dll"
WRONG_UMD_DESTINATION="$STORE/virgl-full-wrong-umd-destination"
TRUNCATED_UMD_LISTS="$STORE/virgl-full-truncated-umd-lists"
PROVENANCE_MANIFEST="$STORE/provenance-manifest.txt"
UNREGISTERED_MANIFEST="$STORE/unregistered-manifest.txt"

mkdir -p \
  "$VIOGPU3D" \
  "$VIOGPU3D_1050" \
  "$UNKNOWN" \
  "$UNREGISTERED" \
  "$REGISTERED" \
  "$ORPHAN_REGISTERED" \
  "$MISSING_REGISTERED_DLL" \
  "$WRONG_UMD_DESTINATION" \
  "$TRUNCATED_UMD_LISTS"

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
  local protocol_marker="$2"
  local pci_device_id="${3:-10F7}"
  local install_section="${4:-VioGpu3D_Inst}"

  cat >"$dir/viogpu3d.inf" <<INF
[Manufacturer]
%RedHat% = RedHat,NTarm64

[RedHat.NTarm64]
%VirtioGpu3D.DeviceDesc% = $install_section, PCI\\VEN_1AF4&DEV_$pci_device_id

; BridgeVMProtocol=$protocol_marker
INF
  write_minimal_pe "$dir/viogpu3d.sys" 144 252
  printf 'fake catalog\n' >"$dir/viogpu3d.cat"
}

add_umd_payload() {
  local dir="$1"
  local registration="$2"
  local source_mode="${3:-complete}"
  local dll
  local wgl_source="viogpu_wgl_arm64.dll"
  local destination_dirid="11"
  if [[ "$source_mode" == "missing-registered-dll" ]]; then
    wgl_source="missing_viogpu_wgl_arm64.dll"
  elif [[ "$source_mode" == "wrong-destination" ]]; then
    destination_dirid="13"
  fi

  printf '\n[DestinationDirs]\nVioGpu3D_Files.Usermode=%s\n' \
    "$destination_dirid" >>"$dir/viogpu3d.inf"
  cat >>"$dir/viogpu3d.inf" <<'INF'

[VioGpu3D_Inst.NT]
CopyFiles=VioGpu3D_Files.Usermode
AddReg=VioGpu3D_DeviceSettings

[VioGpu3D_Files.Usermode]
opengl32.dll,opengl32_arm64.dll,,0
viogpu_d3d10.dll,viogpu_d3d10_arm64.dll,,0
libEGL.dll,libEGL_arm64.dll,,0
INF
  printf 'viogpu_wgl.dll,%s,,0\n' "$wgl_source" >>"$dir/viogpu3d.inf"
  printf 'libGLESv2.dll,libGLESv2_arm64.dll,,0\n' >>"$dir/viogpu3d.inf"
  if [[ "$registration" == "registered" ]]; then
    cat >>"$dir/viogpu3d.inf" <<'INF'

[VioGpu3D_DeviceSettings]
HKR,,UserModeDriverName,0x00010000,%11%\viogpu_d3d10.dll,%11%\viogpu_d3d10.dll,%11%\viogpu_d3d10.dll,%11%\viogpu_d3d10.dll
HKR,,OpenGLDriverName,0x00010000,%11%\viogpu_wgl.dll
HKR,,InstalledDisplayDrivers,0x00010000,viogpu_d3d10,viogpu_d3d10,viogpu_d3d10
HKR,,OpenGLVersion,%REG_DWORD%,4096
HKR,,OpenGLFlags,%REG_DWORD%,3
INF
  elif [[ "$registration" == "truncated" ]]; then
    cat >>"$dir/viogpu3d.inf" <<'INF'

[VioGpu3D_DeviceSettings]
HKR,,UserModeDriverName,0x00010000,%11%\viogpu_d3d10.dll
HKR,,OpenGLDriverName,0x00010000,%11%\viogpu_wgl.dll
HKR,,InstalledDisplayDrivers,0x00010000,viogpu_d3d10
HKR,,OpenGLVersion,%REG_DWORD%,4096
HKR,,OpenGLFlags,%REG_DWORD%,3
INF
  fi
  for dll in \
    opengl32_arm64.dll \
    viogpu_d3d10_arm64.dll \
    libEGL_arm64.dll \
    viogpu_wgl_arm64.dll \
    libGLESv2_arm64.dll
  do
    write_minimal_pe "$dir/$dll" 144 252
  done
}

MANIFEST="$STORE/viogpu3d-manifest.txt"

write_package "$VIOGPU3D" virgl
write_package "$VIOGPU3D_1050" virgl 1050
write_package "$UNKNOWN" audited
write_package "$UNREGISTERED" virgl 1050
write_package "$REGISTERED" virgl 1050
write_package "$ORPHAN_REGISTERED" virgl 1050 Other_Inst
write_package "$MISSING_REGISTERED_DLL" virgl 1050
write_package "$WRONG_UMD_DESTINATION" virgl 1050
write_package "$TRUNCATED_UMD_LISTS" virgl 1050
add_umd_payload "$UNREGISTERED" unregistered
add_umd_payload "$REGISTERED" registered
add_umd_payload "$ORPHAN_REGISTERED" registered
add_umd_payload "$MISSING_REGISTERED_DLL" registered missing-registered-dll
add_umd_payload "$WRONG_UMD_DESTINATION" registered wrong-destination
add_umd_payload "$TRUNCATED_UMD_LISTS" truncated

output="$(
  VIOGPU3D_SOURCE_REPO=https://example.invalid/viogpu3d.git \
  VIOGPU3D_SOURCE_REF=deadbeef \
  VIOGPU3D_BUILD_ID=test-build \
  VIOGPU3D_SIGNING_CERT=test-cert \
    scripts/check-hvf-windows-viogpu3d-package.sh --manifest "$MANIFEST" "$VIOGPU3D" 2>&1
)" ||
  fail "viogpu3d package check failed: $output"

assert_contains "$output" "BridgeVM viogpu3d package check" "package check"
assert_contains "$output" "protocol=virgl" "package check"
assert_contains "$output" "protocol_source=auto" "package check"
assert_contains "$output" "hwids=PCI\\VEN_1AF4&DEV_10F7" "package check"
assert_contains "$output" "manifest=$MANIFEST" "package check"
assert_contains "$output" "PASS: viogpu3d package is injection-ready" "package check"
assert_contains "$output" "package_capability=kmd-only" "package check"
assert_contains "$output" "render_candidate=false" "package check"
assert_contains "$output" "render_candidate_reason=no-user-mode-dll-payload" "package check"
assert_file_contains "$MANIFEST" "BridgeVM viogpu3d package manifest" "package manifest"
assert_file_contains "$MANIFEST" "source_repo=https://example.invalid/viogpu3d.git" "package manifest"
assert_file_contains "$MANIFEST" "source_ref=deadbeef" "package manifest"
assert_file_contains "$MANIFEST" "build_id=test-build" "package manifest"
assert_file_contains "$MANIFEST" "signing_cert=test-cert" "package manifest"
assert_file_contains "$MANIFEST" "protocol=virgl" "package manifest"
assert_file_contains "$MANIFEST" "package_capability=kmd-only" "package manifest"
assert_file_contains "$MANIFEST" "render_candidate=false" "package manifest"
assert_file_contains "$MANIFEST" "hwids=PCI\\VEN_1AF4&DEV_10F7" "package manifest"
assert_file_contains "$MANIFEST" $'file=sys\tsha256=' "package manifest"
assert_file_contains "$MANIFEST" "pe_machine=0xaa64" "package manifest"
assert_file_contains "$MANIFEST" $'file=cat\tsha256=' "package manifest"

unknown_output="$(scripts/check-hvf-windows-viogpu3d-package.sh "$UNKNOWN" 2>&1)" &&
  fail "unknown protocol package unexpectedly passed: $unknown_output"

assert_contains "$unknown_output" "could not identify viogpu3d protocol" "unknown package check"

cat >"$UNKNOWN/bridgevm-package-provenance.env" <<EOF
VIOGPU3D_SOURCE_REPO=https://example.invalid/provenance.git
VIOGPU3D_SOURCE_REF=feedface
VIOGPU3D_BUILD_ID=provenance-build
VIOGPU3D_SIGNING_CERT=provenance-cert
VIOGPU3D_PROTOCOL=virgl
VIOGPU3D_PCI_DEVICE_ID=10f7
EOF

provenance_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh --manifest "$PROVENANCE_MANIFEST" "$UNKNOWN" 2>&1
)" || fail "provenance package check failed: $provenance_output"

assert_contains "$provenance_output" "provenance=$UNKNOWN/bridgevm-package-provenance.env" "provenance package check"
assert_contains "$provenance_output" "source_repo=https://example.invalid/provenance.git" "provenance package check"
assert_contains "$provenance_output" "source_ref=feedface" "provenance package check"
assert_contains "$provenance_output" "build_id=provenance-build" "provenance package check"
assert_contains "$provenance_output" "signing_cert=provenance-cert" "provenance package check"
assert_contains "$provenance_output" "expected_hwid=PCI\\VEN_1AF4&DEV_10F7" "provenance package check"
assert_contains "$provenance_output" "protocol=virgl" "provenance package check"
assert_contains "$provenance_output" "protocol_source=provenance" "provenance package check"
assert_file_contains "$PROVENANCE_MANIFEST" "provenance=$UNKNOWN/bridgevm-package-provenance.env" "provenance manifest"
assert_file_contains "$PROVENANCE_MANIFEST" "source_repo=https://example.invalid/provenance.git" "provenance manifest"
assert_file_contains "$PROVENANCE_MANIFEST" "source_ref=feedface" "provenance manifest"
assert_file_contains "$PROVENANCE_MANIFEST" "protocol=virgl" "provenance manifest"

conflict_output="$(
  VIOGPU3D_PROTOCOL=venus scripts/check-hvf-windows-viogpu3d-package.sh "$VIOGPU3D" 2>&1
)" && fail "conflicting protocol override unexpectedly passed: $conflict_output"

assert_contains "$conflict_output" "conflicts with package scan protocol=virgl" "conflict package check"

override_output="$(
  VIOGPU3D_PROTOCOL=venus scripts/check-hvf-windows-viogpu3d-package.sh "$UNKNOWN" 2>&1
)" || fail "manual protocol override failed: $override_output"

assert_contains "$override_output" "protocol=venus" "override package check"
assert_contains "$override_output" "protocol_source=env" "override package check"

id1050_output="$(scripts/check-hvf-windows-viogpu3d-package.sh --pci-device-id 1050 "$VIOGPU3D_1050" 2>&1)" ||
  fail "DEV_1050 package check failed: $id1050_output"

assert_contains "$id1050_output" "hwids=PCI\\VEN_1AF4&DEV_1050" "DEV_1050 package check"
assert_contains "$id1050_output" "expected_hwid=PCI\\VEN_1AF4&DEV_1050" "DEV_1050 package check"

id_mismatch_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh --pci-device-id 10f7 "$VIOGPU3D_1050" 2>&1
)" && fail "DEV_1050 package unexpectedly matched DEV_10F7: $id_mismatch_output"

assert_contains "$id_mismatch_output" "does not advertise expected PCI\\VEN_1AF4&DEV_10F7" "HWID mismatch package check"

unregistered_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh \
    --manifest "$UNREGISTERED_MANIFEST" \
    "$UNREGISTERED" 2>&1
)" || fail "unregistered UMD package should remain injection-ready: $unregistered_output"

assert_contains "$unregistered_output" "dll_count=5" "unregistered UMD package"
assert_contains "$unregistered_output" "package_capability=umd-payload-unregistered" "unregistered UMD package"
assert_contains "$unregistered_output" "umd_registration=absent" "unregistered UMD package"
assert_contains "$unregistered_output" "umd_user_mode_driver_name_registered=false" "unregistered UMD package"
assert_contains "$unregistered_output" "umd_open_gl_driver_name_registered=false" "unregistered UMD package"
assert_contains "$unregistered_output" "umd_installed_display_drivers_registered=false" "unregistered UMD package"
assert_contains "$unregistered_output" "render_candidate=false" "unregistered UMD package"
assert_contains "$unregistered_output" "user-mode DLL payload is present but required INF UMD registration is incomplete" "unregistered UMD package"
assert_contains "$unregistered_output" "PASS: viogpu3d package is injection-ready" "unregistered UMD package"
assert_file_contains "$UNREGISTERED_MANIFEST" "dll_count=5" "unregistered UMD manifest"
assert_file_contains "$UNREGISTERED_MANIFEST" "package_capability=umd-payload-unregistered" "unregistered UMD manifest"
assert_file_contains "$UNREGISTERED_MANIFEST" "render_candidate=false" "unregistered UMD manifest"

unregistered_required_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh \
    --require-render-candidate \
    "$UNREGISTERED" 2>&1
)" && fail "unregistered UMD package unexpectedly passed the render gate: $unregistered_required_output"

assert_contains "$unregistered_required_output" "injection-ready but not a render candidate" "unregistered UMD render gate"
assert_contains "$unregistered_required_output" "user-mode-dlls-present-but-inf-registration-missing" "unregistered UMD render gate"

registered_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh \
    --require-render-candidate \
    "$REGISTERED" 2>&1
)" || fail "registered UMD package failed the render gate: $registered_output"

assert_contains "$registered_output" "dll_count=5" "registered UMD package"
assert_contains "$registered_output" "package_capability=umd-registered" "registered UMD package"
assert_contains "$registered_output" "umd_registration=complete" "registered UMD package"
assert_contains "$registered_output" "render_candidate=true" "registered UMD package"
assert_contains "$registered_output" "umd_registered_dlls_resolved=true" "registered UMD package"
assert_contains "$registered_output" "umd_open_gl_version_registered=true" "registered UMD package"
assert_contains "$registered_output" "umd_open_gl_flags_registered=true" "registered UMD package"
assert_contains "$registered_output" "umd_registration_inf=$REGISTERED/viogpu3d.inf" "registered UMD package"
assert_contains "$registered_output" "umd_registration_model_section=redhat.ntarm64" "registered UMD package"
assert_contains "$registered_output" "umd_registration_install_section=viogpu3d_inst.nt" "registered UMD package"
assert_contains "$registered_output" "PASS: viogpu3d package is injection-ready" "registered UMD package"

orphan_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh \
    --require-render-candidate \
    "$ORPHAN_REGISTERED" 2>&1
)" && fail "orphan UMD registration unexpectedly passed the render gate: $orphan_output"

assert_contains "$orphan_output" "umd_user_mode_driver_name_registered=false" "orphan UMD registration"
assert_contains "$orphan_output" "umd_open_gl_driver_name_registered=false" "orphan UMD registration"
assert_contains "$orphan_output" "umd_installed_display_drivers_registered=false" "orphan UMD registration"
assert_contains "$orphan_output" "umd_registered_dlls_resolved=false" "orphan UMD registration"
assert_contains "$orphan_output" "umd_open_gl_version_registered=false" "orphan UMD registration"
assert_contains "$orphan_output" "umd_open_gl_flags_registered=false" "orphan UMD registration"
assert_contains "$orphan_output" "render_candidate=false" "orphan UMD registration"
assert_contains "$orphan_output" "user-mode-dlls-present-but-inf-registration-missing" "orphan UMD registration"

missing_dll_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh \
    --require-render-candidate \
    "$MISSING_REGISTERED_DLL" 2>&1
)" && fail "missing registered DLL unexpectedly passed the render gate: $missing_dll_output"

assert_contains "$missing_dll_output" "umd_user_mode_driver_name_registered=true" "missing registered DLL"
assert_contains "$missing_dll_output" "umd_open_gl_driver_name_registered=true" "missing registered DLL"
assert_contains "$missing_dll_output" "umd_installed_display_drivers_registered=true" "missing registered DLL"
assert_contains "$missing_dll_output" "umd_registered_dlls_resolved=false" "missing registered DLL"
assert_contains "$missing_dll_output" "package_capability=umd-registration-dll-payload-unresolved" "missing registered DLL"
assert_contains "$missing_dll_output" "render_candidate_reason=active-inf-registration-dll-payload-unresolved" "missing registered DLL"
assert_contains "$missing_dll_output" "render_candidate=false" "missing registered DLL"

wrong_destination_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh \
    --require-render-candidate \
    "$WRONG_UMD_DESTINATION" 2>&1
)" && fail "wrong UMD destination unexpectedly passed the render gate: $wrong_destination_output"

assert_contains "$wrong_destination_output" "umd_user_mode_driver_name_registered=true" "wrong UMD destination"
assert_contains "$wrong_destination_output" "umd_open_gl_driver_name_registered=true" "wrong UMD destination"
assert_contains "$wrong_destination_output" "umd_installed_display_drivers_registered=true" "wrong UMD destination"
assert_contains "$wrong_destination_output" "umd_open_gl_version_registered=true" "wrong UMD destination"
assert_contains "$wrong_destination_output" "umd_open_gl_flags_registered=true" "wrong UMD destination"
assert_contains "$wrong_destination_output" "umd_registered_dlls_resolved=false" "wrong UMD destination"
assert_contains "$wrong_destination_output" "render_candidate_reason=active-inf-registration-dll-payload-unresolved" "wrong UMD destination"
assert_contains "$wrong_destination_output" "render_candidate=false" "wrong UMD destination"

truncated_lists_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh \
    --require-render-candidate \
    "$TRUNCATED_UMD_LISTS" 2>&1
)" && fail "truncated UMD registration lists unexpectedly passed the render gate: $truncated_lists_output"

assert_contains "$truncated_lists_output" "umd_user_mode_driver_name_registered=true" "truncated UMD registration lists"
assert_contains "$truncated_lists_output" "umd_open_gl_driver_name_registered=true" "truncated UMD registration lists"
assert_contains "$truncated_lists_output" "umd_installed_display_drivers_registered=true" "truncated UMD registration lists"
assert_contains "$truncated_lists_output" "umd_registered_dlls_resolved=false" "truncated UMD registration lists"
assert_contains "$truncated_lists_output" "render_candidate_reason=active-inf-registration-dll-payload-unresolved" "truncated UMD registration lists"
assert_contains "$truncated_lists_output" "render_candidate=false" "truncated UMD registration lists"

echo "PASS: viogpu3d package check smoke ($STORE)"
