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
PIPE_DELIMITER_DECOY="$STORE/virgl-full-pipe-delimiter-decoy"
MISSING_ACTIVE_PAYLOAD="$STORE/virgl-full-missing-active-payload"
ROOT_DECOY_SUBDIR="$STORE/virgl-full-root-decoy-subdir"
MIXED_CASE_REGISTERED_DLL="$STORE/virgl-full-mixed-case-registered-dll"
AT_SOURCE_DECOY="$STORE/virgl-full-at-source-decoy"
STRING_TOKEN_SOURCE="$STORE/virgl-full-string-token-source"
DUPLICATE_REGISTERED_TARGET="$STORE/virgl-full-duplicate-registered-target"
REPLACEONLY_REGISTERED_DLL="$STORE/virgl-full-replaceonly-registered-dll"
MISSING_ANCILLARY_DESTINATION="$STORE/virgl-full-missing-ancillary-destination"
UNRESOLVED_ANCILLARY_DIRID="$STORE/virgl-full-unresolved-ancillary-dirid"
NONCANONICAL_ANCILLARY_DIRID="$STORE/virgl-full-noncanonical-ancillary-dirid"
NUMERIC_ANCILLARY_DIRID="$STORE/virgl-full-numeric-ancillary-dirid"
MIXED_DIRECT_COPYFILES="$STORE/virgl-full-mixed-direct-copyfiles"
INCLUDE_NEEDS_COPYFILES="$STORE/virgl-full-include-needs-copyfiles"
INCLUDE_ONLY="$STORE/virgl-full-include-only"
NEAR_PREFIX_HWID="$STORE/near-prefix-hwid"
ACTIVE_NEAR_PREFIX_HWID="$STORE/virgl-full-active-near-prefix-hwid"
NONFLAT_SOURCE_DISK_NAME="$STORE/virgl-full-nonflat-source-disk-name"
PERCENT_SEMICOLON_DISK_NAME="$STORE/virgl-full-percent-semicolon-disk-name"
PERCENT_COMMA_DISK_NAME="$STORE/virgl-full-percent-comma-disk-name"
PERCENT_ESCAPE_DISK_NAME="$STORE/virgl-full-percent-escape-disk-name"
UNBALANCED_PERCENT_DISK_NAME="$STORE/virgl-full-unbalanced-percent-disk-name"
NONNUMERIC_DISK_ID="$STORE/virgl-full-nonnumeric-disk-id"
LEADING_ZERO_DISK_ID="$STORE/virgl-full-leading-zero-disk-id"
OVERSIZED_DISK_ID="$STORE/virgl-full-oversized-disk-id"
DUPLICATE_SOURCE_MAPPING="$STORE/virgl-full-duplicate-source-mapping"
EXPANDED_STRING_DELIMITER="$STORE/virgl-full-expanded-string-delimiter"
UNDEFINED_STRING_TOKEN="$STORE/virgl-full-undefined-string-token"
LOCALIZED_STRING_OVERRIDE="$STORE/virgl-full-localized-string-override"
DUPLICATE_STRING_DEFINITION="$STORE/virgl-full-duplicate-string-definition"
UNQUOTED_DISK_DESCRIPTION="$STORE/virgl-full-unquoted-disk-description"
AMBIGUOUS_MODEL_PATH="$STORE/virgl-full-ambiguous-model-path"
DUPLICATE_MODEL_ENTRY="$STORE/virgl-full-duplicate-model-entry"
DUPLICATE_DESTINATION="$STORE/virgl-full-duplicate-destination"
MULTIPLE_MATCHING_INFS="$STORE/virgl-full-multiple-matching-infs"
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
  "$PIPE_DELIMITER_DECOY" \
  "$MISSING_ACTIVE_PAYLOAD" \
  "$ROOT_DECOY_SUBDIR" \
  "$MIXED_CASE_REGISTERED_DLL" \
  "$AT_SOURCE_DECOY" \
  "$STRING_TOKEN_SOURCE" \
  "$DUPLICATE_REGISTERED_TARGET" \
  "$REPLACEONLY_REGISTERED_DLL" \
  "$MISSING_ANCILLARY_DESTINATION" \
  "$UNRESOLVED_ANCILLARY_DIRID" \
  "$NONCANONICAL_ANCILLARY_DIRID" \
  "$NUMERIC_ANCILLARY_DIRID" \
  "$MIXED_DIRECT_COPYFILES" \
  "$INCLUDE_NEEDS_COPYFILES" \
  "$INCLUDE_ONLY" \
  "$NEAR_PREFIX_HWID" \
  "$ACTIVE_NEAR_PREFIX_HWID" \
  "$NONFLAT_SOURCE_DISK_NAME" \
  "$PERCENT_SEMICOLON_DISK_NAME" \
  "$PERCENT_COMMA_DISK_NAME" \
  "$PERCENT_ESCAPE_DISK_NAME" \
  "$UNBALANCED_PERCENT_DISK_NAME" \
  "$NONNUMERIC_DISK_ID" \
  "$LEADING_ZERO_DISK_ID" \
  "$OVERSIZED_DISK_ID" \
  "$DUPLICATE_SOURCE_MAPPING" \
  "$EXPANDED_STRING_DELIMITER" \
  "$UNDEFINED_STRING_TOKEN" \
  "$LOCALIZED_STRING_OVERRIDE" \
  "$DUPLICATE_STRING_DEFINITION" \
  "$UNQUOTED_DISK_DESCRIPTION" \
  "$AMBIGUOUS_MODEL_PATH" \
  "$DUPLICATE_MODEL_ENTRY" \
  "$DUPLICATE_DESTINATION" \
  "$MULTIPLE_MATCHING_INFS" \
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
  local manufacturer_decorations="${5:-NTarm64}"
  local model_variant="${6:-single}"
  local pci_vendor_id="${7:-1AF4}"
  local extra_model_line=""
  if [[ "$model_variant" == "duplicate" ]]; then
    extra_model_line='%VirtioGpu3D.SecondDesc% = Other_Inst, PCI\VEN_1AF4&DEV_1050'
  fi

  cat >"$dir/viogpu3d.inf" <<INF
