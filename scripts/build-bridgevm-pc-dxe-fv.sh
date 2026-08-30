#!/usr/bin/env bash
# Assemble the bounded BridgeVM DXE firmware volume from already-built images.
set -euo pipefail

if [[ $# -ne 6 ]]; then
  echo "usage: $0 EDK2_ROOT BUILD_ROOT DXE_CORE RUNTIME PLATFORM_TABLES DXE_PROBE" >&2
  exit 64
fi

edk2_root="$1"
build_root="$2"
core="$3"
runtime="$4"
platform="$5"
probe="$6"
tool_root="$edk2_root/BaseTools/Source/C/bin"
driver_root="$edk2_root/Build/BridgeVmPc/RELEASE_GCC/AARCH64/BridgeVmPcPkg/Drivers"
readonly DXE_CORE_GUID="D6A2CB7F-6A18-4e2f-B43B-9920A733700A"
readonly RUNTIME_GUID="B601F8C4-43B7-4784-95B1-F4226CB40CEE"
readonly PLATFORM_GUID="B6F1376D-3E28-421B-A64C-2B5E0D185397"
readonly DXE_PROBE_GUID="36A32D32-548B-4970-A32A-68B01E131B4A"
readonly FFS2_GUID="8c8ce578-8a3d-4f1c-9935-896185c32dd3"
readonly FV_NAME_GUID="7D2A7E6B-9B08-4C1F-AED5-799718B43F33"

make_driver_ffs() {
  local name="$1" guid="$2" image="$3" depex="$4"
  "$tool_root/GenSec" -s EFI_SECTION_DXE_DEPEX -o "$build_root/$name.depex" "$depex"
  "$tool_root/GenSec" -s EFI_SECTION_PE32 -o "$build_root/$name.pe32" "$image"
  "$tool_root/GenFfs" -t EFI_FV_FILETYPE_DRIVER -g "$guid" \
    -i "$build_root/$name.depex" -i "$build_root/$name.pe32" -o "$build_root/$name.ffs"
}

"$tool_root/GenSec" -s EFI_SECTION_PE32 -o "$build_root/DxeCore.pe32" "$core"
"$tool_root/GenFfs" -t EFI_FV_FILETYPE_DXE_CORE -g "$DXE_CORE_GUID" \
  -i "$build_root/DxeCore.pe32" -o "$build_root/DxeCore.ffs"
make_driver_ffs PlatformTables "$PLATFORM_GUID" "$platform" \
  "$driver_root/PlatformTablesDxe/PlatformTablesDxe/OUTPUT/BridgeVmPcPlatformTablesDxe.depex"
make_driver_ffs RuntimeDxe "$RUNTIME_GUID" "$runtime" "$build_root/runtime/RuntimeDxe.depex"
make_driver_ffs DxeProbe "$DXE_PROBE_GUID" "$probe" \
  "$driver_root/DxeProbe/DxeProbe/OUTPUT/BridgeVmPcDxeProbe.depex"

fv="$build_root/BridgeVmPcDxeEntry.fv"
"$tool_root/GenFv" -o "$fv" -b 0x1000 -n 0x100 \
  -f "$build_root/DxeCore.ffs" -f "$build_root/RuntimeDxe.ffs" \
  -f "$build_root/PlatformTables.ffs" \
  -f "$build_root/DxeProbe.ffs" -g "$FFS2_GUID" --FvNameGuid "$FV_NAME_GUID"

python3 "$(dirname "$0")/check-bridgevm-pc-dxe-fv.py" \
  "$core" "$runtime" "$platform" "$probe" "$fv"
