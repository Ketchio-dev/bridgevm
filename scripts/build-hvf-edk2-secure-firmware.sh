#!/usr/bin/env bash
set -euo pipefail

# Rebuild the exact ArmVirtQemu firmware BridgeVM ships. The source checkout is
# supplied by the caller: this script never contacts the network or silently
# advances the pinned source revision.

readonly EXPECTED_COMMIT="b03a21a63e3bd001f52c527e5a57feddb53a690b"
readonly SOURCE_DATE_EPOCH_PIN="1778208179"
readonly OUTPUT_NAME="edk2-aarch64-secure-code.fd"
readonly EXPECTED_GCC_VERSION="aarch64-elf-gcc (GCC) 16.1.0"
readonly EXPECTED_IASL_VERSION="20260408"
readonly EXPECTED_SHA256="241ce0117723bf30008e83ce1329528c3486398ce6256d943cd1300defffd8ff"
# Module debug entries embed the build path, and EDK2 draws a random stack
# cookie per build, so both are pinned for the digest to reproduce. The cookie
# was drawn once from a CSPRNG; it is readable in the published binary anyway,
# as with any prebuilt EDK2 firmware.
readonly BUILD_WORKSPACE="/tmp/bridgevm-edk2-stable202605"
readonly STACK_COOKIE_VALUE64="0xebf683e4b7f38051"

usage() {
  echo "usage: $0 /path/to/edk2 [output-directory]" >&2
  echo "expected edk2-stable202605 commit: ${EXPECTED_COMMIT}" >&2
}