[Manufacturer]
%RedHat% = RedHat,$manufacturer_decorations

[RedHat.NTarm64]
%VirtioGpu3D.DeviceDesc% = $install_section, PCI\\VEN_$pci_vendor_id&DEV_$pci_device_id
$extra_model_line

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
  local wgl_copy_flags="0"
  local destination_dirid="11"
  local copyfiles_value="VioGpu3D_Files.Usermode"
  local default_destination=""
  local extra_destination=""
  local extra_install_directives=""
  local ancillary_payload=0
  local runtime_source_subdir=""
  local source_disk_id="1"
  local source_disk_name='1="fixture disk",,,'
  if [[ "$source_mode" == "missing-registered-dll" ]]; then
    wgl_source="missing_viogpu_wgl_arm64.dll"
  elif [[ "$source_mode" == "mixed-case-invalid-registered-dll" ]]; then
    wgl_source="viogpu_wgl_arm64.DlL"
  elif [[ "$source_mode" == "at-source-decoy" ]]; then
    wgl_source="@viogpu_wgl_arm64.dll"
  elif [[ "$source_mode" == "string-token-source" ]]; then
    wgl_source="%wgl_source%viogpu_wgl_arm64.dll"
  elif [[ "$source_mode" == "replaceonly-registered-dll" ]]; then
    wgl_copy_flags="0x00000400"
  elif [[ "$source_mode" == "missing-ancillary-destination" ]]; then
    copyfiles_value="VioGpu3D_Files.Usermode,VioGpu3D_Files.Ancillary"
    ancillary_payload=1
  elif [[ "$source_mode" == "unresolved-ancillary-dirid" ]]; then
    copyfiles_value="VioGpu3D_Files.Usermode,VioGpu3D_Files.Ancillary"
    extra_destination='VioGpu3D_Files.Ancillary=%MISSING_DIRID%'
    ancillary_payload=1
  elif [[ "$source_mode" == "noncanonical-ancillary-dirid" ]]; then
    copyfiles_value="VioGpu3D_Files.Usermode,VioGpu3D_Files.Ancillary"
    extra_destination='VioGpu3D_Files.Ancillary=013'
    ancillary_payload=1
  elif [[ "$source_mode" == "numeric-ancillary-dirid" ]]; then
    copyfiles_value="VioGpu3D_Files.Usermode,VioGpu3D_Files.Ancillary"
    extra_destination='VioGpu3D_Files.Ancillary=13'
    ancillary_payload=1
  elif [[ "$source_mode" == "mixed-direct-copyfiles" ]]; then
    copyfiles_value="VioGpu3D_Files.Usermode,@direct_runtime.json"
    default_destination="DefaultDestDir=11"
  elif [[ "$source_mode" == "include-needs-copyfiles" ]]; then
    extra_install_directives=$'Include=extra.inf\nNeeds=ExtraInstall'
  elif [[ "$source_mode" == "include-only" ]]; then
    extra_install_directives='Include=msdv.inf'
  elif [[ "$source_mode" == "nonflat-source-disk-name" ]]; then
    source_disk_name='1="fixture disk",missing.cab,,,0x10,"missing.tag"'
  elif [[ "$source_mode" == "percent-semicolon-disk-name" ]]; then
    source_disk_name='1=%disk;description%,,,missing-base'
  elif [[ "$source_mode" == "percent-comma-disk-name" ]]; then
    source_disk_name='1=%disk,,,,,,description%,,,missing-base'
  elif [[ "$source_mode" == "percent-escape-disk-name" ]]; then
    source_disk_name='1=%disk%%description%,,,'
  elif [[ "$source_mode" == "unbalanced-percent-disk-name" ]]; then
    source_disk_name='1=%disk,,,missing-base'
  elif [[ "$source_mode" == "nonnumeric-disk-id" ]]; then
    source_disk_id="diskid"
    source_disk_name='diskid="fixture disk",,,'
  elif [[ "$source_mode" == "leading-zero-disk-id" ]]; then
    source_disk_id="01"
    source_disk_name='01="fixture disk",,,'
  elif [[ "$source_mode" == "oversized-disk-id" ]]; then
    source_disk_id="4294967296"
    source_disk_name='4294967296="fixture disk",,,'
  elif [[ "$source_mode" == "expanded-string-delimiter" ]]; then
    source_disk_name='1=%disk_description%,,missing-base'
  elif [[ "$source_mode" == "undefined-string-token" ]]; then
    source_disk_name='1=%undefined_disk_description%,,,'
  elif [[ "$source_mode" == "localized-string-override" ||
          "$source_mode" == "duplicate-string-definition" ]]; then
    source_disk_name='1=%disk_description%,,,'
  elif [[ "$source_mode" == "unquoted-disk-description" ]]; then
    source_disk_name='1=fixture disk,,,'
  elif [[ "$source_mode" == "duplicate-destination" ]]; then
    destination_dirid="11,BadSubdir"
    extra_destination='  viogpu3d_files.usermode =11'
  elif [[ "$source_mode" == "wrong-destination" ]]; then
    destination_dirid="13"
  fi

  printf '\n[DestinationDirs]\nVioGpu3D_Files.Usermode=%s\n%s\n' \
    "$destination_dirid" "$default_destination" >>"$dir/viogpu3d.inf"
  [[ -z "$extra_destination" ]] || printf '%s\n' "$extra_destination" >>"$dir/viogpu3d.inf"
  cat >>"$dir/viogpu3d.inf" <<'INF'

