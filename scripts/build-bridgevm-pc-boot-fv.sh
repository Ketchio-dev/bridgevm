#!/usr/bin/env bash
# Assemble the independent-board boot FV from pinned, already-built images.
set -euo pipefail
if [[ $# -ne 8 ]]; then
  echo "usage: $0 EDK2 BUILD CORE_DIR RUNTIME_DIR VARIABLE_DIR DRIVER_DIR BOOT_DIR OUTPUT" >&2
  exit 64
fi
edk2="$1"; build="$2"; core_dir="$3"; runtime_dir="$4"; variable_dir="$5"
drivers="$6"; boot="$7"; output="$8"; tools="$edk2/BaseTools/Source/C/bin"
readonly CORE_GUID="D6A2CB7F-6A18-4e2f-B43B-9920A733700A"
readonly FFS2_GUID="8c8ce578-8a3d-4f1c-9935-896185c32dd3"
readonly FV_GUID="EF12E854-63A1-4D01-A694-CB19A9D70925"
make_driver() {
  local name="$1" guid="$2" image="$3" depex="$4" sections
  "$tools/GenSec" -s EFI_SECTION_PE32 -o "$build/$name.pe32" "$image"
  sections=(-i "$build/$name.pe32")
  if [[ "$depex" != - ]]; then
    "$tools/GenSec" -s EFI_SECTION_DXE_DEPEX -o "$build/$name.depex" "$depex"
    sections=(-i "$build/$name.depex" "${sections[@]}")
  fi
  "$tools/GenFfs" -t EFI_FV_FILETYPE_DRIVER -g "$guid" "${sections[@]}" \
    -o "$build/$name.ffs"
}
core="$core_dir/BridgeVmPcDxeCore.efi"
"$tools/GenSec" -s EFI_SECTION_PE32 -o "$build/DxeCore.pe32" "$core"
"$tools/GenFfs" -t EFI_FV_FILETYPE_DXE_CORE -g "$CORE_GUID" \
  -i "$build/DxeCore.pe32" -o "$build/DxeCore.ffs"
make_driver Runtime B601F8C4-43B7-4784-95B1-F4226CB40CEE "$runtime_dir/RuntimeDxe.efi" "$runtime_dir/RuntimeDxe.depex"
make_driver Variables CBD2E4D5-7068-4FF5-B462-9822B4AD8D60 "$variable_dir/VariableRuntimeDxe.efi" "$variable_dir/VariableRuntimeDxe.depex"
make_driver Platform B6F1376D-3E28-421B-A64C-2B5E0D185397 "$drivers/BridgeVmPcPlatformTablesDxe.efi" "$edk2/Build/BridgeVmPc/RELEASE_GCC/AARCH64/BridgeVmPcPkg/Drivers/PlatformTablesDxe/PlatformTablesDxe/OUTPUT/BridgeVmPcPlatformTablesDxe.depex"
make_driver Cpu B8D9777E-D72A-451F-9BDB-BAFB52A68415 "$drivers/ArmCpuDxe.efi" "$drivers/ArmCpuDxe.depex"
make_driver CpuIo FBC36D76-CF22-2584-DBD8-85FF765BAEF1 "$drivers/CpuMmio2Dxe.efi" "$drivers/CpuMmio2Dxe.depex"
make_driver PciBus 93B80004-9FB3-11D4-9A3A-0090273FC14D "$drivers/PciBusDxe.efi" "$boot/Metronome.depex"
make_driver PciHost 128FB770-5E79-4176-9E51-9BB268A17DD1 "$drivers/PciHostBridgeDxe.efi" "$drivers/PciHostBridgeDxe.depex"
make_driver Nvme 5BE3BDF4-53CF-46A3-A6A9-73C34A6E5EE3 "$drivers/NvmExpressDxe.efi" "$boot/Metronome.depex"
make_driver Security F80697E9-7FD6-4665-8646-88E33EF71DFC "$boot/SecurityStubDxe.efi" "$boot/SecurityStubDxe.depex"
make_driver Gic 953FF472-9B9E-4058-84CF-227DAF89DC82 "$boot/ArmGicV3Dxe.efi" "$boot/ArmGicV3Dxe.depex"
make_driver Timer 49EA041E-6752-42CA-B0B1-7344FE2546B7 "$boot/ArmTimerDxe.efi" "$boot/ArmTimerDxe.depex"
make_driver Metronome C8339973-A563-4561-B858-D8476F9DEFC4 "$boot/Metronome.efi" "$boot/Metronome.depex"
make_driver Watchdog F099D67F-71AE-4C36-B2A3-DCEB0EB2B7D8 "$boot/WatchdogTimer.efi" "$boot/WatchdogTimer.depex"
make_driver Capsule 42857F0A-13F2-4B21-8A23-53D3F714B840 "$boot/CapsuleRuntimeDxe.efi" "$boot/CapsuleRuntimeDxe.depex"
make_driver Monotonic AD608272-D07F-4964-801E-7BD3B7888652 "$boot/MonotonicCounterRuntimeDxe.efi" "$boot/MonotonicCounterRuntimeDxe.depex"
make_driver Reset 4B28E4C7-FF36-4E10-93CF-A82159E777C5 "$boot/ResetSystemRuntimeDxe.efi" "$boot/ResetSystemRuntimeDxe.depex"
make_driver Rtc B336F62D-4135-4A55-AE4E-4971BBF0885D "$boot/RealTimeClock.efi" "$boot/RealTimeClock.depex"
make_driver DiskIo 6B38F7B4-AD98-40E9-9093-ACA2B5A253C4 "$boot/DiskIoDxe.efi" "$boot/Metronome.depex"
make_driver Partition 1FA1F39E-FEFF-4AAE-BD7B-38A070A3B609 "$boot/PartitionDxe.efi" "$boot/Metronome.depex"
make_driver English CD3BAFB6-50FB-4FE8-8E4E-AB74D2C1A600 "$boot/EnglishDxe.efi" "$boot/Metronome.depex"
make_driver Fat 961578FE-B6B7-44C3-AF35-6BC705CD2B1F "$boot/Fat.efi" "$boot/Metronome.depex"
make_driver ConSplit 408EDCEC-CF6D-477C-A5A8-B4844E3DE281 "$boot/ConSplitterDxe.efi" -
make_driver Gop 59BFB167-0E0C-4A04-9045-0658641760DF "$boot/BridgeVmPcGraphicsOutputDxe.efi" "$boot/BridgeVmPcGraphicsOutputDxe.depex"
make_driver Bds 1E153CB8-CF1A-4CFA-A5E9-0E1817B68953 "$boot/BridgeVmPcBootManagerDxe.efi" "$boot/BridgeVmPcBootManagerDxe.depex"
ffs=(DxeCore Runtime Variables Platform Cpu CpuIo PciBus PciHost Nvme Security Gic Timer Metronome Watchdog Capsule Monotonic Reset Rtc DiskIo Partition English Fat ConSplit Gop Bds)
args=(); for name in "${ffs[@]}"; do args+=(-f "$build/$name.ffs"); done
fv="$build/BridgeVmPcBoot.fv"; "$tools/GenFv" -o "$fv" -b 0x1000 -n 0x100 "${args[@]}" \
  -g "$FFS2_GUID" --FvNameGuid "$FV_GUID"
images=("$core" "$runtime_dir/RuntimeDxe.efi" "$variable_dir/VariableRuntimeDxe.efi" "$drivers/BridgeVmPcPlatformTablesDxe.efi" "$drivers/ArmCpuDxe.efi" "$drivers/CpuMmio2Dxe.efi" "$drivers/PciBusDxe.efi" "$drivers/PciHostBridgeDxe.efi" "$drivers/NvmExpressDxe.efi" "$boot/SecurityStubDxe.efi" "$boot/ArmGicV3Dxe.efi" "$boot/ArmTimerDxe.efi" "$boot/Metronome.efi" "$boot/WatchdogTimer.efi" "$boot/CapsuleRuntimeDxe.efi" "$boot/MonotonicCounterRuntimeDxe.efi" "$boot/ResetSystemRuntimeDxe.efi" "$boot/RealTimeClock.efi" "$boot/DiskIoDxe.efi" "$boot/PartitionDxe.efi" "$boot/EnglishDxe.efi" "$boot/Fat.efi" "$boot/ConSplitterDxe.efi" "$boot/BridgeVmPcGraphicsOutputDxe.efi" "$boot/BridgeVmPcBootManagerDxe.efi")
python3 "$(dirname "$0")/check-bridgevm-pc-boot-fv.py" "$fv" \
  "$boot/BridgeVmPcExitBootServicesProbe.efi" "${images[@]}"
mkdir -p "$output"; cp "$fv" "$output/BridgeVmPcBoot.fv"
echo "built $output/BridgeVmPcBoot.fv"
echo "sha256 $(shasum -a 256 "$output/BridgeVmPcBoot.fv" | awk '{print $1}')"
