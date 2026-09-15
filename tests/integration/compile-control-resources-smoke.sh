#!/bin/bash
set -euo pipefail
umask 077
if [[ $# != 2 || $1 != /* || $2 != /* || ! -f $1 || -L $1 || -e $2 || -L $2 ]]; then
  echo 'usage: compile-control-resources-smoke.sh ABSOLUTE_SWIFT_FIXTURE ABSOLUTE_NEW_EXECUTABLE' >&2
  exit 2
fi
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)
output=$2
[[ -d "$(dirname "$output")" ]] || { echo 'output parent must exist' >&2; exit 2; }
scratch=$(mktemp -d "$(dirname "$output")/.resource-smoke-compile.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
sdkroot=${BRIDGEVM_SWIFT_SDKROOT:-$(xcrun --sdk macosx --show-sdk-path)}
# Shared real production inputs. No AppKit, SwiftUI, NSApplication or VM driver.
xcrun swiftc -parse-as-library -swift-version 5 -sdk "$sdkroot" \
  -module-cache-path "$scratch/module-cache" \
  "$root/apps/macos/Sources/BridgeVMControl/BridgeVMControlResources.swift" \
  "$root/apps/macos/Sources/BridgeVMControl/HvfEngine/HvfWindowsBootSeed.swift" \
  "$root/apps/macos/Sources/BridgeVMControl/HvfEngine/HvfSecureBootProvisioner.swift" \
  "$1" -o "$scratch/ResourceSmoke"
chmod 0500 "$scratch/ResourceSmoke"
ln "$scratch/ResourceSmoke" "$output"