[VioGpu3D_Inst.NT]
INF
  printf 'CopyFiles=%s\n' "$copyfiles_value" >>"$dir/viogpu3d.inf"
  [[ -z "$extra_install_directives" ]] || printf '%s\n' "$extra_install_directives" >>"$dir/viogpu3d.inf"
  cat >>"$dir/viogpu3d.inf" <<'INF'
AddReg=VioGpu3D_DeviceSettings

[VioGpu3D_Files.Usermode]
opengl32.dll,opengl32_arm64.dll,,2
viogpu_d3d10.dll,viogpu_d3d10_arm64.dll,,0
libEGL.dll,libEGL_arm64.dll,,0
INF
  printf 'viogpu_wgl.dll,%s,,%s\n' "$wgl_source" "$wgl_copy_flags" >>"$dir/viogpu3d.inf"
  printf 'libGLESv2.dll,libGLESv2_arm64.dll,,0\n' >>"$dir/viogpu3d.inf"
  printf 'viogpu_runtime.json,"runtime source.json",,0\n' >>"$dir/viogpu3d.inf"
  printf '{}\n' >"$dir/runtime source.json"
  if [[ "$source_mode" == "missing-active-payload" ]]; then
    printf 'lvp_icd.aarch64.json,missing_lvp_icd.aarch64.json,,0\n' >>"$dir/viogpu3d.inf"
  elif [[ "$source_mode" == "duplicate-registered-target" ]]; then
    printf 'viogpu_wgl.dll,"runtime source.json",,0\n' >>"$dir/viogpu3d.inf"
  elif (( ancillary_payload == 1 )); then
    cat >>"$dir/viogpu3d.inf" <<'INF'

[VioGpu3D_Files.Ancillary]
ancillary.json,ancillary.json,,0
INF
    printf '{}\n' >"$dir/ancillary.json"
  elif [[ "$source_mode" == "mixed-direct-copyfiles" ]]; then
    printf '{}\n' >"$dir/direct_runtime.json"
  fi
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
  if [[ "$source_mode" == "root-decoy-subdir" ]]; then
    runtime_source_subdir='payload\arm64'
  fi
  printf '\n[SourceDisksNames.arm64]\n%s\n\n[SourceDisksFiles.arm64]\n' \
    "$source_disk_name" >>"$dir/viogpu3d.inf"
  printf 'opengl32_arm64.dll=%s,,\n' "$source_disk_id" >>"$dir/viogpu3d.inf"
  printf 'viogpu_d3d10_arm64.dll=%s,,\n' "$source_disk_id" >>"$dir/viogpu3d.inf"
  printf 'libEGL_arm64.dll=%s,,\n' "$source_disk_id" >>"$dir/viogpu3d.inf"
  printf '%s=%s,,\n' "$wgl_source" "$source_disk_id" >>"$dir/viogpu3d.inf"
  printf 'libGLESv2_arm64.dll=%s,,\n' "$source_disk_id" >>"$dir/viogpu3d.inf"
  printf '"runtime source.json"=%s,%s,\n' "$source_disk_id" "$runtime_source_subdir" >>"$dir/viogpu3d.inf"
  if [[ "$source_mode" == "duplicate-source-mapping" ]]; then
    printf 'opengl32_arm64.dll=%s,,\n' "$source_disk_id" >>"$dir/viogpu3d.inf"
  fi
  if [[ "$source_mode" == "missing-active-payload" ]]; then
    printf 'missing_lvp_icd.aarch64.json=%s,,\n' "$source_disk_id" >>"$dir/viogpu3d.inf"
  elif (( ancillary_payload == 1 )); then
    printf 'ancillary.json=%s,,\n' "$source_disk_id" >>"$dir/viogpu3d.inf"
  elif [[ "$source_mode" == "mixed-direct-copyfiles" ]]; then
    printf 'direct_runtime.json=%s,,\n' "$source_disk_id" >>"$dir/viogpu3d.inf"
  fi
  if [[ "$source_mode" == "percent-semicolon-disk-name" ]]; then
    printf '\n[Strings]\ndisk;description="fixture disk"\n' >>"$dir/viogpu3d.inf"
  elif [[ "$source_mode" == "percent-comma-disk-name" ]]; then
    printf '\n[Strings]\ndisk,,,,,,description="fixture disk"\n' >>"$dir/viogpu3d.inf"
  elif [[ "$source_mode" == "percent-escape-disk-name" ]]; then
    printf '\n[Strings]\ndisk="fixture"\ndescription=" disk"\n' >>"$dir/viogpu3d.inf"
  elif [[ "$source_mode" == "expanded-string-delimiter" ]]; then
    printf '\n[Strings]\ndisk_description="fixture,disk"\n' >>"$dir/viogpu3d.inf"
  elif [[ "$source_mode" == "localized-string-override" ]]; then
    printf '\n[Strings]\ndisk_description="fixture disk"\n\n[Strings.0409]\ndisk_description="localized fixture disk"\n' >>"$dir/viogpu3d.inf"
  elif [[ "$source_mode" == "duplicate-string-definition" ]]; then
    printf '\n[Strings]\ndisk_description="fixture disk"\nDisk_Description="fixture disk"\n' >>"$dir/viogpu3d.inf"
  fi
  for dll in \
    opengl32_arm64.dll \
    viogpu_d3d10_arm64.dll \
    libEGL_arm64.dll \
    viogpu_wgl_arm64.dll \
    libGLESv2_arm64.dll
  do
    if [[ ( "$source_mode" == "mixed-case-invalid-registered-dll" || "$source_mode" == "string-token-source" ) && "$dll" == "viogpu_wgl_arm64.dll" ]]; then
      continue
    fi
    write_minimal_pe "$dir/$dll" 144 252
  done
  if [[ "$source_mode" == "mixed-case-invalid-registered-dll" ]]; then
    printf 'not a PE image\n' >"$dir/$wgl_source"
  elif [[ "$source_mode" == "string-token-source" ]]; then
    write_minimal_pe "$dir/$wgl_source" 144 252
  fi
}

