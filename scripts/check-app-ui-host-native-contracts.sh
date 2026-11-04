#!/usr/bin/env bash
# macOS half of the normal-app UI host contracts: compiles the owned host,
# overview fixture, launcher and driver contracts and runs them without
# NSApplication, an app launch or an accessibility permission query.
set -euo pipefail
if [[ $# != 2 || $1 != --output || $2 != /* ]]; then
  echo 'usage: check-app-ui-host-native-contracts.sh --output ABSOLUTE_NEW_DIRECTORY' >&2
  exit 2
fi
[[ ! -e "$2" && ! -L "$2" && -d "$(dirname "$2")" ]] || { echo 'output must be new and its parent must exist' >&2; exit 2; }
mkdir -m 700 "$2"
out=$(cd "$2" && pwd -P)
cd "$(dirname "${BASH_SOURCE[0]}")/.."

echo '--- owned host and overview fixture contracts'
bash scripts/check-app-ui-host-contracts.sh --scratch-path "$out/swift"

echo '--- the diagnostic host is excluded from release builds'
host_bin_dir=$(swift build --package-path apps/macos --scratch-path "$out/swift" --show-bin-path -Xswiftc -DBRIDGEVM_APP_UI_HOST)
bash scripts/check-app-ui-host-build-boundary.sh --diagnostic-binary "$host_bin_dir/BridgeVMControl" --output "$out/build-boundary"

echo '--- launcher and its pure ownership contracts'
mkdir "$out/launcher"
bash apps/macos/scripts/build-app-ui-launcher.sh --output "$out/launcher/AppUIHostLauncher"
xcrun swiftc -parse-as-library -swift-version 5 apps/macos/scripts/AppUIHostLaunchOwnership.swift \
  tests/integration/AppUIHostLateOwnershipContracts.swift tests/integration/app-ui-launcher-ownership.swift \
  -o "$out/launcher/ownership-tests"
"$out/launcher/ownership-tests"

echo '--- native UI driver contracts'
bash scripts/check-app-ui-driver-contracts.sh --output "$out/driver"
echo 'native UI host contracts: PASS'
