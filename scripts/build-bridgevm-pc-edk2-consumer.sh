#!/usr/bin/env bash
# Build the first BridgeVM Virtual ARM PC EDK2 module from an offline, pinned
# TianoCore checkout. This produces a DXE driver, not a bootable firmware FD.
set -euo pipefail

readonly EXPECTED_EDK2_COMMIT="b03a21a63e3bd001f52c527e5a57feddb53a690b"
readonly EXPECTED_BROTLI_COMMIT="e230f474b87134e8c6c85b630084c612057f253e"
readonly EXPECTED_MIPI_COMMIT="370b5944c046bab043dd8b133727b2135af7747a"
readonly SOURCE_DATE_EPOCH_PIN="1778208179"
readonly EXPECTED_GCC_VERSION="aarch64-elf-gcc (GCC) 16.1.0"
readonly EXPECTED_IASL_VERSION="20260408"
readonly EXPECTED_ARTIFACT_SHA256="01f555aec886cf241f277c53ac2bf57d38fd064a8ae4b6508f4f4897802efccc"

if [[ $# -ne 2 ]]; then
  echo "usage: $0 /path/to/pinned-edk2 OUTPUT_DIR" >&2
  exit 64
fi

edk2_root="$(cd "$1" && pwd)"
output_dir="$2"
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
package_root="$repo_root/crates/bridgevm-hvf/firmware/BridgeVmPcPkg"
brotli_root="$edk2_root/BaseTools/Source/C/BrotliCompress/brotli"
mipi_root="$edk2_root/MdePkg/Library/MipiSysTLib/mipisyst"

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
require_commit "BaseTools brotli" "$brotli_root" "$EXPECTED_BROTLI_COMMIT"
require_commit "MIPI Sys-T" "$mipi_root" "$EXPECTED_MIPI_COMMIT"
if ! git -C "$edk2_root" diff --quiet --ignore-submodules=none ||
   ! git -C "$edk2_root" diff --cached --quiet --ignore-submodules=none; then
  echo "refusing a dirty EDK2 checkout" >&2
  exit 66
fi

if rg -n -i 'qemu|armvirt|ovmf|fw[_-]?cfg|utm' "$package_root"; then
  echo "BridgeVmPcPkg contains a prohibited compatibility-platform dependency" >&2
  exit 67
fi
while IFS= read -r dependency; do
  case "$dependency" in
    MdePkg/MdePkg.dec|BridgeVmPcPkg/BridgeVmPcPkg.dec) ;;
    *) echo "unapproved EDK2 package dependency: ${dependency}" >&2; exit 67 ;;
  esac
done < <(awk '/^[[:space:]]+[A-Za-z0-9]+Pkg\/.*\.dec$/ {print $1}' \
  "$package_root/Drivers/PlatformTablesDxe/PlatformTablesDxe.inf")

gcc_version="$(/opt/homebrew/bin/aarch64-elf-gcc --version | head -1)"
iasl_version="$(/opt/homebrew/bin/iasl -v | awk '/version/{print $NF; exit}')"
if [[ "$gcc_version" != "$EXPECTED_GCC_VERSION" || "$iasl_version" != "$EXPECTED_IASL_VERSION" ]]; then
  echo "refusing unpinned firmware tools: gcc='${gcc_version}' iasl='${iasl_version}'" >&2
  exit 69
fi

package_tree_sha256="$(python3 - "$package_root" <<'PY'
import hashlib
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
digest = hashlib.sha256()
for path in sorted(item for item in root.rglob("*") if item.is_file()):
    relative = path.relative_to(root).as_posix().encode()
    digest.update(len(relative).to_bytes(4, "big"))
    digest.update(relative)
    data = path.read_bytes()
    digest.update(len(data).to_bytes(8, "big"))
    digest.update(data)
print(digest.hexdigest())
PY
)"

package_link_root="$(mktemp -d "/tmp/bridgevm-pc-edk2.XXXXXX")"
trap 'rm -rf "$package_link_root"' EXIT
ln -s "$repo_root/crates/bridgevm-hvf/firmware" "$package_link_root/packages"

