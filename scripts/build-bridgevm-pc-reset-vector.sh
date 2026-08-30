#!/usr/bin/env bash
# Build the BridgeVM-owned AArch64 reset and SEC entry into the 64 MiB
# flash-code image. This development FD is not complete UEFI firmware.
set -euo pipefail
readonly EXPECTED_GCC_VERSION="aarch64-elf-gcc (GCC) 16.1.0"
readonly EXPECTED_LD_VERSION="GNU ld (GNU Binutils) 2.46.1"
readonly EXPECTED_FD_SHA256="8db976249ff86c9613d0a13a0d811ee68a94ef90835d524deb451b927bc332d6"
readonly FLASH_SIZE=$((0x04000000))
readonly VECTOR_LIMIT=$((0x1000))
if [[ $# -ne 1 ]]; then
  echo "usage: $0 OUTPUT_DIR" >&2
  exit 64
fi
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
source_root="$repo_root/crates/bridgevm-hvf/firmware/BridgeVmPcPkg/ResetVector"
output_dir="$1"
gcc="/opt/homebrew/bin/aarch64-elf-gcc"
ld="/opt/homebrew/bin/aarch64-elf-ld"
objcopy="/opt/homebrew/bin/aarch64-elf-objcopy"
nm="/opt/homebrew/bin/aarch64-elf-nm"
objdump="/opt/homebrew/bin/aarch64-elf-objdump"
gcc_version="$($gcc --version | head -1)"
ld_version="$($ld --version | head -1)"
if [[ "$gcc_version" != "$EXPECTED_GCC_VERSION" || "$ld_version" != "$EXPECTED_LD_VERSION" ]]; then
  echo "refusing unpinned reset-vector tools: gcc='${gcc_version}' ld='${ld_version}'" >&2
  exit 65
fi
if rg -n -i 'qemu|armvirt|ovmf|fw[_-]?cfg|u[t]m' "$source_root"; then
  echo "reset-vector source contains a prohibited compatibility-platform reference" >&2
  exit 66
fi
build_root="$(mktemp -d "/tmp/bridgevm-pc-reset-vector.XXXXXX")"
trap 'rm -rf "$build_root"' EXIT
object="$build_root/reset-vector.o"
sec_object="$build_root/sec.o"
hob_object="$build_root/hob.o"
elf="$build_root/reset-vector.elf"
vector="$build_root/reset-vector.bin"

"$gcc" -c -x assembler-with-cpp -ffreestanding -fno-pic \
  "$source_root/BridgeVmPcResetVector.S" -o "$object"
"$gcc" -c -O2 -Wall -Wextra -Werror -ffreestanding -fno-builtin -fno-pic -fno-stack-protector \
  "$source_root/BridgeVmPcSec.c" -o "$sec_object"
"$gcc" -c -O2 -Wall -Wextra -Werror -ffreestanding -fno-builtin -fno-pic -fno-stack-protector \
  "$source_root/BridgeVmPcHob.c" -o "$hob_object"
"$ld" --build-id=none -nostdlib \
  -T "$source_root/BridgeVmPcResetVector.ld" "$object" "$sec_object" "$hob_object" -o "$elf"
"$objcopy" -O binary "$elf" "$vector"

start_address="$($nm -n "$elf" | awk '$3 == "_start" {print $1}')"
[[ "$start_address" == "0000000000000000" ]] || {
  echo "reset vector linked at ${start_address:-missing}, expected zero" >&2
  exit 67
}
"$objdump" -d "$elf" | grep -qE '[[:space:]]hvc[[:space:]]+#?0x?0'
vector_size="$(stat -f '%z' "$vector")"
(( vector_size > 0 && vector_size <= VECTOR_LIMIT )) || {
  echo "reset vector size ${vector_size} is outside 1..${VECTOR_LIMIT}" >&2
  exit 67
}
mkdir -p "$output_dir"
artifact="$output_dir/BridgeVmPcResetVector.fd"
python3 - "$artifact" "$vector" "$FLASH_SIZE" <<'PY'
import pathlib
import sys

artifact = pathlib.Path(sys.argv[1])
vector = pathlib.Path(sys.argv[2]).read_bytes()
size = int(sys.argv[3])
chunk = b"\xff" * (1024 * 1024)
with artifact.open("wb") as stream:
    for _ in range(size // len(chunk)):
        stream.write(chunk)
    stream.seek(0)
    stream.write(vector)
PY

artifact_size="$(stat -f '%z' "$artifact")"
[[ "$artifact_size" -eq "$FLASH_SIZE" ]] || {
  echo "reset-vector FD has size ${artifact_size}, expected ${FLASH_SIZE}" >&2
  exit 68
}
if strings -a "$artifact" | rg -i 'qemu|armvirt|ovmf|fw[_-]?cfg|u[t]m'; then
  echo "reset-vector FD contains a prohibited compatibility-platform reference" >&2
  exit 68
fi

vector_sha256="$(shasum -a 256 "$vector" | awk '{print $1}')"
artifact_sha256="$(shasum -a 256 "$artifact" | awk '{print $1}')"
[[ "$artifact_sha256" == "$EXPECTED_FD_SHA256" ]] || {
  echo "reset-vector FD digest ${artifact_sha256} does not match ${EXPECTED_FD_SHA256}" >&2
  exit 69
}

source_tree_sha256="$(python3 - "$source_root" <<'PY'
import hashlib
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
digest = hashlib.sha256()
for path in sorted(item for item in root.rglob("*") if item.is_file()):
    name = path.relative_to(root).as_posix().encode()
    data = path.read_bytes()
    digest.update(len(name).to_bytes(4, "big"))
    digest.update(name)
    digest.update(len(data).to_bytes(8, "big"))
    digest.update(data)
print(digest.hexdigest())
PY
)"

receipt="$output_dir/BridgeVmPcResetVector.build.json"
printf '%s\n' \
  '{' \
  '  "schemaVersion": 1,' \
  '  "artifactKind": "development-only-reset-sec-hob-fd",' \
  '  "boardId": "com.ketchio.bridgevm.virtual-arm-pc",' \
  '  "boardAbi": 1,' \
  '  "resetVectorGpa": 0,' \
  "  \"gccVersion\": \"${gcc_version}\"," \
  "  \"ldVersion\": \"${ld_version}\"," \
  "  \"sourceTreeSha256\": \"${source_tree_sha256}\"," \
  "  \"vectorSize\": ${vector_size}," \
  "  \"vectorSha256\": \"${vector_sha256}\"," \
  "  \"size\": ${artifact_size}," \
  "  \"sha256\": \"${artifact_sha256}\"," \
  '  "claimBoundary": "reset, bounded SEC validation, and bounded PI HOB construction only; no firmware volume, PEI, DXE, UEFI service, or Windows boot claim"' \
  '}' > "$receipt"

echo "built $artifact"
echo "sha256 $artifact_sha256"
echo "vector_size $vector_size"
echo "receipt $receipt"