MANIFEST="$STORE/viogpu3d-manifest.txt"

write_package "$VIOGPU3D" virgl
write_package "$VIOGPU3D_1050" virgl 1050
write_package "$UNKNOWN" audited
write_package "$UNREGISTERED" virgl 1050
write_package "$REGISTERED" virgl 1050
write_package "$ORPHAN_REGISTERED" virgl 1050 Other_Inst
write_package "$MISSING_REGISTERED_DLL" virgl 1050
write_package "$PIPE_DELIMITER_DECOY" virgl 1050
write_package "$MISSING_ACTIVE_PAYLOAD" virgl 1050
write_package "$ROOT_DECOY_SUBDIR" virgl 1050
write_package "$MIXED_CASE_REGISTERED_DLL" virgl 1050
write_package "$AT_SOURCE_DECOY" virgl 1050
write_package "$STRING_TOKEN_SOURCE" virgl 1050
write_package "$DUPLICATE_REGISTERED_TARGET" virgl 1050
write_package "$REPLACEONLY_REGISTERED_DLL" virgl 1050
write_package "$MISSING_ANCILLARY_DESTINATION" virgl 1050
write_package "$UNRESOLVED_ANCILLARY_DIRID" virgl 1050
write_package "$NONCANONICAL_ANCILLARY_DIRID" virgl 1050
write_package "$NUMERIC_ANCILLARY_DIRID" virgl 1050
write_package "$MIXED_DIRECT_COPYFILES" virgl 1050
write_package "$INCLUDE_NEEDS_COPYFILES" virgl 1050
write_package "$INCLUDE_ONLY" virgl 1050
write_package "$NEAR_PREFIX_HWID" virgl 10500
write_package "$ACTIVE_NEAR_PREFIX_HWID" virgl 1050 VioGpu3D_Inst NTarm64 single 1AF40
write_package "$NONFLAT_SOURCE_DISK_NAME" virgl 1050
write_package "$PERCENT_SEMICOLON_DISK_NAME" virgl 1050
write_package "$PERCENT_COMMA_DISK_NAME" virgl 1050
write_package "$PERCENT_ESCAPE_DISK_NAME" virgl 1050
write_package "$UNBALANCED_PERCENT_DISK_NAME" virgl 1050
write_package "$NONNUMERIC_DISK_ID" virgl 1050
write_package "$LEADING_ZERO_DISK_ID" virgl 1050
write_package "$OVERSIZED_DISK_ID" virgl 1050
write_package "$DUPLICATE_SOURCE_MAPPING" virgl 1050
write_package "$EXPANDED_STRING_DELIMITER" virgl 1050
write_package "$UNDEFINED_STRING_TOKEN" virgl 1050
write_package "$LOCALIZED_STRING_OVERRIDE" virgl 1050
write_package "$DUPLICATE_STRING_DEFINITION" virgl 1050
write_package "$UNQUOTED_DISK_DESCRIPTION" virgl 1050
write_package "$AMBIGUOUS_MODEL_PATH" virgl 1050 VioGpu3D_Inst 'NTarm64,NTarm64.10.0'
write_package "$DUPLICATE_MODEL_ENTRY" virgl 1050 VioGpu3D_Inst NTarm64 duplicate
write_package "$DUPLICATE_DESTINATION" virgl 1050
write_package "$MULTIPLE_MATCHING_INFS" virgl 1050
write_package "$WRONG_UMD_DESTINATION" virgl 1050
write_package "$TRUNCATED_UMD_LISTS" virgl 1050
printf '\n[RedHat.NTarm64.10.0]\n' >>"$AMBIGUOUS_MODEL_PATH/viogpu3d.inf"
add_umd_payload "$UNREGISTERED" unregistered
add_umd_payload "$REGISTERED" registered
add_umd_payload "$ORPHAN_REGISTERED" registered
add_umd_payload "$MISSING_REGISTERED_DLL" registered missing-registered-dll
add_umd_payload "$PIPE_DELIMITER_DECOY" registered missing-registered-dll
write_minimal_pe "$PIPE_DELIMITER_DECOY/decoy|missing_viogpu_wgl_arm64.dll" 144 252
add_umd_payload "$MISSING_ACTIVE_PAYLOAD" registered missing-active-payload
add_umd_payload "$ROOT_DECOY_SUBDIR" registered root-decoy-subdir
add_umd_payload "$MIXED_CASE_REGISTERED_DLL" registered mixed-case-invalid-registered-dll
add_umd_payload "$AT_SOURCE_DECOY" registered at-source-decoy
add_umd_payload "$STRING_TOKEN_SOURCE" registered string-token-source
add_umd_payload "$DUPLICATE_REGISTERED_TARGET" registered duplicate-registered-target
add_umd_payload "$REPLACEONLY_REGISTERED_DLL" registered replaceonly-registered-dll
add_umd_payload "$MISSING_ANCILLARY_DESTINATION" registered missing-ancillary-destination
add_umd_payload "$UNRESOLVED_ANCILLARY_DIRID" registered unresolved-ancillary-dirid
add_umd_payload "$NONCANONICAL_ANCILLARY_DIRID" registered noncanonical-ancillary-dirid
add_umd_payload "$NUMERIC_ANCILLARY_DIRID" registered numeric-ancillary-dirid
add_umd_payload "$MIXED_DIRECT_COPYFILES" registered mixed-direct-copyfiles
add_umd_payload "$INCLUDE_NEEDS_COPYFILES" registered include-needs-copyfiles
add_umd_payload "$INCLUDE_ONLY" registered include-only
add_umd_payload "$ACTIVE_NEAR_PREFIX_HWID" registered
add_umd_payload "$NONFLAT_SOURCE_DISK_NAME" registered nonflat-source-disk-name
add_umd_payload "$PERCENT_SEMICOLON_DISK_NAME" registered percent-semicolon-disk-name
add_umd_payload "$PERCENT_COMMA_DISK_NAME" registered percent-comma-disk-name
add_umd_payload "$PERCENT_ESCAPE_DISK_NAME" registered percent-escape-disk-name
add_umd_payload "$UNBALANCED_PERCENT_DISK_NAME" registered unbalanced-percent-disk-name
add_umd_payload "$NONNUMERIC_DISK_ID" registered nonnumeric-disk-id
add_umd_payload "$LEADING_ZERO_DISK_ID" registered leading-zero-disk-id
add_umd_payload "$OVERSIZED_DISK_ID" registered oversized-disk-id
add_umd_payload "$DUPLICATE_SOURCE_MAPPING" registered duplicate-source-mapping
add_umd_payload "$EXPANDED_STRING_DELIMITER" registered expanded-string-delimiter
add_umd_payload "$UNDEFINED_STRING_TOKEN" registered undefined-string-token
add_umd_payload "$LOCALIZED_STRING_OVERRIDE" registered localized-string-override
add_umd_payload "$DUPLICATE_STRING_DEFINITION" registered duplicate-string-definition
add_umd_payload "$UNQUOTED_DISK_DESCRIPTION" registered unquoted-disk-description
add_umd_payload "$AMBIGUOUS_MODEL_PATH" registered
add_umd_payload "$DUPLICATE_MODEL_ENTRY" registered
add_umd_payload "$DUPLICATE_DESTINATION" registered duplicate-destination
add_umd_payload "$MULTIPLE_MATCHING_INFS" registered
add_umd_payload "$WRONG_UMD_DESTINATION" registered wrong-destination
add_umd_payload "$TRUNCATED_UMD_LISTS" truncated
cat >"$MULTIPLE_MATCHING_INFS/shadow.inf" <<'INF'
[Manufacturer]
%RedHat% = RedHat,NTarm64

