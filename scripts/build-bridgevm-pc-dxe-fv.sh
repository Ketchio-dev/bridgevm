#!/usr/bin/env bash
# Assemble the bounded BridgeVM DXE firmware volume from already-built images.
set -euo pipefail
if [[ $# -ne 11 ]]; then
  echo "usage: $0 EDK2 BUILD CORE RUNTIME VARIABLES PLATFORM CPU CPU_IO PCI_BUS PCI_HOST PROBE" >&2
  exit 64
fi
edk2_root="$1"; build_root="$2"; core="$3"; runtime="$4"; variables="$5"; platform="$6"
cpu="$7"; cpu_io="$8"; pci_bus="$9"; pci_host="${10}"; probe="${11}"
tool_root="$edk2_root/BaseTools/Source/C/bin"
driver_root="$edk2_root/Build/BridgeVmPc/RELEASE_GCC/AARCH64/BridgeVmPcPkg/Drivers"
readonly DXE_CORE_GUID="D6A2CB7F-6A18-4e2f-B43B-9920A733700A"
readonly FFS2_GUID="8c8ce578-8a3d-4f1c-9935-896185c32dd3"
readonly FV_NAME_GUID="7D2A7E6B-9B08-4C1F-AED5-799718B43F33"
make_driver_ffs() {
  local name="$1" guid="$2" image="$3" depex="$4" sections
  "$tool_root/GenSec" -s EFI_SECTION_PE32 -o "$build_root/$name.pe32" "$image"
  sections=(-i "$build_root/$name.pe32")
  if [[ "$depex" != - ]]; then
    "$tool_root/GenSec" -s EFI_SECTION_DXE_DEPEX -o "$build_root/$name.depex" "$depex"
    sections=(-i "$build_root/$name.depex" "${sections[@]}")
  fi
  "$tool_root/GenFfs" -t EFI_FV_FILETYPE_DRIVER -g "$guid" "${sections[@]}" -o "$build_root/$name.ffs"
}
"$tool_root/GenSec" -s EFI_SECTION_PE32 -o "$build_root/DxeCore.pe32" "$core"
"$tool_root/GenFfs" -t EFI_FV_FILETYPE_DXE_CORE -g "$DXE_CORE_GUID" \
  -i "$build_root/DxeCore.pe32" -o "$build_root/DxeCore.ffs"
make_driver_ffs RuntimeDxe B601F8C4-43B7-4784-95B1-F4226CB40CEE "$runtime" "$build_root/runtime/RuntimeDxe.depex"
make_driver_ffs VariableRuntimeDxe CBD2E4D5-7068-4FF5-B462-9822B4AD8D60 "$variables" "$build_root/variables/VariableRuntimeDxe.depex"
make_driver_ffs PlatformTables B6F1376D-3E28-421B-A64C-2B5E0D185397 "$platform" "$driver_root/PlatformTablesDxe/PlatformTablesDxe/OUTPUT/BridgeVmPcPlatformTablesDxe.depex"
make_driver_ffs ArmCpu B8D9777E-D72A-451F-9BDB-BAFB52A68415 "$cpu" "$build_root/drivers/ArmCpuDxe.depex"
make_driver_ffs CpuMmio2 FBC36D76-CF22-2584-DBD8-85FF765BAEF1 "$cpu_io" "$build_root/drivers/CpuMmio2Dxe.depex"
make_driver_ffs PciBus 93B80004-9FB3-11D4-9A3A-0090273FC14D "$pci_bus" "$build_root/drivers/ArmCpuDxe.depex"
make_driver_ffs PciHostBridge 128FB770-5E79-4176-9E51-9BB268A17DD1 "$pci_host" "$build_root/drivers/PciHostBridgeDxe.depex"
make_driver_ffs DxeProbe 36A32D32-548B-4970-A32A-68B01E131B4A "$probe" "$driver_root/DxeProbe/DxeProbe/OUTPUT/BridgeVmPcDxeProbe.depex"
fv="$build_root/BridgeVmPcDxeEntry.fv"
"$tool_root/GenFv" -o "$fv" -b 0x1000 -n 0x100 -f "$build_root/DxeCore.ffs" \
  -f "$build_root/RuntimeDxe.ffs" -f "$build_root/VariableRuntimeDxe.ffs" \
  -f "$build_root/PlatformTables.ffs" -f "$build_root/ArmCpu.ffs" \
  -f "$build_root/CpuMmio2.ffs" -f "$build_root/PciBus.ffs" \
  -f "$build_root/PciHostBridge.ffs" -f "$build_root/DxeProbe.ffs" \
  -g "$FFS2_GUID" --FvNameGuid "$FV_NAME_GUID"
python3 "$(dirname "$0")/check-bridgevm-pc-dxe-fv.py" "$core" "$runtime" \
  "$variables" "$platform" "$cpu" "$cpu_io" "$pci_bus" "$pci_host" "$probe" "$fv"