if [[ $# -lt 1 || $# -gt 2 ]]; then
  usage
  exit 64
fi

edk2_root="$(cd "$1" && pwd)"
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
output_dir="${2:-${repo_root}/crates/bridgevm-hvf/firmware}"
patch_dir="$repo_root/crates/bridgevm-hvf/firmware/patches"
ppi_patch="$patch_dir/0001-armvirt-process-tpm-ppi.patch"
ramfb_patch="$patch_dir/0002-ramfb-desktop-resolutions.patch"

actual_commit="$(git -C "$edk2_root" rev-parse HEAD)"
if [[ "$actual_commit" != "$EXPECTED_COMMIT" ]]; then
  echo "refusing unpinned EDK2 source: got ${actual_commit}, expected ${EXPECTED_COMMIT}" >&2
  exit 65
fi

if ! git -C "$edk2_root" diff --quiet || ! git -C "$edk2_root" diff --cached --quiet; then
  echo "refusing a dirty EDK2 source tree" >&2
  exit 66
fi

if git -C "$edk2_root" submodule status --recursive | grep -Eq '^[+-U]' ; then
  echo "EDK2 submodules are incomplete or do not match the pinned commits" >&2
  exit 66
fi

for patch in "$ppi_patch" "$ramfb_patch"; do
  if [[ ! -f "$patch" ]]; then
    echo "required firmware patch is missing: ${patch}" >&2
    exit 67
  fi
  if ! git -C "$edk2_root" apply --check --ignore-space-change "$patch"; then
    echo "firmware patch does not apply cleanly to the pinned source: ${patch}" >&2
    exit 67
  fi
done

if [[ -e "$BUILD_WORKSPACE" || -L "$BUILD_WORKSPACE" ]]; then
  if [[ ! -L "$BUILD_WORKSPACE" || "$(cd "$BUILD_WORKSPACE" && pwd -P)" != "$(cd "$edk2_root" && pwd -P)" ]]; then
    echo "refusing: ${BUILD_WORKSPACE} exists and is not a link to ${edk2_root}" >&2
    exit 68
  fi
else
  ln -s "$edk2_root" "$BUILD_WORKSPACE"
fi

patch_active=0
cleanup_source_patch() {
  if [[ "$patch_active" == "1" ]]; then
    git -C "$edk2_root" apply --reverse --ignore-space-change "$ramfb_patch" "$ppi_patch" || {
      echo "failed to restore the EDK2 source tree after the build" >&2
      return 1
    }
    # The pinned EDK2 checkout stores these sources as CRLF. `git apply` can
    # match them with --ignore-space-change but leaves reversed context lines
    # as LF, so restore the known-clean patch targets byte-for-byte.
    git -C "$edk2_root" restore --source=HEAD -- \
      OvmfPkg/Library/Tcg2PhysicalPresenceLibQemu/DxeTcg2PhysicalPresenceLib.c \
      OvmfPkg/Library/PlatformBootManagerLibLight/PlatformBm.c \
      OvmfPkg/Library/PlatformBootManagerLibLight/PlatformBootManagerLib.inf \
      OvmfPkg/QemuRamfbDxe/QemuRamfb.c
    patch_active=0
  fi
}
trap cleanup_source_patch EXIT
git -C "$edk2_root" apply --ignore-space-change "$ppi_patch" "$ramfb_patch"
patch_active=1

readonly TPM_PPI_LIBRARY_BINDING='Tcg2PhysicalPresenceLib|OvmfPkg/Library/Tcg2PhysicalPresenceLibQemu/DxeTcg2PhysicalPresenceLib.inf'
if ! grep -Fq "$TPM_PPI_LIBRARY_BINDING" "$edk2_root/ArmVirtPkg/ArmVirtQemu.dsc"; then
  echo "firmware source does not bind ArmVirtQemu to the QEMU TPM PPI request processor" >&2
  exit 67
fi
if ! grep -Fq 'Tcg2PhysicalPresenceLibProcessRequest (NULL);' \
  "$edk2_root/OvmfPkg/Library/PlatformBootManagerLibLight/PlatformBm.c"; then
  echo "firmware source does not process TPM PPI requests from ArmVirt's boot manager" >&2
  exit 67
fi
if ! grep -Fq 'QemuTpmPpiRead32' \
  "$edk2_root/OvmfPkg/Library/Tcg2PhysicalPresenceLibQemu/DxeTcg2PhysicalPresenceLib.c"; then
  echo "firmware source does not use AArch64-safe TPM PPI field accessors" >&2
  exit 67
fi
if ! grep -Eq '^ +2560, // HorizontalResolution' "$edk2_root/OvmfPkg/QemuRamfbDxe/QemuRamfb.c"; then
  echo "firmware source does not offer the desktop ramfb resolutions" >&2
  exit 67
fi

for tool in /opt/homebrew/bin/aarch64-elf-gcc /opt/homebrew/bin/iasl shasum; do
  if [[ ! -x "$tool" ]] && ! command -v "$tool" >/dev/null 2>&1; then
    echo "required tool missing: ${tool}" >&2
    exit 69
  fi
done

gcc_version="$(/opt/homebrew/bin/aarch64-elf-gcc --version | head -n 1)"
iasl_version="$(/opt/homebrew/bin/iasl -v | awk '/version/{print $NF; exit}')"
if [[ "$gcc_version" != "$EXPECTED_GCC_VERSION" || "$iasl_version" != "$EXPECTED_IASL_VERSION" ]]; then
  echo "refusing an unpinned firmware toolchain: gcc='${gcc_version}' iasl='${iasl_version}'" >&2
  exit 69
fi

export GCC_AARCH64_PREFIX="/opt/homebrew/bin/aarch64-elf-"
export SOURCE_DATE_EPOCH="$SOURCE_DATE_EPOCH_PIN"
export PYTHON_COMMAND="$(command -v python3)"

make -C "$BUILD_WORKSPACE/BaseTools"
cd "$BUILD_WORKSPACE"
# A clean platform build: stale makefiles would keep another workspace path.
rm -rf Build/ArmVirtQemu-AArch64
build_dir="Build/ArmVirtQemu-AArch64/RELEASE_GCC"
mkdir -p "$build_dir"
python3 -c 'import json, sys; json.dump([int(sys.argv[1], 16)] * 100, open(sys.argv[2], "w"))' \
  "$STACK_COOKIE_VALUE64" "$build_dir/StackCookieValues64.json"
# shellcheck disable=SC1091
set +u
source ./edksetup.sh BaseTools
set -u
build -a AARCH64 -t GCC -p ArmVirtPkg/ArmVirtQemu.dsc -b RELEASE \
  -D SECURE_BOOT_ENABLE=TRUE \
  -D TPM2_ENABLE=TRUE \
  -D TPM2_CONFIG_ENABLE=TRUE

firmware="$BUILD_WORKSPACE/$build_dir/FV/QEMU_EFI.fd"
xref="$BUILD_WORKSPACE/$build_dir/FV/Guid.xref"
if [[ ! -f "$firmware" || ! -f "$xref" ]]; then
  echo "expected EDK2 build outputs are missing" >&2
  exit 70
fi

for module in SecurityStubDxe SecureBootConfigDxe EnrollDefaultKeys Tcg2Dxe; do
  if ! grep -q "$module" "$xref"; then
    echo "firmware verification failed: ${module} absent from Guid.xref" >&2
    exit 71
  fi
done

firmware_size="$(stat -f '%z' "$firmware")"
if [[ "$firmware_size" != "3145728" ]]; then
  echo "firmware verification failed: unexpected size ${firmware_size}" >&2
  exit 72
fi

built_sha256="$(shasum -a 256 "$firmware" | awk '{print $1}')"
if [[ "$built_sha256" != "$EXPECTED_SHA256" ]]; then
  echo "firmware verification failed: digest ${built_sha256} does not match ${EXPECTED_SHA256}" >&2
  exit 73
fi

ppi_patch_sha256="$(shasum -a 256 "$ppi_patch" | awk '{print $1}')"
ramfb_patch_sha256="$(shasum -a 256 "$ramfb_patch" | awk '{print $1}')"
cleanup_source_patch
trap - EXIT
if ! git -C "$edk2_root" diff --quiet || ! git -C "$edk2_root" diff --cached --quiet; then
  echo "EDK2 source tree was not restored after the patched build" >&2
  exit 74
fi

mkdir -p "$output_dir"
install -m 0644 "$firmware" "$output_dir/$OUTPUT_NAME"
firmware_sha256="$(shasum -a 256 "$output_dir/$OUTPUT_NAME" | awk '{print $1}')"

receipt="$output_dir/${OUTPUT_NAME}.build.json"
printf '%s\n' \
  '{' \
  '  "schemaVersion": 1,' \
  '  "project": "tianocore/edk2",' \
  '  "tag": "edk2-stable202605",' \
  "  \"commit\": \"${EXPECTED_COMMIT}\"," \
  "  \"sourceDateEpoch\": ${SOURCE_DATE_EPOCH_PIN}," \
  "  \"buildWorkspace\": \"${BUILD_WORKSPACE}\"," \
  "  \"stackCookieValue64\": \"${STACK_COOKIE_VALUE64}\"," \
  '  "platform": "ArmVirtPkg/ArmVirtQemu.dsc",' \
  '  "architecture": "AARCH64",' \
  '  "target": "RELEASE",' \
  '  "toolchain": "GCC",' \
  '  "defines": ["SECURE_BOOT_ENABLE=TRUE", "TPM2_ENABLE=TRUE", "TPM2_CONFIG_ENABLE=TRUE"],' \
  "  \"gccVersion\": \"${gcc_version}\"," \
  "  \"iaslVersion\": \"${iasl_version}\"," \
  "  \"size\": ${firmware_size}," \
  "  \"sha256\": \"${firmware_sha256}\"," \
  '  "verifiedModules": ["SecurityStubDxe", "SecureBootConfigDxe", "EnrollDefaultKeys", "Tcg2Dxe"],' \
  '  "verifiedLibraryInstances": ["Tcg2PhysicalPresenceLibQemu"],' \
  "  \"patches\": [{\"path\": \"crates/bridgevm-hvf/firmware/patches/0001-armvirt-process-tpm-ppi.patch\", \"sha256\": \"${ppi_patch_sha256}\"}, {\"path\": \"crates/bridgevm-hvf/firmware/patches/0002-ramfb-desktop-resolutions.patch\", \"sha256\": \"${ramfb_patch_sha256}\"}]" \
  '}' > "$receipt"

echo "installed $output_dir/$OUTPUT_NAME"
echo "sha256 $firmware_sha256"
echo "receipt $receipt"
