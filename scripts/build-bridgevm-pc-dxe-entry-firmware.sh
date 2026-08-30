#!/usr/bin/env bash
# Build the development-only BridgeVM PC reset-to-DXE-dispatch probe firmware.
set -euo pipefail

readonly EXPECTED_EDK2_COMMIT="b03a21a63e3bd001f52c527e5a57feddb53a690b"
readonly EXPECTED_GCC_VERSION="aarch64-elf-gcc (GCC) 16.1.0"
readonly EXPECTED_LD_VERSION="GNU ld (GNU Binutils) 2.46.1"
readonly EXPECTED_REBASED_DXE_SHA256="b4ca5c00ef7e1b4104776005fe3c07978c78e39d92d2f035cfa72edabdf77d10"
readonly EXPECTED_PROBE_SHA256="463912d8120a00dbcf1cc2493857b318b092889fcd473df53fc1bfa363c4afac"
readonly EXPECTED_VECTOR_SHA256="a8d8a79279903253dd7dcc4d34a43aa5c00ac597cf45db613c9d23f03c69ddba"
readonly EXPECTED_FV_SHA256="181e8f906e16e412afbd4fae9fd418e5f322ca5a823db1b7f60a737b9916a413"
readonly EXPECTED_FD_SHA256="57c134b8f3f42bb9bb020936d4d87926b0d6563bfa0339bb110996a6e4ed6da6"
readonly DXE_CORE_GUID="D6A2CB7F-6A18-4e2f-B43B-9920A733700A"
readonly DXE_PROBE_GUID="36A32D32-548B-4970-A32A-68B01E131B4A"
readonly FFS2_GUID="8c8ce578-8a3d-4f1c-9935-896185c32dd3"
readonly FV_NAME_GUID="7D2A7E6B-9B08-4C1F-AED5-799718B43F33"
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
if rg -n -i 'qemu|armvirt|ovmf|fw[_-]?cfg|u[t]m' \
     "$repo_root/crates/bridgevm-hvf/firmware/BridgeVmPcPkg"; then
  echo "BridgeVmPcPkg contains a prohibited compatibility-platform reference" >&2
  exit 67
fi

build_root="$(mktemp -d "/tmp/bridgevm-pc-dxe-entry.XXXXXX")"
trap 'rm -rf "$build_root"' EXIT
"$repo_root/scripts/build-bridgevm-pc-dxe-core-fv.sh" \
  "$edk2_root" "$build_root/core"
"$repo_root/scripts/build-bridgevm-pc-edk2-consumer.sh" \
  "$edk2_root" "$build_root/drivers"

core="$build_root/DxeCore.efi"
probe="$build_root/DxeProbe.efi"
cp "$build_root/core/BridgeVmPcDxeCore.efi" "$core"
cp "$edk2_root/Build/BridgeVmPc/RELEASE_GCC/AARCH64/BridgeVmPcDxeProbe.efi" "$probe"
"$tool_root/GenFw" --rebase 0x100400000 -r "$core"
"$tool_root/GenFw" -z -r "$probe"
rebased_sha256="$(shasum -a 256 "$core" | awk '{print $1}')"
probe_sha256="$(shasum -a 256 "$probe" | awk '{print $1}')"
[[ "$rebased_sha256" == "$EXPECTED_REBASED_DXE_SHA256" ]] || {
  echo "rebased DXE Core digest ${rebased_sha256} does not match ${EXPECTED_REBASED_DXE_SHA256}" >&2
  exit 68
}
[[ "$probe_sha256" == "$EXPECTED_PROBE_SHA256" ]] || {
  echo "DXE probe digest ${probe_sha256} does not match ${EXPECTED_PROBE_SHA256}" >&2
  exit 68
}

probe_depex="$edk2_root/Build/BridgeVmPc/RELEASE_GCC/AARCH64/BridgeVmPcPkg/Drivers/DxeProbe/DxeProbe/OUTPUT/BridgeVmPcDxeProbe.depex"
"$tool_root/GenSec" -s EFI_SECTION_PE32 -o "$build_root/DxeCore.pe32" "$core"
"$tool_root/GenFfs" -t EFI_FV_FILETYPE_DXE_CORE -g "$DXE_CORE_GUID" \
  -i "$build_root/DxeCore.pe32" -o "$build_root/DxeCore.ffs"
"$tool_root/GenSec" -s EFI_SECTION_DXE_DEPEX -o "$build_root/DxeProbe.depex" \
  "$probe_depex"
"$tool_root/GenSec" -s EFI_SECTION_PE32 -o "$build_root/DxeProbe.pe32" "$probe"
"$tool_root/GenFfs" -t EFI_FV_FILETYPE_DRIVER -g "$DXE_PROBE_GUID" \
  -i "$build_root/DxeProbe.depex" -i "$build_root/DxeProbe.pe32" \
  -o "$build_root/DxeProbe.ffs"
fv="$build_root/BridgeVmPcDxeEntry.fv"
"$tool_root/GenFv" -o "$fv" -b 0x1000 -n 0x100 \
  -f "$build_root/DxeCore.ffs" -f "$build_root/DxeProbe.ffs" \
  -g "$FFS2_GUID" --FvNameGuid "$FV_NAME_GUID"