[RedHat.NTarm64]
%VirtioGpu3D.DeviceDesc% = Missing_Inst, PCI\VEN_1AF4&DEV_1050
INF
cat >"$INCLUDE_NEEDS_COPYFILES/extra.inf" <<'INF'
[DestinationDirs]
ExtraFiles=11

[ExtraInstall]
CopyFiles=ExtraFiles

[ExtraFiles]
missing_runtime.json
INF
cat >>"$ACTIVE_NEAR_PREFIX_HWID/viogpu3d.inf" <<'INF'

[DeadModels]
%Dead.DeviceDesc% = Dead_Inst, PCI\VEN_1AF4&DEV_1050
INF

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
assert_contains "$unregistered_output" "required INF UMD registration or active CopyFiles payload closure is incomplete" "unregistered UMD package"
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
assert_contains "$registered_output" "umd_active_copyfiles_payload_resolved=true" "registered UMD package"
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

pipe_decoy_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh \
    --require-render-candidate \
    "$PIPE_DELIMITER_DECOY" 2>&1
)" && fail "pipe-delimited basename unexpectedly fabricated a missing payload: $pipe_decoy_output"

assert_contains "$pipe_decoy_output" "package filename is not Windows-safe" "pipe-delimited payload decoy"
assert_contains "$pipe_decoy_output" "decoy|missing_viogpu_wgl_arm64.dll" "pipe-delimited payload decoy"

missing_active_payload_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh \
    --require-render-candidate \
    "$MISSING_ACTIVE_PAYLOAD" 2>&1
)" && fail "missing ancillary active CopyFiles payload unexpectedly passed the render gate: $missing_active_payload_output"

assert_contains "$missing_active_payload_output" "umd_registered_dlls_resolved=true" "missing ancillary active CopyFiles payload"
assert_contains "$missing_active_payload_output" "umd_active_copyfiles_payload_resolved=false" "missing ancillary active CopyFiles payload"
assert_contains "$missing_active_payload_output" "package_capability=umd-registration-active-payload-unresolved" "missing ancillary active CopyFiles payload"
assert_contains "$missing_active_payload_output" "render_candidate_reason=active-copyfiles-source-payload-unresolved" "missing ancillary active CopyFiles payload"
assert_contains "$missing_active_payload_output" "render_candidate=false" "missing ancillary active CopyFiles payload"

