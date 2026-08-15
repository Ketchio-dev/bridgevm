#!/usr/bin/env bash
# Runs every XCTest function under the shim, per suite.
#
# Three per-suite runners rather than one merged binary: merging app and test
# sources into one module produced 58 redeclaration errors (fileprivate
# symbols coexist as separate files and collide once merged). Per suite, the
# app sources compile into a static library with -enable-testing and the test
# sources into an executable against it -- file scope stays intact.
#
# The result line is honest about provenance: measured under a shim, not
# Apple XCTest.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SHIM="$ROOT/apps/macos/XCTestShim"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
TARGET=arm64-apple-macosx14.0

echo "== shim =="
swiftc -emit-module -emit-library -static -module-name XCTest -target "$TARGET" \
    -o "$WORK/libXCTest.a" "$SHIM/XCTest.swift" "$SHIM/Runner.swift"

# SwiftPM synthesizes Bundle.module for targets with resources; a shell build
# has none, so tests read resources via BV_SHIM_RESOURCES instead.
cat > "$WORK/bundle-module.swift" <<'EOF'
import Foundation
extension Bundle {
    static let module: Bundle = {
        if let dir = ProcessInfo.processInfo.environment["BV_SHIM_RESOURCES"],
           let bundle = Bundle(path: dir) {
            return bundle
        }
        return Bundle.main
    }()
}
EOF


# shellcheck source=scripts/run-xctest-shim-suite.sh
source "$ROOT/scripts/run-xctest-shim-suite.sh"

# The suites share only the read-only shim library and each writes its own
# lib<name>.a, so they are independent: run concurrently they cost the longest
# rather than the sum, 59.1 s to 34.7 s. Logs are buffered per suite so the
# three do not interleave, and each exit status is checked on its own.
logged() { local n="$1"; shift; suite "$n" "$@" > "$WORK/log-$n" 2>&1; }

logged BridgeVMApp apps/macos/Sources/BridgeVMApp apps/macos/Tests/BridgeVMAppTests &
pids=($!)
logged BridgeVMControl apps/macos/Sources/BridgeVMControl apps/macos/Tests/BridgeVMControlTests &
pids+=($!)
logged AppleVzRunnerCore apps/macos/Sources/AppleVzRunnerCore apps/macos/Tests/AppleVzRunnerTests \
    -framework Virtualization &
pids+=($!)

failed=0
for pid in "${pids[@]}"; do wait "$pid" || failed=1; done
for name in BridgeVMApp BridgeVMControl AppleVzRunnerCore; do cat "$WORK/log-$name"; done
(( failed == 0 )) || { echo "FAIL: at least one shim suite failed" >&2; exit 1; }

echo "PASS: all three shim suites"