python3 - "$core" "$probe" "$fv" <<'PY'
import pathlib
import struct
import sys

core = pathlib.Path(sys.argv[1]).read_bytes()
probe = pathlib.Path(sys.argv[2]).read_bytes()
fv = pathlib.Path(sys.argv[3]).read_bytes()
assert len(core) == 0x17000 and len(probe) == 0x3000 and len(fv) == 0x100000
assert fv.find(core) == 0x94 and probe in fv
assert struct.unpack_from("<Q", fv, 0x20)[0] == len(fv)
assert fv[0x28:0x2c] == b"_FVH"
header_size = struct.unpack_from("<H", fv, 0x30)[0]
assert header_size == 0x48
assert sum(struct.unpack(f"<{header_size // 2}H", fv[:header_size])) & 0xffff == 0
pe_offset = struct.unpack_from("<I", core, 0x3c)[0]
optional = pe_offset + 24
assert core[pe_offset:pe_offset + 4] == b"PE\0\0"
assert struct.unpack_from("<I", core, optional + 16)[0] == 0x6bec
assert struct.unpack_from("<Q", core, optional + 24)[0] == 0x100400000
assert struct.unpack_from("<I", core, optional + 56)[0] == len(core)
PY

fv_sha256="$(shasum -a 256 "$fv" | awk '{print $1}')"
[[ "$fv_sha256" == "$EXPECTED_FV_SHA256" ]] || {
  echo "DXE entry FV digest ${fv_sha256} does not match ${EXPECTED_FV_SHA256}" >&2
  exit 69
}

reset_object="$build_root/reset.o"
exception_object="$build_root/exception.o"
sec_object="$build_root/sec.o"
hob_object="$build_root/hob.o"
ipl_object="$build_root/dxe-ipl.o"
elf="$build_root/firmware.elf"
vector="$build_root/firmware.bin"
"$gcc" -c -x assembler-with-cpp -DBRIDGE_VM_PC_DXE_ENTRY -ffreestanding -fno-pic \
  "$source_root/BridgeVmPcResetVector.S" -o "$reset_object"
"$gcc" -c -x assembler-with-cpp -ffreestanding -fno-pic \
  "$source_root/BridgeVmPcExceptionVector.S" -o "$exception_object"
for item in "BridgeVmPcSec.c:$sec_object" "BridgeVmPcHob.c:$hob_object" \
            "BridgeVmPcDxeIpl.c:$ipl_object"; do
  source_file="${item%%:*}"
  object_file="${item#*:}"
  "$gcc" -c -O2 -Wall -Wextra -Werror -ffreestanding -fno-builtin -fno-pic \
    -fno-stack-protector "$source_root/$source_file" -o "$object_file"
done
"$ld" --build-id=none -nostdlib -T "$source_root/BridgeVmPcDxeEntry.ld" \
  "$reset_object" "$exception_object" "$sec_object" "$hob_object" "$ipl_object" -o "$elf"
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
source_tree_sha256="$(python3 - "$repo_root/crates/bridgevm-hvf/firmware/BridgeVmPcPkg" <<'PY'
import hashlib
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
digest = hashlib.sha256()
for path in sorted(item for item in root.rglob("*") if item.is_file()):
    name = path.relative_to(root).as_posix().encode()
    data = path.read_bytes()
    digest.update(len(name).to_bytes(4, "big") + name)
    digest.update(len(data).to_bytes(8, "big") + data)
print(digest.hexdigest())
PY
)"

receipt="$output_dir/BridgeVmPcDxeEntry.build.json"
printf '%s\n' \
  '{' \
  '  "schemaVersion": 1,' \
  '  "artifactKind": "development-only-reset-to-dxe-dispatch-probe",' \
  "  \"edk2Commit\": \"${EXPECTED_EDK2_COMMIT}\"," \
  "  \"gccVersion\": \"${gcc_version}\"," \
  "  \"ldVersion\": \"${ld_version}\"," \
  "  \"sourceTreeSha256\": \"${source_tree_sha256}\"," \
  "  \"resetVectorSize\": ${vector_size}," \
  "  \"resetVectorSha256\": \"${vector_sha256}\"," \
  "  \"rebasedDxeCoreSha256\": \"${rebased_sha256}\"," \
  "  \"dxeProbeSha256\": \"${probe_sha256}\"," \
  "  \"firmwareVolumeSha256\": \"${fv_sha256}\"," \
  "  \"size\": ${FLASH_SIZE}," \
  "  \"sha256\": \"${artifact_sha256}\"," \
  '  "claimBoundary": "bounded DXE Core dispatch probe only; no architectural-protocol, UEFI boot-service completeness, boot manager, or Windows boot claim"' \
  '}' > "$receipt"

echo "built $artifact"
echo "sha256 $artifact_sha256"
echo "vector_size $vector_size"
echo "fv_sha256 $fv_sha256"
echo "receipt $receipt"