root_decoy_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh \
    --require-render-candidate \
    "$ROOT_DECOY_SUBDIR" 2>&1
)" && fail "root payload decoy unexpectedly satisfied a non-flat SourceDisksFiles mapping: $root_decoy_output"

assert_contains "$root_decoy_output" "umd_registered_dlls_resolved=true" "root payload decoy"
assert_contains "$root_decoy_output" "umd_active_copyfiles_payload_resolved=false" "root payload decoy"
assert_contains "$root_decoy_output" "package_capability=umd-registration-active-payload-unresolved" "root payload decoy"
assert_contains "$root_decoy_output" "render_candidate_reason=active-copyfiles-source-payload-unresolved" "root payload decoy"
assert_contains "$root_decoy_output" "render_candidate=false" "root payload decoy"

mixed_case_dll_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh \
    --require-render-candidate \
    "$MIXED_CASE_REGISTERED_DLL" 2>&1
)" && fail "invalid mixed-case registered DLL unexpectedly bypassed the ARM64 PE gate: $mixed_case_dll_output"

assert_contains "$mixed_case_dll_output" "viogpu3d DLL is not a PE/MZ image" "mixed-case registered DLL"
assert_contains "$mixed_case_dll_output" "viogpu_wgl_arm64.DlL" "mixed-case registered DLL"

at_source_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh \
    --require-render-candidate \
    "$AT_SOURCE_DECOY" 2>&1
)" && fail "named CopyFiles @source unexpectedly resolved to a root decoy: $at_source_output"

assert_contains "$at_source_output" "umd_registered_dlls_resolved=false" "named CopyFiles @source"
assert_contains "$at_source_output" "render_candidate_reason=active-inf-registration-dll-payload-unresolved" "named CopyFiles @source"
assert_contains "$at_source_output" "render_candidate=false" "named CopyFiles @source"

string_token_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh \
    --require-render-candidate \
    "$STRING_TOKEN_SOURCE" 2>&1
)" && fail "CopyFiles string-token source unexpectedly passed as a literal package filename: $string_token_output"

assert_contains "$string_token_output" "umd_registered_dlls_resolved=false" "CopyFiles string-token source"
assert_contains "$string_token_output" "render_candidate_reason=active-inf-registration-dll-payload-unresolved" "CopyFiles string-token source"
assert_contains "$string_token_output" "render_candidate=false" "CopyFiles string-token source"

duplicate_target_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh \
    --require-render-candidate \
    "$DUPLICATE_REGISTERED_TARGET" 2>&1
)" && fail "duplicate registered CopyFiles target unexpectedly passed: $duplicate_target_output"

assert_contains "$duplicate_target_output" "umd_registered_dlls_resolved=false" "duplicate registered CopyFiles target"
assert_contains "$duplicate_target_output" "render_candidate=false" "duplicate registered CopyFiles target"

replaceonly_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh \
    --require-render-candidate \
    "$REPLACEONLY_REGISTERED_DLL" 2>&1
)" && fail "REPLACEONLY registered DLL unexpectedly passed the fresh-guest render gate: $replaceonly_output"

assert_contains "$replaceonly_output" "umd_registered_dlls_resolved=false" "REPLACEONLY registered DLL"
assert_contains "$replaceonly_output" "render_candidate=false" "REPLACEONLY registered DLL"

missing_destination_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh \
    --require-render-candidate \
    "$MISSING_ANCILLARY_DESTINATION" 2>&1
)" && fail "ancillary CopyFiles operation without a destination unexpectedly passed: $missing_destination_output"

assert_contains "$missing_destination_output" "umd_registered_dlls_resolved=true" "missing ancillary CopyFiles destination"
assert_contains "$missing_destination_output" "umd_active_copyfiles_payload_resolved=false" "missing ancillary CopyFiles destination"
assert_contains "$missing_destination_output" "render_candidate_reason=active-copyfiles-source-payload-unresolved" "missing ancillary CopyFiles destination"

unresolved_dirid_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh \
    --require-render-candidate \
    "$UNRESOLVED_ANCILLARY_DIRID" 2>&1
)" && fail "undefined ancillary CopyFiles DirID token unexpectedly passed: $unresolved_dirid_output"

assert_contains "$unresolved_dirid_output" "umd_registered_dlls_resolved=true" "undefined ancillary CopyFiles DirID"
assert_contains "$unresolved_dirid_output" "umd_active_copyfiles_payload_resolved=false" "undefined ancillary CopyFiles DirID"
assert_contains "$unresolved_dirid_output" "render_candidate_reason=active-copyfiles-source-payload-unresolved" "undefined ancillary CopyFiles DirID"

noncanonical_dirid_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh \
    --require-render-candidate \
    "$NONCANONICAL_ANCILLARY_DIRID" 2>&1
)" && fail "noncanonical ancillary CopyFiles DirID unexpectedly passed: $noncanonical_dirid_output"

assert_contains "$noncanonical_dirid_output" "umd_registered_dlls_resolved=true" "noncanonical ancillary CopyFiles DirID"
assert_contains "$noncanonical_dirid_output" "umd_active_copyfiles_payload_resolved=false" "noncanonical ancillary CopyFiles DirID"

numeric_dirid_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh \
    --require-render-candidate \
    "$NUMERIC_ANCILLARY_DIRID" 2>&1
)" || fail "canonical numeric ancillary CopyFiles DirID should pass: $numeric_dirid_output"

