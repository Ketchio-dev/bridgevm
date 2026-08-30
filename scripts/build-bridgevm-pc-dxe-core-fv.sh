#!/usr/bin/env bash
# Build a pinned generic AArch64 DXE Core and package it in a PI firmware
# volume. This build-only artifact is not connected to the reset path yet.
set -euo pipefail

readonly EXPECTED_EDK2_COMMIT="b03a21a63e3bd001f52c527e5a57feddb53a690b"
readonly EXPECTED_BROTLI_COMMIT="e230f474b87134e8c6c85b630084c612057f253e"
readonly EXPECTED_MIPI_COMMIT="370b5944c046bab043dd8b133727b2135af7747a"
readonly SOURCE_DATE_EPOCH_PIN="1778208179"
readonly EXPECTED_GCC_VERSION="aarch64-elf-gcc (GCC) 16.1.0"
readonly EXPECTED_LD_VERSION="GNU ld (GNU Binutils) 2.46.1"
readonly EXPECTED_DXE_SHA256="c882629072e592ca85a62ac27f5bf5ea6210687ffcc8651b7bd1bdf73754bb02"
readonly EXPECTED_FV_SHA256="022e09f7e60c3f1cf5b1416a66714b642714e827ba085957383ea3264f3f4ed6"
readonly DXE_CORE_GUID="D6A2CB7F-6A18-4e2f-B43B-9920A733700A"
readonly FFS2_GUID="8c8ce578-8a3d-4f1c-9935-896185c32dd3"
readonly FV_NAME_GUID="7D2A7E6B-9B08-4C1F-AED5-799718B43F33"

