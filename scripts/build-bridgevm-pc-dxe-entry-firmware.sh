#!/usr/bin/env bash
# Build the development-only BridgeVM PC RuntimeDxe/platform-table probe.
set -euo pipefail
readonly EXPECTED_EDK2_COMMIT="b03a21a63e3bd001f52c527e5a57feddb53a690b"
readonly EXPECTED_GCC_VERSION="aarch64-elf-gcc (GCC) 16.1.0"
readonly EXPECTED_LD_VERSION="GNU ld (GNU Binutils) 2.46.1"
readonly EXPECTED_REBASED_DXE_SHA256="cfe2ea1a7dc5573b4a5f6952e9177475ef91fb41da40c08e89c6f48bec2a4d90"
readonly EXPECTED_RUNTIME_SHA256="5b3a37c1403e77b51b8dafb16a796dcbc3080692617ff0eae8b759c979b4260d"
readonly EXPECTED_VARIABLE_SHA256="3d6f0fbd9d155f76d6f1001ee67fce25e36bd5aef20bea088421a772a7500a90"
readonly EXPECTED_PLATFORM_SHA256="16b3fdd6ede6d5aea14d26419351cf262ef358692fd28682dbbafe74c22438b5"
readonly EXPECTED_PROBE_SHA256="3258c4efc888efa2415be86756af4fd9fcbd5ac874a89df838ce1c89c87b31e0"
readonly EXPECTED_VECTOR_SHA256="3ec6ddb04175dbfbd84dc784a264c8f487410c03a0c4d4a38f630e8b06032226"
readonly EXPECTED_FV_SHA256="0b093cc672914ecf7b2b842b83d8325ffc870d1e314993cab963f78a01fcf78e"
readonly EXPECTED_FD_SHA256="42e294e45119d08a5a8d6b4f28b5de9b79872be9282d700832460977bbd8282b"
readonly FLASH_SIZE=$((0x04000000))
readonly FV_OFFSET=$((0x00100000))
readonly FV_SIZE=$((0x00100000))
readonly VECTOR_LIMIT=$((0x1800))
if [[ $# -ne 2 ]]; then
  echo "usage: $0 /path/to/pinned-edk2 OUTPUT_DIR" >&2
  exit 64
fi
edk2_root="$(cd "$1" && pwd)"
output_dir="$2"
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
source_root="$repo_root/crates/bridgevm-hvf/firmware/BridgeVmPcPkg/ResetVector"
tool_root="$edk2_root/BaseTools/Source/C/bin"
gcc="/opt/homebrew/bin/aarch64-elf-gcc"
ld="/opt/homebrew/bin/aarch64-elf-ld"
objcopy="/opt/homebrew/bin/aarch64-elf-objcopy"
nm="/opt/homebrew/bin/aarch64-elf-nm"
objdump="/opt/homebrew/bin/aarch64-elf-objdump"
[[ "$(git -C "$edk2_root" rev-parse HEAD)" == "$EXPECTED_EDK2_COMMIT" ]] || {
  echo "refusing an unpinned EDK2 checkout" >&2
  exit 65
}
gcc_version="$($gcc --version | head -1)"
ld_version="$($ld --version | head -1)"
if [[ "$gcc_version" != "$EXPECTED_GCC_VERSION" || "$ld_version" != "$EXPECTED_LD_VERSION" ]]; then
  echo "refusing unpinned firmware tools: gcc='${gcc_version}' ld='${ld_version}'" >&2
  exit 66
fi
"$repo_root/scripts/check-bridgevm-pc-prohibited-references.sh" tree BridgeVmPcPkg \
  "$repo_root/crates/bridgevm-hvf/firmware/BridgeVmPcPkg"
build_root="$(mktemp -d "/tmp/bridgevm-pc-dxe-entry.XXXXXX")"
trap 'rm -rf "$build_root"' EXIT
"$repo_root/scripts/build-bridgevm-pc-dxe-core-fv.sh" \
  "$edk2_root" "$build_root/core"
"$repo_root/scripts/build-bridgevm-pc-runtime-dxe.sh" \
  "$edk2_root" "$build_root/runtime"
"$repo_root/scripts/build-bridgevm-pc-variable-dxe.sh" \
  "$edk2_root" "$build_root/variables"
"$repo_root/scripts/build-bridgevm-pc-edk2-consumer.sh" \
  "$edk2_root" "$build_root/drivers"
core="$build_root/DxeCore.efi"; runtime="$build_root/RuntimeDxe.efi"
variables="$build_root/VariableRuntimeDxe.efi"; platform="$build_root/PlatformTables.efi"
probe="$build_root/DxeProbe.efi"; pci_root="$build_root/drivers"
cpu="$pci_root/ArmCpuDxe.efi"; cpu_io="$pci_root/CpuMmio2Dxe.efi"
pci_bus="$pci_root/PciBusDxe.efi"; pci_host="$pci_root/PciHostBridgeDxe.efi"
cp "$build_root/core/BridgeVmPcDxeCore.efi" "$core"
cp "$build_root/runtime/RuntimeDxe.efi" "$runtime"
cp "$build_root/variables/VariableRuntimeDxe.efi" "$variables"
cp "$build_root/drivers/BridgeVmPcPlatformTablesDxe.efi" "$platform"
cp "$edk2_root/Build/BridgeVmPc/RELEASE_GCC/AARCH64/BridgeVmPcDxeProbe.efi" "$probe"
"$tool_root/GenFw" --rebase 0x100400000 -r "$core"
"$tool_root/GenFw" -z -r "$probe"
sha256() { shasum -a 256 "$1" | awk '{print $1}'; }
rebased_sha256="$(sha256 "$core")"; runtime_sha256="$(sha256 "$runtime")"
variable_sha256="$(sha256 "$variables")"; platform_sha256="$(sha256 "$platform")"
probe_sha256="$(sha256 "$probe")"; pci_receipt_sha256="$(sha256 "$pci_root/BridgeVmPcPciDxe.build.json")"
[[ "$rebased_sha256" == "$EXPECTED_REBASED_DXE_SHA256" ]] || { echo "unexpected rebased DXE Core digest: $rebased_sha256" >&2; exit 68; }
[[ "$runtime_sha256" == "$EXPECTED_RUNTIME_SHA256" ]] || { echo "unexpected RuntimeDxe digest: $runtime_sha256" >&2; exit 68; }
[[ "$variable_sha256" == "$EXPECTED_VARIABLE_SHA256" ]] || { echo "unexpected variable-service digest: $variable_sha256" >&2; exit 68; }
[[ "$platform_sha256" == "$EXPECTED_PLATFORM_SHA256" ]] || { echo "unexpected platform-tables digest: $platform_sha256" >&2; exit 68; }
[[ "$probe_sha256" == "$EXPECTED_PROBE_SHA256" ]] || { echo "unexpected DXE probe digest: $probe_sha256" >&2; exit 68; }
"$repo_root/scripts/build-bridgevm-pc-dxe-fv.sh" \
  "$edk2_root" "$build_root" "$core" "$runtime" "$variables" "$platform" \
  "$cpu" "$cpu_io" "$pci_bus" "$pci_host" "$probe"
fv="$build_root/BridgeVmPcDxeEntry.fv"
fv_sha256="$(shasum -a 256 "$fv" | awk '{print $1}')"
[[ "$fv_sha256" == "$EXPECTED_FV_SHA256" ]] || {
  echo "DXE entry FV digest ${fv_sha256} does not match ${EXPECTED_FV_SHA256}" >&2
  exit 69
}
reset_object="$build_root/reset.o"; exception_object="$build_root/exception.o"; mmu_object="$build_root/mmu.o"
sec_object="$build_root/sec.o"; hob_object="$build_root/hob.o"; ipl_object="$build_root/dxe-ipl.o"
elf="$build_root/firmware.elf"
vector="$build_root/firmware.bin"
"$gcc" -c -x assembler-with-cpp -DBRIDGE_VM_PC_DXE_ENTRY -ffreestanding -fno-pic \
  "$source_root/BridgeVmPcResetVector.S" -o "$reset_object"
"$gcc" -c -x assembler-with-cpp -ffreestanding -fno-pic \
  "$source_root/BridgeVmPcExceptionVector.S" -o "$exception_object"
"$gcc" -c -x assembler-with-cpp -ffreestanding -fno-pic \
  "$source_root/BridgeVmPcMmu.S" -o "$mmu_object"
for item in "BridgeVmPcSec.c:$sec_object" "BridgeVmPcHob.c:$hob_object" \
            "BridgeVmPcDxeIpl.c:$ipl_object"; do
  source_file="${item%%:*}"
  object_file="${item#*:}"
  "$gcc" -c -O2 -Wall -Wextra -Werror -DBRIDGE_VM_PC_DXE_ENTRY \
    -ffreestanding -fno-builtin -fno-pic \
    -fno-stack-protector "$source_root/$source_file" -o "$object_file"
done
"$ld" --build-id=none -nostdlib -T "$source_root/BridgeVmPcDxeEntry.ld" \
  "$reset_object" "$exception_object" "$mmu_object" "$sec_object" "$hob_object" \
  "$ipl_object" -o "$elf"
"$objcopy" -O binary "$elf" "$vector"
[[ "$($nm -n "$elf" | awk '$3 == "_start" {print $1}')" == "0000000000000000" ]]
grep -qE '[[:space:]]hvc[[:space:]]+#?(0x)?2' < <("$objdump" -d "$elf")
vector_size="$(stat -f '%z' "$vector")"
(( vector_size > 0 && vector_size <= VECTOR_LIMIT )) || {
  echo "DXE entry vector size ${vector_size} is outside 1..${VECTOR_LIMIT}" >&2
  exit 70
}
vector_sha256="$(shasum -a 256 "$vector" | awk '{print $1}')"
[[ "$vector_sha256" == "$EXPECTED_VECTOR_SHA256" ]] || {
  echo "DXE entry vector digest ${vector_sha256} does not match ${EXPECTED_VECTOR_SHA256}" >&2
  exit 70
}
mkdir -p "$output_dir"
artifact="$output_dir/BridgeVmPcDxeEntry.fd"
python3 - "$artifact" "$vector" "$fv" "$FLASH_SIZE" "$FV_OFFSET" "$FV_SIZE" <<'PY'
import pathlib
import sys
artifact = pathlib.Path(sys.argv[1])
vector = pathlib.Path(sys.argv[2]).read_bytes()
fv = pathlib.Path(sys.argv[3]).read_bytes()
size, offset, fv_size = map(int, sys.argv[4:])
assert len(fv) == fv_size and len(vector) <= offset
with artifact.open("wb") as stream:
    stream.write(b"\xff" * size)
    stream.seek(0)
    stream.write(vector)
    stream.seek(offset)
    stream.write(fv)
PY
artifact_sha256="$(shasum -a 256 "$artifact" | awk '{print $1}')"
[[ "$artifact_sha256" == "$EXPECTED_FD_SHA256" ]] || {
  echo "DXE entry FD digest ${artifact_sha256} does not match ${EXPECTED_FD_SHA256}" >&2
  exit 71
}
cp "$fv" "$output_dir/BridgeVmPcDxeEntry.fv"
receipt="$("$repo_root/scripts/write-bridgevm-pc-dxe-receipt.sh" \
  "$output_dir" "$repo_root" "$EXPECTED_EDK2_COMMIT" "$gcc_version" "$ld_version" \
  "$vector_size" "$vector_sha256" "$rebased_sha256" "$runtime_sha256" \
  "$variable_sha256" "$platform_sha256" "$pci_receipt_sha256" "$probe_sha256" "$fv_sha256" \
  "$artifact_sha256" "$FLASH_SIZE")"
echo "built $artifact"
echo "sha256 $artifact_sha256"
echo "vector_size $vector_size"
echo "fv_sha256 $fv_sha256"
echo "receipt $receipt"