base_tools_log="$package_link_root/base-tools.log"
if ! make -C "$edk2_root/BaseTools" -j8 >"$base_tools_log" 2>&1; then
  tail -200 "$base_tools_log" >&2
  exit 70
fi
export WORKSPACE="$edk2_root"
export PACKAGES_PATH="$package_link_root/packages"
export GCC_AARCH64_PREFIX="/opt/homebrew/bin/aarch64-elf-"
export PYTHON_COMMAND="$(command -v python3)"
export SOURCE_DATE_EPOCH="$SOURCE_DATE_EPOCH_PIN"
cd "$edk2_root"
set +u
# shellcheck disable=SC1091
source ./edksetup.sh BaseTools >/dev/null
set -u
build_log="$package_link_root/build.log"
if ! build -a AARCH64 -t GCC -p BridgeVmPcPkg/BridgeVmPcPkg.dsc \
     -b RELEASE -n 8 >"$build_log" 2>&1; then
  tail -200 "$build_log" >&2
  exit 70
fi

built="$edk2_root/Build/BridgeVmPc/RELEASE_GCC/AARCH64/BridgeVmPcPlatformTablesDxe.efi"
[[ -f "$built" ]] || { echo "expected DXE driver is missing: ${built}" >&2; exit 70; }
mkdir -p "$output_dir"
artifact="$output_dir/BridgeVmPcPlatformTablesDxe.efi"
cp "$built" "$artifact"
"$edk2_root/BaseTools/Source/C/bin/GenFw" -z -r "$artifact"

if ! /opt/homebrew/bin/aarch64-elf-objdump -f "$artifact" | grep -q 'file format pei-aarch64-little'; then
  echo "firmware output is not an AArch64 PE/COFF image" >&2
  exit 71
fi
if strings -a "$artifact" | rg -i 'qemu|armvirt|ovmf|fw[_-]?cfg|utm'; then
  echo "firmware output contains a prohibited compatibility-platform reference" >&2
  exit 71
fi

artifact_sha256="$(shasum -a 256 "$artifact" | awk '{print $1}')"
[[ "$artifact_sha256" == "$EXPECTED_ARTIFACT_SHA256" ]] || {
  echo "firmware digest ${artifact_sha256} does not match ${EXPECTED_ARTIFACT_SHA256}" >&2
  exit 72
}
artifact_size="$(stat -f '%z' "$artifact")"

receipt="$output_dir/BridgeVmPcPlatformTablesDxe.build.json"
printf '%s\n' \
  '{' \
  '  "schemaVersion": 1,' \
  '  "artifactKind": "development-only-uefi-dxe-driver",' \
  "  \"edk2Commit\": \"${EXPECTED_EDK2_COMMIT}\"," \
  "  \"brotliCommit\": \"${EXPECTED_BROTLI_COMMIT}\"," \
  "  \"mipiSysTCommit\": \"${EXPECTED_MIPI_COMMIT}\"," \
  "  \"sourceDateEpoch\": ${SOURCE_DATE_EPOCH_PIN}," \
  '  "platform": "BridgeVmPcPkg/BridgeVmPcPkg.dsc",' \
  '  "module": "BridgeVmPcPkg/Drivers/PlatformTablesDxe/PlatformTablesDxe.inf",' \
  '  "architecture": "AARCH64",' \
  '  "target": "RELEASE",' \
  "  \"gccVersion\": \"${gcc_version}\"," \
  "  \"iaslVersion\": \"${iasl_version}\"," \
  "  \"packageTreeSha256\": \"${package_tree_sha256}\"," \
  "  \"size\": ${artifact_size}," \
  "  \"sha256\": \"${artifact_sha256}\"," \
  '  "claimBoundary": "module build only; no reset-vector, UEFI boot, or Windows boot claim"' \
  '}' > "$receipt"

echo "built $artifact"
echo "sha256 $artifact_sha256"
echo "receipt $receipt"