assert_contains "$numeric_dirid_output" "umd_active_copyfiles_payload_resolved=true" "numeric ancillary CopyFiles DirID"
assert_contains "$numeric_dirid_output" "render_candidate=true" "numeric ancillary CopyFiles DirID"

mixed_direct_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh \
    --require-render-candidate \
    "$MIXED_DIRECT_COPYFILES" 2>&1
)" && fail "mixed section/direct CopyFiles directive unexpectedly passed: $mixed_direct_output"

assert_contains "$mixed_direct_output" "umd_registered_dlls_resolved=false" "mixed section/direct CopyFiles directive"
assert_contains "$mixed_direct_output" "render_candidate=false" "mixed section/direct CopyFiles directive"

include_needs_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh \
    --require-render-candidate \
    "$INCLUDE_NEEDS_COPYFILES" 2>&1
)" && fail "selected DDInstall Needs delegation with a missing payload unexpectedly passed: $include_needs_output"

assert_contains "$include_needs_output" "umd_registered_dlls_resolved=false" "selected DDInstall Needs delegation"
assert_contains "$include_needs_output" "render_candidate=false" "selected DDInstall Needs delegation"

include_only_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh \
    --require-render-candidate \
    "$INCLUDE_ONLY" 2>&1
)" || fail "Include-only selected DDInstall should remain supported: $include_only_output"

assert_contains "$include_only_output" "render_candidate=true" "Include-only selected DDInstall"

near_prefix_hwid_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh "$NEAR_PREFIX_HWID" 2>&1
)" && fail "DEV_10500 near-prefix HWID unexpectedly passed global package detection: $near_prefix_hwid_output"

assert_contains "$near_prefix_hwid_output" "does not advertise a supported VirtIO GPU HWID" "near-prefix global HWID detection"

active_near_prefix_hwid_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh \
    --pci-device-id 1050 \
    --require-render-candidate \
    "$ACTIVE_NEAR_PREFIX_HWID" 2>&1
)" && fail "VEN_1AF40 near-prefix active model unexpectedly passed: $active_near_prefix_hwid_output"

assert_contains "$active_near_prefix_hwid_output" "render_candidate=false" "near-prefix active HWID model"

nonflat_disk_name_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh \
    --require-render-candidate \
    "$NONFLAT_SOURCE_DISK_NAME" 2>&1
)" && fail "CAB/tag SourceDisksNames contract unexpectedly passed the flat injector gate: $nonflat_disk_name_output"

assert_contains "$nonflat_disk_name_output" "umd_registered_dlls_resolved=true" "non-flat SourceDisksNames contract"
assert_contains "$nonflat_disk_name_output" "umd_active_copyfiles_payload_resolved=false" "non-flat SourceDisksNames contract"
assert_contains "$nonflat_disk_name_output" "render_candidate_reason=active-copyfiles-source-payload-unresolved" "non-flat SourceDisksNames contract"

percent_semicolon_disk_name_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh \
    --require-render-candidate \
    "$PERCENT_SEMICOLON_DISK_NAME" 2>&1
)" && fail "semicolon inside a SourceDisksNames string token hid a non-flat base path: $percent_semicolon_disk_name_output"

assert_contains "$percent_semicolon_disk_name_output" "umd_registered_dlls_resolved=true" "percent-token SourceDisksNames contract"
assert_contains "$percent_semicolon_disk_name_output" "umd_active_copyfiles_payload_resolved=false" "percent-token SourceDisksNames contract"
assert_contains "$percent_semicolon_disk_name_output" "render_candidate_reason=active-copyfiles-source-payload-unresolved" "percent-token SourceDisksNames contract"

percent_comma_disk_name_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh \
    --require-render-candidate \
    "$PERCENT_COMMA_DISK_NAME" 2>&1
)" && fail "commas inside a SourceDisksNames string token hid a non-flat base path: $percent_comma_disk_name_output"

assert_contains "$percent_comma_disk_name_output" "umd_registered_dlls_resolved=true" "percent-comma SourceDisksNames contract"
assert_contains "$percent_comma_disk_name_output" "umd_active_copyfiles_payload_resolved=false" "percent-comma SourceDisksNames contract"
assert_contains "$percent_comma_disk_name_output" "render_candidate_reason=active-copyfiles-source-payload-unresolved" "percent-comma SourceDisksNames contract"

percent_escape_disk_name_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh \
    --require-render-candidate \
    "$PERCENT_ESCAPE_DISK_NAME" 2>&1
)" && fail "adjacent/escaped percent syntax unexpectedly passed as one SourceDisksNames description token: $percent_escape_disk_name_output"

assert_contains "$percent_escape_disk_name_output" "umd_active_copyfiles_payload_resolved=false" "percent escape SourceDisksNames contract"
assert_contains "$percent_escape_disk_name_output" "render_candidate=false" "percent escape SourceDisksNames contract"

unbalanced_percent_disk_name_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh \
    --require-render-candidate \
    "$UNBALANCED_PERCENT_DISK_NAME" 2>&1
)" && fail "unbalanced SourceDisksNames string token unexpectedly passed: $unbalanced_percent_disk_name_output"

assert_contains "$unbalanced_percent_disk_name_output" "umd_active_copyfiles_payload_resolved=false" "unbalanced percent SourceDisksNames contract"
assert_contains "$unbalanced_percent_disk_name_output" "render_candidate=false" "unbalanced percent SourceDisksNames contract"

