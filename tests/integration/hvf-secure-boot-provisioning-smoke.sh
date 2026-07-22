#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FIRMWARE="$ROOT/crates/bridgevm-hvf/firmware/edk2-aarch64-secure-code.fd"
BUILD_RECEIPT="$FIRMWARE.build.json"
POLICY="$ROOT/apps/macos/Sources/BridgeVMControl/Resources/secureboot-microsoft-only-aarch64-v1.6.5.json"
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-secure-boot-smoke.XXXXXX")"
trap 'rm -rf "$WORKDIR"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

[[ -f "$FIRMWARE" && "$(stat -f '%z' "$FIRMWARE")" == "3145728" ]] \
  || fail "pinned Secure Boot + TPM2 firmware is missing or not 3 MiB"
[[ "$(shasum -a 256 "$FIRMWARE" | awk '{ print $1 }')" == \
  "f41c7eb7c1a9dabf8ed10c4e52642378e05df171eecd65ca15ed414d9fabdff9" ]] \
  || fail "pinned firmware digest mismatch"
[[ -s "$BUILD_RECEIPT" ]] || fail "firmware build receipt missing"
grep -Fq '"verifiedLibraryInstances": ["Tcg2PhysicalPresenceLibQemu"]' "$BUILD_RECEIPT" \
  || fail "firmware build receipt does not verify the QEMU TPM PPI request processor"
[[ -s "$POLICY" ]] || fail "Microsoft Secure Boot policy missing"

sdkroot="${BRIDGEVM_SWIFT_SDKROOT:-}"
if [[ -z "$sdkroot" && -d /Library/Developer/CommandLineTools/SDKs/MacOSX15.4.sdk ]]; then
  sdkroot=/Library/Developer/CommandLineTools/SDKs/MacOSX15.4.sdk
fi
if [[ -z "$sdkroot" ]]; then
  sdkroot="$(xcrun --sdk macosx --show-sdk-path)"
fi

CLANG_MODULE_CACHE_PATH="$WORKDIR/clang-module-cache" \
  SWIFTPM_MODULECACHE_OVERRIDE="$WORKDIR/swift-module-cache" \
  SDKROOT="$sdkroot" \
  swiftc -parse-as-library \
    "$ROOT/apps/macos/Sources/BridgeVMControl/HvfEngine/HvfWindowsBootSeed.swift" \
    "$ROOT/apps/macos/Sources/BridgeVMControl/HvfEngine/HvfSecureBootProvisioner.swift" \
    "$ROOT/tests/integration/HvfSecureBootProvisioningSmoke.swift" \
    -o "$WORKDIR/hvf-secure-boot-provisioning-smoke"

"$WORKDIR/hvf-secure-boot-provisioning-smoke" "$POLICY"
