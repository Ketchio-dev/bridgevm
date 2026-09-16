#!/bin/bash
# Compile only; actual LS/AX use belongs exclusively to the physical app-only tier.
set -euo pipefail
umask 077
if [[ $# != 2 || $1 != --output || $2 != /* ]]; then
  echo 'usage: build-app-ui-driver.sh --output ABSOLUTE_NEW_FILE' >&2
  exit 2
fi
output=$2
[[ ! -e "$output" && ! -L "$output" && -d "$(dirname "$output")" ]] || exit 2
script_dir=$(cd "$(dirname "$0")" && pwd -P)
core_dir="$script_dir/../Sources/BridgeVMControl/AppUIHostDriverProtocol"
scratch=$(mktemp -d "$(dirname "$output")/.app-ui-driver-build.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
xcrun swiftc -parse-as-library -swift-version 5 -O -D BRIDGEVM_APP_UI_DRIVER \
  -target arm64-apple-macosx14.0 -module-cache-path "$scratch/module-cache" \
  -framework AppKit -framework ApplicationServices -framework CryptoKit -framework Security \
  "$core_dir"/*.swift "$script_dir"/AppUIDriver*.swift "$script_dir"/AppUIHostV2*.swift \
  "$script_dir/AppUIHostLaunchOwnership.swift" "$script_dir/LaunchAppUIHost.swift" \
  "$script_dir/AppUIHostLauncherMain.swift" -o "$scratch/AppUIHostLauncher"
chmod 0500 "$scratch/AppUIHostLauncher"
ln "$scratch/AppUIHostLauncher" "$output"
