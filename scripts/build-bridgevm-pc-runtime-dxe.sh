#!/usr/bin/env bash
# Build the pinned generic RuntimeDxe used by the BridgeVM PC firmware probe.
set -euo pipefail

readonly EXPECTED_EDK2_COMMIT="b03a21a63e3bd001f52c527e5a57feddb53a690b"
readonly EXPECTED_BROTLI_COMMIT="e230f474b87134e8c6c85b630084c612057f253e"
readonly EXPECTED_MIPI_COMMIT="370b5944c046bab043dd8b133727b2135af7747a"
readonly SOURCE_DATE_EPOCH_PIN="1778208179"
readonly EXPECTED_GCC_VERSION="aarch64-elf-gcc (GCC) 16.1.0"
readonly EXPECTED_LD_VERSION="GNU ld (GNU Binutils) 2.46.1"
readonly EXPECTED_RUNTIME_SHA256="1ec9344d12805f32ae7e68a1a0d755ad00f5b7b59414a75cebfaaaae19ffab75"
readonly EXPECTED_DEPEX_SHA256="557c754d26e2667287367a856ea5fcd584f35ab796d24a6a875d1648a4637d23"

if [[ $# -ne 2 ]]; then
  echo "usage: $0 /path/to/pinned-edk2 OUTPUT_DIR" >&2
  exit 64
fi

edk2_root="$(cd "$1" && pwd)"
output_dir="$2"
brotli_root="$edk2_root/BaseTools/Source/C/BrotliCompress/brotli"
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

require_commit EDK2 "$edk2_root" "$EXPECTED_EDK2_COMMIT"
require_commit "BaseTools brotli" "$brotli_root" "$EXPECTED_BROTLI_COMMIT"
require_commit "MIPI Sys-T" "$mipi_root" "$EXPECTED_MIPI_COMMIT"
if ! git -C "$edk2_root" diff --quiet --ignore-submodules=none ||
   ! git -C "$edk2_root" diff --cached --quiet --ignore-submodules=none; then
  echo "refusing a dirty EDK2 checkout" >&2
  exit 66
fi

gcc_version="$(/opt/homebrew/bin/aarch64-elf-gcc --version | head -1)"
ld_version="$(/opt/homebrew/bin/aarch64-elf-ld --version | head -1)"
if [[ "$gcc_version" != "$EXPECTED_GCC_VERSION" || "$ld_version" != "$EXPECTED_LD_VERSION" ]]; then
  echo "refusing unpinned firmware tools: gcc='${gcc_version}' ld='${ld_version}'" >&2
  exit 67
fi

build_root="$(mktemp -d /tmp/bridgevm-pc-runtime-dxe.XXXXXX)"
trap 'rm -rf "$build_root"' EXIT
if ! make -C "$edk2_root/BaseTools" -j8 >"$build_root/base-tools.log" 2>&1; then
  tail -200 "$build_root/base-tools.log" >&2
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
if ! build -a AARCH64 -t GCC -p MdeModulePkg/MdeModulePkg.dsc \
     -m MdeModulePkg/Core/RuntimeDxe/RuntimeDxe.inf -b RELEASE -n 8 \
     >"$build_root/build.log" 2>&1; then
  tail -200 "$build_root/build.log" >&2
  exit 69
fi

built_root="$edk2_root/Build/MdeModule/RELEASE_GCC/AARCH64"
built="$built_root/RuntimeDxe.efi"
depex="$built_root/MdeModulePkg/Core/RuntimeDxe/RuntimeDxe/OUTPUT/RuntimeDxe.depex"
[[ -f "$built" && -f "$depex" ]] || { echo "RuntimeDxe output is missing" >&2; exit 69; }
mkdir -p "$output_dir"
artifact="$output_dir/RuntimeDxe.efi"
artifact_depex="$output_dir/RuntimeDxe.depex"
cp "$built" "$artifact"
cp "$depex" "$artifact_depex"
"$tool_root/GenFw" -z -r "$artifact"
if ! /opt/homebrew/bin/aarch64-elf-objdump -f "$artifact" | grep -q 'file format pei-aarch64-little'; then
  echo "RuntimeDxe output is not an AArch64 PE/COFF image" >&2
  exit 70
fi
if strings -a "$artifact" | grep -i -E 'qemu|armvirt|ovmf|fw[_-]?cfg|u[t]m'; then
  echo "RuntimeDxe contains a prohibited compatibility-platform reference" >&2
  exit 70
fi
artifact_sha256="$(shasum -a 256 "$artifact" | awk '{print $1}')"
depex_sha256="$(shasum -a 256 "$artifact_depex" | awk '{print $1}')"
[[ "$artifact_sha256" == "$EXPECTED_RUNTIME_SHA256" ]] || {
  echo "RuntimeDxe digest ${artifact_sha256} does not match ${EXPECTED_RUNTIME_SHA256}" >&2
  exit 71
}
[[ "$depex_sha256" == "$EXPECTED_DEPEX_SHA256" ]] || {
  echo "RuntimeDxe DEPEX digest ${depex_sha256} does not match ${EXPECTED_DEPEX_SHA256}" >&2
  exit 71
}
receipt="$output_dir/RuntimeDxe.build.json"
printf '%s\n' '{' '  "schemaVersion": 1,' \
  '  "artifactKind": "development-only-generic-runtime-dxe",' \
  "  \"edk2Commit\": \"${EXPECTED_EDK2_COMMIT}\"," \
  "  \"sourceDateEpoch\": ${SOURCE_DATE_EPOCH_PIN}," \
  "  \"gccVersion\": \"${gcc_version}\"," \
  "  \"ldVersion\": \"${ld_version}\"," \
  "  \"size\": $(stat -f '%z' "$artifact")," \
  "  \"sha256\": \"${artifact_sha256}\"," \
  "  \"depexSha256\": \"${depex_sha256}\"," \
  '  "claimBoundary": "generic Runtime Architectural Protocol module build only; no variable, reset, time, boot-manager, or Windows claim"' \
  '}' >"$receipt"
echo "built $artifact"
echo "sha256 $artifact_sha256"
echo "receipt $receipt"
