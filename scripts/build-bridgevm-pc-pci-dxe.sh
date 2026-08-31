#!/usr/bin/env bash
# Build the pinned standard UEFI PCI stack for the BridgeVM Virtual ARM PC.
set -euo pipefail

readonly EXPECTED_EDK2_COMMIT="b03a21a63e3bd001f52c527e5a57feddb53a690b"
readonly EXPECTED_BROTLI_COMMIT="e230f474b87134e8c6c85b630084c612057f253e"
readonly EXPECTED_MIPI_COMMIT="370b5944c046bab043dd8b133727b2135af7747a"
readonly SOURCE_DATE_EPOCH_PIN="1778208179"
readonly EXPECTED_GCC_VERSION="aarch64-elf-gcc (GCC) 16.1.0"
readonly EXPECTED_LD_VERSION="GNU ld (GNU Binutils) 2.46.1"
readonly EXPECTED_CPU_SHA256="0f5ede8ff76d4e3a7257696d353d31cc15be2e8b308711770088207527aa25ac"
readonly EXPECTED_CPU_IO_SHA256="f59c2cfe4e20e404d4fdf6a04c779ee05b2db9b227faa9a34ca917c0784a2abc"
readonly EXPECTED_HOST_SHA256="682028fbf86081ed359c3acf78eab7191988e7c70da6e553bcda3bdafef67301"
readonly EXPECTED_BUS_SHA256="612cba33d56b6c3476d464f8b42d29e38a6c9f9703121e1e93bb4c98b34d37a7"
readonly EXPECTED_TRUE_DEPEX_SHA256="557c754d26e2667287367a856ea5fcd584f35ab796d24a6a875d1648a4637d23"
readonly EXPECTED_HOST_DEPEX_SHA256="097f82885f39ca3b1f95c92c8322a8df04785ab576522a75b9bddf5da68ad9ad"