if [[ $# -ne 2 ]]; then
  echo "usage: $0 /path/to/pinned-edk2 OUTPUT_DIR" >&2
  exit 64
fi

edk2_root="$(cd "$1" && pwd)"
output_dir="$2"
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
base_brotli="$edk2_root/BaseTools/Source/C/BrotliCompress/brotli"
module_brotli="$edk2_root/MdeModulePkg/Library/BrotliCustomDecompressLib/brotli"
mipi_root="$edk2_root/MdePkg/Library/MipiSysTLib/mipisyst"
tool_root="$edk2_root/BaseTools/Source/C/bin"

require_commit() {
  local label="$1" path="$2" expected="$3" actual
  [[ -d "$path/.git" || -f "$path/.git" ]] || {
    echo "missing pinned ${label} checkout: ${path}" >&2
    exit 65
  }
  actual="$(git -C "$path" rev-parse HEAD)"
  [[ "$actual" == "$expected" ]] || {
    echo "refusing ${label} commit ${actual}; expected ${expected}" >&2
    exit 65
  }
}

require_commit "EDK2" "$edk2_root" "$EXPECTED_EDK2_COMMIT"
require_commit "BaseTools brotli" "$base_brotli" "$EXPECTED_BROTLI_COMMIT"
require_commit "MdeModulePkg brotli" "$module_brotli" "$EXPECTED_BROTLI_COMMIT"
require_commit "MIPI Sys-T" "$mipi_root" "$EXPECTED_MIPI_COMMIT"
if ! git -C "$edk2_root" diff --quiet --ignore-submodules=none ||
   ! git -C "$edk2_root" diff --cached --quiet --ignore-submodules=none; then
  echo "refusing a dirty EDK2 checkout" >&2
  exit 66
fi

gcc="/opt/homebrew/bin/aarch64-elf-gcc"
ld="/opt/homebrew/bin/aarch64-elf-ld"
objdump="/opt/homebrew/bin/aarch64-elf-objdump"
gcc_version="$($gcc --version | head -1)"
ld_version="$($ld --version | head -1)"
if [[ "$gcc_version" != "$EXPECTED_GCC_VERSION" || "$ld_version" != "$EXPECTED_LD_VERSION" ]]; then
  echo "refusing unpinned firmware tools: gcc='${gcc_version}' ld='${ld_version}'" >&2
  exit 67
fi

build_root="$(mktemp -d "/tmp/bridgevm-pc-dxe-core.XXXXXX")"
trap 'rm -rf "$build_root"' EXIT
base_tools_log="$build_root/base-tools.log"
if ! make -C "$edk2_root/BaseTools" -j8 >"$base_tools_log" 2>&1; then
  tail -200 "$base_tools_log" >&2
  exit 68
fi

export WORKSPACE="$edk2_root"
export GCC_AARCH64_PREFIX="/opt/homebrew/bin/aarch64-elf-"
export PYTHON_COMMAND="$(command -v python3)"
export SOURCE_DATE_EPOCH="$SOURCE_DATE_EPOCH_PIN"
cd "$edk2_root"
set +u
# shellcheck disable=SC1091
source ./edksetup.sh BaseTools >/dev/null
set -u
build_log="$build_root/build.log"
if ! build -a AARCH64 -t GCC -p MdeModulePkg/MdeModulePkg.dsc \
     -m MdeModulePkg/Core/Dxe/DxeMain.inf -b RELEASE -n 8 >"$build_log" 2>&1; then
  tail -200 "$build_log" >&2
  exit 69
fi

built="$edk2_root/Build/MdeModule/RELEASE_GCC/AARCH64/DxeCore.efi"
[[ -f "$built" ]] || { echo "expected DXE Core is missing: ${built}" >&2; exit 69; }
cp "$built" "$build_root/DxeCore.efi"
"$tool_root/GenFw" -z -r "$build_root/DxeCore.efi"
"$tool_root/GenSec" -s EFI_SECTION_PE32 -o "$build_root/DxeCore.pe32" \
  "$build_root/DxeCore.efi"
"$tool_root/GenFfs" -t EFI_FV_FILETYPE_DXE_CORE -g "$DXE_CORE_GUID" \
  -i "$build_root/DxeCore.pe32" -o "$build_root/DxeCore.ffs"
"$tool_root/GenFv" -o "$build_root/BridgeVmPcDxeCore.fv" -b 0x1000 -n 0x100 \
  -f "$build_root/DxeCore.ffs" -g "$FFS2_GUID" --FvNameGuid "$FV_NAME_GUID"

if ! "$objdump" -f "$build_root/DxeCore.efi" | grep -q 'file format pei-aarch64-little'; then
  echo "DXE Core output is not an AArch64 PE/COFF image" >&2
  exit 70
fi
if strings -a "$build_root/DxeCore.efi" | rg -i 'qemu|armvirt|ovmf|fw[_-]?cfg|u[t]m'; then
  echo "DXE Core contains a prohibited compatibility-platform reference" >&2
  exit 70
fi

python3 - "$build_root/BridgeVmPcDxeCore.fv" <<'PY'
import pathlib
import struct
import sys

data = pathlib.Path(sys.argv[1]).read_bytes()
assert len(data) == 0x100000
assert struct.unpack_from("<Q", data, 0x20)[0] == len(data)
assert data[0x28:0x2c] == b"_FVH"
header_size = struct.unpack_from("<H", data, 0x30)[0]
assert header_size == 0x48
assert sum(struct.unpack(f"<{header_size // 2}H", data[:header_size])) & 0xffff == 0
ffs2 = bytes.fromhex("78e58c8c3d8a1c4f9935896185c32dd3")
dxe = bytes.fromhex("7fcba2d6186a2f4eb43b9920a733700a")
assert data[0x10:0x20] == ffs2
assert dxe in data
PY

dxe_sha256="$(shasum -a 256 "$build_root/DxeCore.efi" | awk '{print $1}')"
fv_sha256="$(shasum -a 256 "$build_root/BridgeVmPcDxeCore.fv" | awk '{print $1}')"
[[ "$dxe_sha256" == "$EXPECTED_DXE_SHA256" ]] || {
  echo "DXE Core digest ${dxe_sha256} does not match ${EXPECTED_DXE_SHA256}" >&2
  exit 71
}
[[ "$fv_sha256" == "$EXPECTED_FV_SHA256" ]] || {
  echo "DXE FV digest ${fv_sha256} does not match ${EXPECTED_FV_SHA256}" >&2
  exit 71
}

mkdir -p "$output_dir"
dxe_artifact="$output_dir/BridgeVmPcDxeCore.efi"
fv_artifact="$output_dir/BridgeVmPcDxeCore.fv"
cp "$build_root/DxeCore.efi" "$dxe_artifact"
cp "$build_root/BridgeVmPcDxeCore.fv" "$fv_artifact"
dxe_size="$(stat -f '%z' "$dxe_artifact")"
fv_size="$(stat -f '%z' "$fv_artifact")"
script_sha256="$(shasum -a 256 "$repo_root/scripts/build-bridgevm-pc-dxe-core-fv.sh" | awk '{print $1}')"

receipt="$output_dir/BridgeVmPcDxeCore.build.json"
printf '%s\n' \
  '{' \
  '  "schemaVersion": 1,' \
  '  "artifactKind": "development-only-uefi-dxe-core-fv",' \
  "  \"edk2Commit\": \"${EXPECTED_EDK2_COMMIT}\"," \
  "  \"baseToolsBrotliCommit\": \"${EXPECTED_BROTLI_COMMIT}\"," \
  "  \"moduleBrotliCommit\": \"${EXPECTED_BROTLI_COMMIT}\"," \
  "  \"mipiSysTCommit\": \"${EXPECTED_MIPI_COMMIT}\"," \
  "  \"sourceDateEpoch\": ${SOURCE_DATE_EPOCH_PIN}," \
  '  "platform": "MdeModulePkg/MdeModulePkg.dsc",' \
  '  "module": "MdeModulePkg/Core/Dxe/DxeMain.inf",' \
  '  "architecture": "AARCH64",' \
  '  "target": "RELEASE",' \
  "  \"gccVersion\": \"${gcc_version}\"," \
  "  \"ldVersion\": \"${ld_version}\"," \
  "  \"buildScriptSha256\": \"${script_sha256}\"," \
  "  \"dxeCoreSize\": ${dxe_size}," \
  "  \"dxeCoreSha256\": \"${dxe_sha256}\"," \
  "  \"firmwareVolumeSize\": ${fv_size}," \
  "  \"firmwareVolumeSha256\": \"${fv_sha256}\"," \
  '  "claimBoundary": "build-only DXE Core firmware volume; not integrated into reset, no DXE entry, UEFI service, or Windows boot claim"' \
  '}' > "$receipt"

echo "built $dxe_artifact"
echo "dxe_sha256 $dxe_sha256"
echo "built $fv_artifact"
echo "fv_sha256 $fv_sha256"
echo "receipt $receipt"
