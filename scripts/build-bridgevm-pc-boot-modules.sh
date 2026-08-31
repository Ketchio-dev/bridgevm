#!/usr/bin/env bash
# Build the pinned generic UEFI boot stack and BridgeVM BDS/application.
set -euo pipefail
readonly EDK2_COMMIT="b03a21a63e3bd001f52c527e5a57feddb53a690b"
readonly BROTLI_COMMIT="e230f474b87134e8c6c85b630084c612057f253e"
readonly MIPI_COMMIT="370b5944c046bab043dd8b133727b2135af7747a"
readonly SOURCE_EPOCH="1778208179"
readonly GCC_VERSION="aarch64-elf-gcc (GCC) 16.1.0"
readonly MODULES=(ArmGicV3Dxe ArmTimerDxe BridgeVmPcBootManagerDxe CapsuleRuntimeDxe DiskIoDxe Fat Metronome MonotonicCounterRuntimeDxe PartitionDxe RealTimeClock ResetSystemRuntimeDxe SecurityStubDxe WatchdogTimer EnglishDxe BridgeVmPcGraphicsOutputDxe BridgeVmPcExitBootServicesProbe)
readonly HASHES=(40862435b086a16becf606a01a4863f90403f6f12d46419906108b35796ab7af 047555aed42514cd3141887f6ef5b076468687fa6a278a1dde44fe6934604c0c fbb69c4b0ccdfe1f03df0ab10a45337d52ece309b5bef55fb192655de10e158c 00ee840a2087b7016dfc6d138bfaaf1e73028ed119be611b6b6305d4afdc5bb4 0032010bdd3844d27ae27f582e784309fd9917f4fc65cfd36b6cbc6903bd0222 910cec7c14fe3fe400657ff2659d5731361b281356d2b333e1f3385a659ce753 42e9dbc6ec2af5e96267f9d3a951d67e2c8668e55dfa69b4c23aa5af3100ce32 02544678cf971c1874e6bc44deadf1626079d493d82b8157510ad50d63d2d076 139543729ddc5b2335339b799aed8d05a02efafae21b84e5477313a230a3ad01 445b2afb4cdf632bc7a9cb1291df1d2a747837c0b70901e9a33eb79be996378f ea0d7c68b11079dcc1e715318498ecc8454a71abbd98443c4d307acb3564d072 e7ae45b3b59fddbaa38720f0f36dcf3f22d3f2b8649d86a8b6c2a18c5726b82c 312ba55df8f949264dd89c45d6c5e1aa4dc02b04a6bb00eb6728135b2a3d68d8 fe7427e41de3b7d09ea9df818794d87b0ebbf2d8c761591569f488e2e52c1edb e5227474efe839185d634cf4c90ba4e6891af5774a4b85d46af3feeeff996554 93f86906c18acdc8be76466ad5ff63f50358c52a68583cea703c1b97696fff85)
if [[ $# -ne 2 ]]; then
  echo "usage: $0 /path/to/pinned-edk2 OUTPUT_DIR" >&2
  exit 64
fi
edk2="$(cd "$1" && pwd)"; output="$2"; repo="$(cd "$(dirname "$0")/.." && pwd)"
package="$repo/crates/bridgevm-hvf/firmware"; tools="$edk2/BaseTools/Source/C/bin"
require_commit() {
  local name="$1" path="$2" expected="$3"
  [[ "$(git -C "$path" rev-parse HEAD)" == "$expected" ]] || {
    echo "refusing unpinned ${name} source" >&2; exit 65;
  }
}
require_commit EDK2 "$edk2" "$EDK2_COMMIT"
require_commit brotli "$edk2/BaseTools/Source/C/BrotliCompress/brotli" "$BROTLI_COMMIT"
require_commit mipi "$edk2/MdePkg/Library/MipiSysTLib/mipisyst" "$MIPI_COMMIT"
git -C "$edk2" diff --quiet --ignore-submodules=none &&
  git -C "$edk2" diff --cached --quiet --ignore-submodules=none || {
    echo "refusing a dirty EDK2 checkout" >&2; exit 66;
  }
gcc_version="$(/opt/homebrew/bin/aarch64-elf-gcc --version | head -1)"
[[ "$gcc_version" == "$GCC_VERSION" ]] || {
  echo "refusing firmware compiler ${gcc_version}" >&2; exit 67;
}
"$repo/scripts/check-bridgevm-pc-prohibited-references.sh" tree BridgeVmPcPkg \
  "$package/BridgeVmPcPkg"
work="$(mktemp -d /tmp/bridgevm-pc-boot-modules.XXXXXX)"; trap 'rm -rf "$work"' EXIT
ln -s "$package" "$work/packages"
make -C "$edk2/BaseTools" -j8 >"$work/base-tools.log" 2>&1 || {
  tail -200 "$work/base-tools.log" >&2; exit 68;
}
export WORKSPACE="$edk2" PACKAGES_PATH="$work/packages"
export GCC_AARCH64_PREFIX="/opt/homebrew/bin/aarch64-elf-"
export PYTHON_COMMAND="$(command -v python3)" SOURCE_DATE_EPOCH="$SOURCE_EPOCH"
cd "$edk2"; set +u
# shellcheck disable=SC1091
source ./edksetup.sh BaseTools >/dev/null
set -u
build -a AARCH64 -t GCC -p BridgeVmPcPkg/BridgeVmPcBoot.dsc -b RELEASE -n 8 \
  >"$work/build.log" 2>&1 || { tail -240 "$work/build.log" >&2; exit 69; }
built="$edk2/Build/BridgeVmPcBoot/RELEASE_GCC/AARCH64"; mkdir -p "$output"
for index in "${!MODULES[@]}"; do
  name="${MODULES[$index]}"; artifact="$output/$name.efi"
  cp "$built/$name.efi" "$artifact"; "$tools/GenFw" -z -r "$artifact"
  /opt/homebrew/bin/aarch64-elf-objdump -f "$artifact" | grep -q 'pei-aarch64-little'
  actual="$(shasum -a 256 "$artifact" | awk '{print $1}')"
  [[ "$actual" == "${HASHES[$index]}" ]] || {
    echo "$name digest $actual does not match ${HASHES[$index]}" >&2; exit 70;
  }
done
declare -a depex=(
  "ArmGicV3Dxe:ArmPkg/Drivers/ArmGicDxe/ArmGicV3Dxe/OUTPUT/ArmGicV3Dxe.depex"
  "ArmTimerDxe:ArmPkg/Drivers/TimerDxe/TimerDxe/OUTPUT/ArmTimerDxe.depex"
  "BridgeVmPcBootManagerDxe:BridgeVmPcPkg/Drivers/BootManagerDxe/BootManagerDxe/OUTPUT/BridgeVmPcBootManagerDxe.depex"
  "BridgeVmPcGraphicsOutputDxe:BridgeVmPcPkg/Drivers/GraphicsOutputDxe/GraphicsOutputDxe/OUTPUT/BridgeVmPcGraphicsOutputDxe.depex"
  "CapsuleRuntimeDxe:MdeModulePkg/Universal/CapsuleRuntimeDxe/CapsuleRuntimeDxe/OUTPUT/CapsuleRuntimeDxe.depex"
  "Metronome:MdeModulePkg/Universal/Metronome/Metronome/OUTPUT/Metronome.depex"
  "MonotonicCounterRuntimeDxe:MdeModulePkg/Universal/MonotonicCounterRuntimeDxe/MonotonicCounterRuntimeDxe/OUTPUT/MonotonicCounterRuntimeDxe.depex"
  "RealTimeClock:EmbeddedPkg/RealTimeClockRuntimeDxe/RealTimeClockRuntimeDxe/OUTPUT/RealTimeClock.depex"
  "ResetSystemRuntimeDxe:MdeModulePkg/Universal/ResetSystemRuntimeDxe/ResetSystemRuntimeDxe/OUTPUT/ResetSystemRuntimeDxe.depex"
  "SecurityStubDxe:MdeModulePkg/Universal/SecurityStubDxe/SecurityStubDxe/OUTPUT/SecurityStubDxe.depex"
  "WatchdogTimer:MdeModulePkg/Universal/WatchdogTimerDxe/WatchdogTimer/OUTPUT/WatchdogTimer.depex"
)
for item in "${depex[@]}"; do
  cp "$built/${item#*:}" "$output/${item%%:*}.depex"
done
if strings -a "$output"/*.efi | grep -i -E 'qemu|armvirt|ovmf|fw[_-]?cfg|u[t]m'; then
  echo "boot output contains a prohibited compatibility-platform reference" >&2; exit 71
fi
tree_hash="$(python3 - "$package/BridgeVmPcPkg" <<'PY'
import hashlib, pathlib, sys
root=pathlib.Path(sys.argv[1]); digest=hashlib.sha256()
for path in sorted(p for p in root.rglob('*') if p.is_file()):
    relative=path.relative_to(root).as_posix().encode(); data=path.read_bytes()
    digest.update(len(relative).to_bytes(4,'big')); digest.update(relative)
    digest.update(len(data).to_bytes(8,'big')); digest.update(data)
print(digest.hexdigest())
PY
)"
cat >"$output/BridgeVmPcBootModules.build.json" <<EOF
{
  "schemaVersion": 1,
  "artifactKind": "development-only-standard-uefi-boot-stack",
  "edk2Commit": "$EDK2_COMMIT",
  "sourceDateEpoch": $SOURCE_EPOCH,
  "gccVersion": "$gcc_version",
  "packageTreeSha256": "$tree_hash",
  "bootManagerSha256": "${HASHES[2]}",
  "graphicsOutputSha256": "${HASHES[14]}",
  "exitBootServicesApplicationSha256": "${HASHES[15]}",
  "claimBoundary": "module build only; BDS dispatch, filesystem load, ExitBootServices, and Windows boot require live evidence"
}
EOF
echo "built BridgeVM PC boot modules in $output"
