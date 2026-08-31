#!/usr/bin/env bash
# Build development-only BridgeVM PC firmware through BDS and ExitBootServices.
set -euo pipefail
readonly EDK2_COMMIT="b03a21a63e3bd001f52c527e5a57feddb53a690b"
readonly GCC_VERSION="aarch64-elf-gcc (GCC) 16.1.0"
readonly LD_VERSION="GNU ld (GNU Binutils) 2.46.1"
readonly CORE_SHA="cfe2ea1a7dc5573b4a5f6952e9177475ef91fb41da40c08e89c6f48bec2a4d90"
readonly VECTOR_SHA="5a9feed757d4c33f1868832357ba257c91a1b2c14dabb5b6836ee508aa26bff1"
readonly FV_SHA="65f61995770113efc01350cd2b1242145a11424204b097c0e0452412a1c3ac6d"
readonly FD_SHA="358d673e11e38b29a7eb5c38d95e6fd34cce0f310bd3e5290cc24a68212d0c29"
readonly MEDIA_SHA="a49be97db44c0d68b3382f3b1e46eba2fc7a3b12bcba14c1ec720f0511b71979"
readonly VARS_SHA="71189f7fb6aed638640078fba3a35fda6c39c8962e74dcc75935aac948da9063"
readonly FLASH_SIZE=$((0x04000000)) FV_OFFSET=$((0x00100000)) FV_SIZE=$((0x00100000))
if [[ $# -ne 2 ]]; then
  echo "usage: $0 /path/to/pinned-edk2 OUTPUT_DIR" >&2
  exit 64
fi
edk2="$(cd "$1" && pwd)"; output="$2"; repo="$(cd "$(dirname "$0")/.." && pwd)"
source_root="$repo/crates/bridgevm-hvf/firmware/BridgeVmPcPkg/ResetVector"
gcc=/opt/homebrew/bin/aarch64-elf-gcc; ld=/opt/homebrew/bin/aarch64-elf-ld
objcopy=/opt/homebrew/bin/aarch64-elf-objcopy; nm=/opt/homebrew/bin/aarch64-elf-nm
objdump=/opt/homebrew/bin/aarch64-elf-objdump; tools="$edk2/BaseTools/Source/C/bin"
[[ "$(git -C "$edk2" rev-parse HEAD)" == "$EDK2_COMMIT" ]] || {
  echo "refusing unpinned EDK2 source" >&2; exit 65;
}
gcc_version="$($gcc --version | head -1)"; ld_version="$($ld --version | head -1)"
[[ "$gcc_version" == "$GCC_VERSION" && "$ld_version" == "$LD_VERSION" ]] || {
  echo "refusing unpinned firmware tools" >&2; exit 66;
}
work="$(mktemp -d /tmp/bridgevm-pc-boot-firmware.XXXXXX)"
trap 'rm -rf "$work"' EXIT
"$repo/scripts/build-bridgevm-pc-dxe-core-fv.sh" "$edk2" "$work/core"
"$repo/scripts/build-bridgevm-pc-runtime-dxe.sh" "$edk2" "$work/runtime"
"$repo/scripts/build-bridgevm-pc-variable-dxe.sh" "$edk2" "$work/variables"
"$repo/scripts/build-bridgevm-pc-edk2-consumer.sh" "$edk2" "$work/drivers"
"$repo/scripts/build-bridgevm-pc-boot-modules.sh" "$edk2" "$work/boot"
"$tools/GenFw" --rebase 0x100400000 -r "$work/core/BridgeVmPcDxeCore.efi"
[[ "$(shasum -a 256 "$work/core/BridgeVmPcDxeCore.efi" | awk '{print $1}')" == "$CORE_SHA" ]] || {
  echo "rebased DXE Core digest changed" >&2; exit 67;
}
mkdir -p "$work/fv"
"$repo/scripts/build-bridgevm-pc-boot-fv.sh" "$edk2" "$work/fv" "$work/core" \
  "$work/runtime" "$work/variables" "$work/drivers" "$work/boot" "$work/artifacts"
fv="$work/artifacts/BridgeVmPcBoot.fv"
[[ "$(shasum -a 256 "$fv" | awk '{print $1}')" == "$FV_SHA" ]] || {
  echo "boot FV digest changed" >&2; exit 68;
}
"$repo/scripts/build-bridgevm-pc-boot-media.py" "$work/boot/BridgeVmPcExitBootServicesProbe.efi" "$work/BridgeVmPcBoot.img"
[[ "$(shasum -a 256 "$work/BridgeVmPcBoot.img" | awk '{print $1}')" == "$MEDIA_SHA" ]] || {
  echo "boot media digest changed" >&2; exit 68;
}
reset="$work/reset.o"; exception="$work/exception.o"; mmu="$work/mmu.o"; mmu_ram="$work/mmu-ram.o"
sec="$work/sec.o"; hob="$work/hob.o"; ipl="$work/ipl.o"; elf="$work/firmware.elf"
vector="$work/firmware.bin"
"$gcc" -c -x assembler-with-cpp -DBRIDGE_VM_PC_DXE_ENTRY -ffreestanding -fno-pic \
  "$source_root/BridgeVmPcResetVector.S" -o "$reset"
"$gcc" -c -x assembler-with-cpp -ffreestanding -fno-pic \
  "$source_root/BridgeVmPcExceptionVector.S" -o "$exception"
"$gcc" -c -x assembler-with-cpp -ffreestanding -fno-pic \
  "$source_root/BridgeVmPcMmu.S" -o "$mmu"
"$gcc" -c -x assembler-with-cpp -ffreestanding -fno-pic "$source_root/BridgeVmPcMmuRam.S" -o "$mmu_ram"
for item in "BridgeVmPcSec.c:$sec" "BridgeVmPcHob.c:$hob" "BridgeVmPcDxeIpl.c:$ipl"; do
  "$gcc" -c -O2 -Wall -Wextra -Werror -DBRIDGE_VM_PC_DXE_ENTRY -ffreestanding \
    -fno-builtin -fno-pic -fno-stack-protector "$source_root/${item%%:*}" -o "${item#*:}"
done
"$ld" --build-id=none -nostdlib -T "$source_root/BridgeVmPcDxeEntry.ld" \
  "$reset" "$exception" "$mmu" "$mmu_ram" "$sec" "$hob" "$ipl" -o "$elf"
"$objcopy" -O binary "$elf" "$vector"
[[ "$($nm -n "$elf" | awk '$3 == "_start" {print $1}')" == 0000000000000000 ]]
grep -qE '[[:space:]]hvc[[:space:]]+#?(0x)?2' < <("$objdump" -d "$elf")
[[ "$(shasum -a 256 "$vector" | awk '{print $1}')" == "$VECTOR_SHA" ]] || {
  echo "reset-vector digest changed" >&2; exit 69;
}
mkdir -p "$output"
fd="$output/BridgeVmPcBoot.fd"
vars="$output/BridgeVmPcBootVars.fd"
python3 - "$fd" "$vars" "$vector" "$fv" "$FLASH_SIZE" "$FV_OFFSET" "$FV_SIZE" <<'PY'
import pathlib, sys
out, vars_out, vector, fv = map(pathlib.Path, sys.argv[1:5]); size, offset, fv_size = map(int, sys.argv[5:])
v=vector.read_bytes(); f=fv.read_bytes()
assert len(v) <= offset and len(f) == fv_size
with out.open('wb') as stream:
    stream.write(b'\xff' * size); stream.seek(0); stream.write(v); stream.seek(offset); stream.write(f)
vars_out.write_bytes(b'\xff' * 0x10000)
PY
[[ "$(shasum -a 256 "$fd" | awk '{print $1}')" == "$FD_SHA" ]] || {
  echo "boot firmware digest changed" >&2; exit 70;
}
[[ "$(shasum -a 256 "$vars" | awk '{print $1}')" == "$VARS_SHA" ]] || {
  echo "boot vars digest changed" >&2; exit 70;
}
cp "$fv" "$output/BridgeVmPcBoot.fv"
cp "$work/BridgeVmPcBoot.img" "$output/BridgeVmPcBoot.img"
cp "$work/boot/BridgeVmPcBootModules.build.json" "$output/"
cat >"$output/BridgeVmPcBoot.build.json" <<EOF
{
  "schemaVersion": 1,
  "artifactKind": "development-only-independent-board-bds-boot-probe",
  "sourceCommit": "$(git -C "$repo" rev-parse HEAD)",
  "edk2Commit": "$EDK2_COMMIT",
  "firmwareSha256": "$FD_SHA",
  "firmwareVolumeSha256": "$FV_SHA",
  "bootMediaSha256": "$MEDIA_SHA",
  "initialVarsSha256": "$VARS_SHA",
  "bootApplicationSha256": "93f86906c18acdc8be76466ad5ff63f50358c52a68583cea703c1b97696fff85",
  "claimBoundary": "development-only BDS/ESP/PE/ExitBootServices probe; no Windows boot or production-signing claim"
}
EOF
echo "built $fd"
echo "firmware_sha256 $FD_SHA"
echo "boot_media_sha256 $MEDIA_SHA"
echo "initial_vars_sha256 $VARS_SHA"
