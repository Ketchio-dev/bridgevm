#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FIRMWARE="$ROOT/crates/bridgevm-hvf/firmware/edk2-aarch64-secure-code.fd"
BUILD_RECEIPT="$FIRMWARE.build.json"
POLICY="$ROOT/apps/macos/Sources/BridgeVMControl/Resources/secureboot-microsoft-windows-transition-aarch64-v1.6.5.json"
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-secure-boot-smoke.XXXXXX")"
trap 'rm -rf "$WORKDIR"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

[[ -f "$FIRMWARE" && "$(stat -f '%z' "$FIRMWARE")" == "3145728" ]] \
  || fail "pinned Secure Boot + TPM2 firmware is missing or not 3 MiB"
[[ "$(shasum -a 256 "$FIRMWARE" | awk '{ print $1 }')" == \
  "241ce0117723bf30008e83ce1329528c3486398ce6256d943cd1300defffd8ff" ]] \
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

BRIDGEVM_SWIFT_SDKROOT="$sdkroot" \
  "$ROOT/tests/integration/compile-control-resources-smoke.sh" \
  "$ROOT/tests/integration/HvfSecureBootProvisioningSmoke.swift" "$WORKDIR/hvf-secure-boot-provisioning-smoke"

"$WORKDIR/hvf-secure-boot-provisioning-smoke" "$POLICY"
