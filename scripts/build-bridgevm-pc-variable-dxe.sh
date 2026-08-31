#!/usr/bin/env bash
# Build the pinned generic variable service for the BridgeVM PC vars aperture.
set -euo pipefail

readonly EXPECTED_EDK2_COMMIT="b03a21a63e3bd001f52c527e5a57feddb53a690b"
readonly EXPECTED_BROTLI_COMMIT="e230f474b87134e8c6c85b630084c612057f253e"
readonly EXPECTED_MIPI_COMMIT="370b5944c046bab043dd8b133727b2135af7747a"
readonly SOURCE_DATE_EPOCH_PIN="1778208179"
readonly EXPECTED_GCC_VERSION="aarch64-elf-gcc (GCC) 16.1.0"
readonly EXPECTED_LD_VERSION="GNU ld (GNU Binutils) 2.46.1"
readonly EXPECTED_VARIABLE_SHA256="3d6f0fbd9d155f76d6f1001ee67fce25e36bd5aef20bea088421a772a7500a90"
readonly EXPECTED_DEPEX_SHA256="557c754d26e2667287367a856ea5fcd584f35ab796d24a6a875d1648a4637d23"

if [[ $# -ne 2 ]]; then
  echo "usage: $0 /path/to/pinned-edk2 OUTPUT_DIR" >&2
  exit 64
fi

edk2_root="$(cd "$1" && pwd)"
output_dir="$2"
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
package_root="$repo_root/crates/bridgevm-hvf/firmware"
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

build_root="$(mktemp -d /tmp/bridgevm-pc-variable-dxe.XXXXXX)"
trap 'rm -rf "$build_root"' EXIT
ln -s "$package_root" "$build_root/packages"
if ! make -C "$edk2_root/BaseTools" -j8 >"$build_root/base-tools.log" 2>&1; then
  tail -200 "$build_root/base-tools.log" >&2
  exit 68
fi
export WORKSPACE="$edk2_root"
export PACKAGES_PATH="$build_root/packages"
export GCC_AARCH64_PREFIX="/opt/homebrew/bin/aarch64-elf-"
export PYTHON_COMMAND="$(command -v python3)"
export SOURCE_DATE_EPOCH="$SOURCE_DATE_EPOCH_PIN"
cd "$edk2_root"
set +u
# shellcheck disable=SC1091
source ./edksetup.sh BaseTools >/dev/null
set -u
if ! build -a AARCH64 -t GCC -p BridgeVmPcPkg/BridgeVmPcRuntimeServices.dsc \
     -m MdeModulePkg/Universal/Variable/RuntimeDxe/VariableRuntimeDxe.inf \
     -b RELEASE -n 8 >"$build_root/build.log" 2>&1; then
  tail -200 "$build_root/build.log" >&2
  exit 69
fi

built_root="$edk2_root/Build/BridgeVmPcRuntimeServices/RELEASE_GCC/AARCH64"
built="$built_root/VariableRuntimeDxe.efi"
depex="$built_root/MdeModulePkg/Universal/Variable/RuntimeDxe/VariableRuntimeDxe/OUTPUT/VariableRuntimeDxe.depex"
[[ -f "$built" && -f "$depex" ]] || { echo "variable-service output is missing" >&2; exit 69; }
library_list="$built_root/MdeModulePkg/Universal/Variable/RuntimeDxe/VariableRuntimeDxe/OUTPUT/static_library_files.lst"
if ! grep -Fq '/BaseDebugLibNull/' "$library_list" ||
   grep -Fq '/UefiDebugLibConOut/' "$library_list"; then
  echo "variable service unexpectedly links console-backed debug code" >&2
  exit 69
fi
mkdir -p "$output_dir"
artifact="$output_dir/VariableRuntimeDxe.efi"
artifact_depex="$output_dir/VariableRuntimeDxe.depex"
cp "$built" "$artifact"
cp "$depex" "$artifact_depex"
"$tool_root/GenFw" -z -r "$artifact"
if ! /opt/homebrew/bin/aarch64-elf-objdump -f "$artifact" | grep -q 'file format pei-aarch64-little'; then
  echo "variable-service output is not an AArch64 PE/COFF image" >&2
  exit 70
fi
if strings -a "$artifact" | grep -i -E 'qemu|armvirt|ovmf|fw[_-]?cfg|u[t]m'; then
  echo "variable-service output contains a prohibited compatibility-platform reference" >&2
  exit 70
fi
artifact_sha256="$(shasum -a 256 "$artifact" | awk '{print $1}')"
depex_sha256="$(shasum -a 256 "$artifact_depex" | awk '{print $1}')"
[[ "$artifact_sha256" == "$EXPECTED_VARIABLE_SHA256" ]] || {
  echo "variable-service digest ${artifact_sha256} does not match ${EXPECTED_VARIABLE_SHA256}" >&2
  exit 71
}
[[ "$depex_sha256" == "$EXPECTED_DEPEX_SHA256" ]] || {
  echo "variable-service DEPEX digest ${depex_sha256} does not match ${EXPECTED_DEPEX_SHA256}" >&2
  exit 71
}
config_sha256="$(shasum -a 256 "$package_root/BridgeVmPcPkg/BridgeVmPcRuntimeServices.dsc" | awk '{print $1}')"
receipt="$output_dir/VariableRuntimeDxe.build.json"
printf '%s\n' '{' '  "schemaVersion": 1,' \
  '  "artifactKind": "development-only-preserved-backing-variable-runtime-dxe",' \
  "  \"edk2Commit\": \"${EXPECTED_EDK2_COMMIT}\"," \
  "  \"sourceDateEpoch\": ${SOURCE_DATE_EPOCH_PIN}," \
  "  \"gccVersion\": \"${gcc_version}\"," \
  "  \"ldVersion\": \"${ld_version}\"," \
  "  \"platformConfigSha256\": \"${config_sha256}\"," \
  "  \"size\": $(stat -f '%z' "$artifact")," \
  "  \"sha256\": \"${artifact_sha256}\"," \
  "  \"depexSha256\": \"${depex_sha256}\"," \
  '  "claimBoundary": "standalone variable services configured for the first 64 KiB of the BridgeVM vars aperture; this artifact alone does not prove integration, reboot persistence, power-failure atomicity, virtual-address transition, BDS, or Windows boot"' \
  '}' >"$receipt"
echo "built $artifact"
echo "sha256 $artifact_sha256"
echo "receipt $receipt"