if [[ $# -ne 2 ]]; then
  echo "usage: $0 /path/to/pinned-edk2 OUTPUT_DIR" >&2
  exit 64
fi
edk2_root="$(cd "$1" && pwd)"; output_dir="$2"
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
package_root="$repo_root/crates/bridgevm-hvf/firmware"
brotli_root="$edk2_root/BaseTools/Source/C/BrotliCompress/brotli"
mipi_root="$edk2_root/MdePkg/Library/MipiSysTLib/mipisyst"
tool_root="$edk2_root/BaseTools/Source/C/bin"
require_commit() {
  local label="$1" path="$2" expected="$3" actual
  [[ -d "$path/.git" || -f "$path/.git" ]] || {
    echo "missing pinned ${label} checkout: ${path}" >&2; exit 65;
  }
  actual="$(git -C "$path" rev-parse HEAD)"
  [[ "$actual" == "$expected" ]] || {
    echo "refusing ${label} commit ${actual}; expected ${expected}" >&2; exit 65;
  }
}
require_commit EDK2 "$edk2_root" "$EXPECTED_EDK2_COMMIT"
require_commit "BaseTools brotli" "$brotli_root" "$EXPECTED_BROTLI_COMMIT"
require_commit "MIPI Sys-T" "$mipi_root" "$EXPECTED_MIPI_COMMIT"
if ! git -C "$edk2_root" diff --quiet --ignore-submodules=none ||
   ! git -C "$edk2_root" diff --cached --quiet --ignore-submodules=none; then
  echo "refusing a dirty EDK2 checkout" >&2; exit 66
fi
gcc_version="$(/opt/homebrew/bin/aarch64-elf-gcc --version | head -1)"
ld_version="$(/opt/homebrew/bin/aarch64-elf-ld --version | head -1)"
if [[ "$gcc_version" != "$EXPECTED_GCC_VERSION" || "$ld_version" != "$EXPECTED_LD_VERSION" ]]; then
  echo "refusing unpinned firmware tools: gcc='${gcc_version}' ld='${ld_version}'" >&2; exit 67
fi
build_root="$(mktemp -d /tmp/bridgevm-pc-pci-dxe.XXXXXX)"
trap 'rm -rf "$build_root"' EXIT
ln -s "$package_root" "$build_root/packages"
if ! make -C "$edk2_root/BaseTools" -j8 >"$build_root/base-tools.log" 2>&1; then
  tail -200 "$build_root/base-tools.log" >&2; exit 68
fi
export WORKSPACE="$edk2_root" PACKAGES_PATH="$build_root/packages"
export GCC_AARCH64_PREFIX="/opt/homebrew/bin/aarch64-elf-"
export PYTHON_COMMAND="$(command -v python3)" SOURCE_DATE_EPOCH="$SOURCE_DATE_EPOCH_PIN"
cd "$edk2_root"
set +u
# shellcheck disable=SC1091
source ./edksetup.sh BaseTools >/dev/null
set -u
if ! build -a AARCH64 -t GCC -p BridgeVmPcPkg/BridgeVmPcPci.dsc -b RELEASE -n 8 \
     >"$build_root/build.log" 2>&1; then
  tail -200 "$build_root/build.log" >&2; exit 69
fi
built_root="$edk2_root/Build/BridgeVmPcPci/RELEASE_GCC/AARCH64"
names=(ArmCpuDxe CpuMmio2Dxe PciHostBridgeDxe PciBusDxe)
expected=("$EXPECTED_CPU_SHA256" "$EXPECTED_CPU_IO_SHA256" "$EXPECTED_HOST_SHA256" "$EXPECTED_BUS_SHA256")
mkdir -p "$output_dir"
for index in 0 1 2 3; do
  built="$built_root/${names[$index]}.efi"; artifact="$output_dir/${names[$index]}.efi"
  [[ -f "$built" ]] || { echo "missing ${names[$index]} output" >&2; exit 69; }
  cp "$built" "$artifact"; "$tool_root/GenFw" -z -r "$artifact"
  /opt/homebrew/bin/aarch64-elf-objdump -f "$artifact" | grep -q 'file format pei-aarch64-little'
  actual="$(shasum -a 256 "$artifact" | awk '{print $1}')"
  [[ "$actual" == "${expected[$index]}" ]] || {
    echo "${names[$index]} digest ${actual} does not match ${expected[$index]}" >&2; exit 70;
  }
done
cpu_depex="$built_root/ArmPkg/Drivers/CpuDxe/CpuDxe/OUTPUT/ArmCpuDxe.depex"
io_depex="$built_root/UefiCpuPkg/CpuMmio2Dxe/CpuMmio2Dxe/OUTPUT/CpuMmio2Dxe.depex"
host_depex="$built_root/MdeModulePkg/Bus/Pci/PciHostBridgeDxe/PciHostBridgeDxe/OUTPUT/PciHostBridgeDxe.depex"
cp "$cpu_depex" "$output_dir/ArmCpuDxe.depex"
cp "$io_depex" "$output_dir/CpuMmio2Dxe.depex"
cp "$host_depex" "$output_dir/PciHostBridgeDxe.depex"
[[ "$(shasum -a 256 "$cpu_depex" | awk '{print $1}')" == "$EXPECTED_TRUE_DEPEX_SHA256" ]]
[[ "$(shasum -a 256 "$io_depex" | awk '{print $1}')" == "$EXPECTED_TRUE_DEPEX_SHA256" ]]
[[ "$(shasum -a 256 "$host_depex" | awk '{print $1}')" == "$EXPECTED_HOST_DEPEX_SHA256" ]]
if strings -a "$output_dir"/*.efi | grep -i -E 'qemu|armvirt|ovmf|fw[_-]?cfg|u[t]m'; then
  echo "PCI DXE output contains a prohibited compatibility-platform reference" >&2; exit 71
fi
config_sha256="$(shasum -a 256 "$package_root/BridgeVmPcPkg/BridgeVmPcPci.dsc" | awk '{print $1}')"
host_source_sha256="$(shasum -a 256 "$package_root/BridgeVmPcPkg/Library/PciHostBridgeLib/PciHostBridgeLib.c" | awk '{print $1}')"
receipt="$output_dir/BridgeVmPcPciDxe.build.json"
printf '%s\n' '{' '  "schemaVersion": 1,' \
  '  "artifactKind": "development-only-standard-uefi-pci-stack",' \
  "  \"edk2Commit\": \"${EXPECTED_EDK2_COMMIT}\"," \
  "  \"sourceDateEpoch\": ${SOURCE_DATE_EPOCH_PIN}," \
  "  \"gccVersion\": \"${gcc_version}\"," \
  "  \"ldVersion\": \"${ld_version}\"," \
  "  \"platformConfigSha256\": \"${config_sha256}\"," \
  "  \"hostBridgeSourceSha256\": \"${host_source_sha256}\"," \
  "  \"armCpuDxeSha256\": \"${EXPECTED_CPU_SHA256}\"," \
  "  \"cpuMmio2DxeSha256\": \"${EXPECTED_CPU_IO_SHA256}\"," \
  "  \"pciHostBridgeDxeSha256\": \"${EXPECTED_HOST_SHA256}\"," \
  "  \"pciBusDxeSha256\": \"${EXPECTED_BUS_SHA256}\"," \
  '  "claimBoundary": "module build only; standard PCI enumeration, BAR operation, DMA, interrupts, boot manager, and Windows boot require separate live evidence"' \
  '}' >"$receipt"
echo "built standard UEFI PCI stack in $output_dir"
echo "receipt $receipt"
