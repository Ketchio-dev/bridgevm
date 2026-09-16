#!/bin/bash
# Deterministic only: injected AX graphs, private files, signing and compiler checks.
set -euo pipefail
umask 077
if [[ $# != 2 || $1 != --output || $2 != /* ]]; then
  echo 'usage: check-app-ui-driver-contracts.sh --output ABSOLUTE_NEW_DIRECTORY' >&2
  exit 2
fi
[[ ! -e "$2" && ! -L "$2" && -d "$(dirname "$2")" ]] || exit 2
output="$(cd "$(dirname "$2")" && pwd -P)/$(basename "$2")"
mkdir -m 700 "$output"
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$repo_root"
core=apps/macos/Sources/BridgeVMControl/AppUIHostDriverProtocol
scripts=apps/macos/scripts
tests=tests/integration
compile() {
  local name=$1; shift
  xcrun swiftc -parse-as-library -swift-version 5 -D BRIDGEVM_APP_UI_DRIVER \
    -module-cache-path "$output/module-cache" "$core"/*.swift "$@" -o "$output/$name"
  "$output/$name"
}
compile protocol "$tests"/AppUIDriver*Contracts.swift "$tests/AppUIDriverProtocolTestSupport.swift" \
  "$tests/app-ui-driver-protocol.swift" 2>&1 | tee "$output/protocol.log"
compile ownership "$scripts/AppUIHostV2Ownership.swift" "$tests/app-ui-driver-ownership.swift" \
  2>&1 | tee "$output/ownership.log"
compile ax "$scripts/AppUIDriverAXTypes.swift" "$scripts/AppUIDriverAXGraph.swift" \
  "$scripts/AppUIDriverAXOperations.swift" "$tests/app-ui-driver-ax-fixture.swift" \
  "$tests/app-ui-driver-ax-contracts.swift" 2>&1 | tee "$output/ax.log"
bash "$scripts/build-app-ui-driver.sh" --output "$output/AppUIHostLauncher" \
  2>&1 | tee "$output/driver-build.log"
boundary=apps/macos/Sources/BridgeVMControl/AppUIHostDriverBuildBoundary.swift
if xcrun swiftc -typecheck -D BRIDGEVM_APP_UI_DRIVER "$boundary" > "$output/forbidden-driver.log" 2>&1; then
  echo 'app compile incorrectly accepted standalone driver flag' >&2
  exit 1
fi
grep -Fq 'error: BRIDGEVM_APP_UI_DRIVER is reserved for the standalone diagnostic launcher' "$output/forbidden-driver.log"
python3 "$tests/app-ui-driver-packaging-contracts.py" 2>&1 | tee "$output/packaging.log"
python3 "$tests/app-ui-host-v2-tier-smoke.py" 2>&1 | tee "$output/queue.log"
shasum -a 256 "$output/AppUIHostLauncher" > "$output/launcher.sha256"
echo 'native UI driver contracts: PASS (no application or accessibility permission query)'