nonnumeric_disk_id_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh \
    --require-render-candidate \
    "$NONNUMERIC_DISK_ID" 2>&1
)" && fail "nonnumeric SourceDisks disk ID unexpectedly passed: $nonnumeric_disk_id_output"

assert_contains "$nonnumeric_disk_id_output" "umd_active_copyfiles_payload_resolved=false" "nonnumeric SourceDisks disk ID"
assert_contains "$nonnumeric_disk_id_output" "render_candidate=false" "nonnumeric SourceDisks disk ID"

leading_zero_disk_id_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh --require-render-candidate "$LEADING_ZERO_DISK_ID" 2>&1
)" && fail "leading-zero SourceDisks disk ID unexpectedly passed: $leading_zero_disk_id_output"
assert_contains "$leading_zero_disk_id_output" "umd_active_copyfiles_payload_resolved=false" "leading-zero SourceDisks disk ID"

oversized_disk_id_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh --require-render-candidate "$OVERSIZED_DISK_ID" 2>&1
)" && fail "oversized SourceDisks disk ID unexpectedly passed: $oversized_disk_id_output"
assert_contains "$oversized_disk_id_output" "umd_active_copyfiles_payload_resolved=false" "oversized SourceDisks disk ID"

duplicate_source_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh --require-render-candidate "$DUPLICATE_SOURCE_MAPPING" 2>&1
)" && fail "duplicate exact-section SourceDisksFiles mapping unexpectedly passed: $duplicate_source_output"
assert_contains "$duplicate_source_output" "umd_active_copyfiles_payload_resolved=false" "duplicate SourceDisksFiles mapping"

expanded_string_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh --require-render-candidate "$EXPANDED_STRING_DELIMITER" 2>&1
)" && fail "SourceDisksNames string expansion with a CSV delimiter unexpectedly passed: $expanded_string_output"
assert_contains "$expanded_string_output" "umd_active_copyfiles_payload_resolved=false" "expanded-delimiter SourceDisksNames contract"
assert_contains "$expanded_string_output" "render_candidate_reason=active-copyfiles-source-payload-unresolved" "expanded-delimiter SourceDisksNames contract"

undefined_string_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh --require-render-candidate "$UNDEFINED_STRING_TOKEN" 2>&1
)" && fail "undefined SourceDisksNames string token unexpectedly passed: $undefined_string_output"
assert_contains "$undefined_string_output" "umd_active_copyfiles_payload_resolved=false" "undefined SourceDisksNames string token"

localized_string_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh --require-render-candidate "$LOCALIZED_STRING_OVERRIDE" 2>&1
)" && fail "locale-dependent SourceDisksNames string token unexpectedly passed: $localized_string_output"
assert_contains "$localized_string_output" "umd_active_copyfiles_payload_resolved=false" "localized SourceDisksNames string token"

duplicate_string_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh --require-render-candidate "$DUPLICATE_STRING_DEFINITION" 2>&1
)" && fail "duplicate generic Strings definition unexpectedly passed: $duplicate_string_output"
assert_contains "$duplicate_string_output" "umd_active_copyfiles_payload_resolved=false" "duplicate generic Strings definition"

unquoted_description_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh --require-render-candidate "$UNQUOTED_DISK_DESCRIPTION" 2>&1
)" && fail "unquoted literal SourceDisksNames description unexpectedly passed: $unquoted_description_output"
assert_contains "$unquoted_description_output" "umd_active_copyfiles_payload_resolved=false" "unquoted SourceDisksNames description"

ambiguous_model_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh --pci-device-id 1050 --require-render-candidate "$AMBIGUOUS_MODEL_PATH" 2>&1
)" && fail "ambiguous ARM64 TargetOSVersion model paths unexpectedly passed: $ambiguous_model_output"
assert_contains "$ambiguous_model_output" "umd_registered_dlls_resolved=false" "ambiguous ARM64 model paths"
assert_contains "$ambiguous_model_output" "render_candidate=false" "ambiguous ARM64 model paths"

duplicate_model_entry_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh --pci-device-id 1050 --require-render-candidate "$DUPLICATE_MODEL_ENTRY" 2>&1
)" && fail "duplicate expected-HWID model entries unexpectedly passed: $duplicate_model_entry_output"
assert_contains "$duplicate_model_entry_output" "umd_registered_dlls_resolved=false" "duplicate expected-HWID model entries"
assert_contains "$duplicate_model_entry_output" "render_candidate=false" "duplicate expected-HWID model entries"

duplicate_destination_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh --require-render-candidate "$DUPLICATE_DESTINATION" 2>&1
)" && fail "duplicate normalized DestinationDirs key unexpectedly passed: $duplicate_destination_output"
assert_contains "$duplicate_destination_output" "umd_registered_dlls_resolved=false" "duplicate DestinationDirs key"
assert_contains "$duplicate_destination_output" "render_candidate=false" "duplicate DestinationDirs key"

multiple_inf_output="$(
  scripts/check-hvf-windows-viogpu3d-package.sh --pci-device-id 1050 --require-render-candidate "$MULTIPLE_MATCHING_INFS" 2>&1
)" && fail "multiple INF files advertising the expected HWID unexpectedly passed: $multiple_inf_output"
assert_contains "$multiple_inf_output" "inf_count=2" "multiple matching INF files"
assert_contains "$multiple_inf_output" "umd_registration_inf=<none>" "multiple matching INF files"
assert_contains "$multiple_inf_output" "render_candidate=false" "multiple matching INF files"

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
