#!/usr/bin/env bash
# Runs all 651 XCTest functions under the shim, per suite.
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

# suite <name> <sources-dir> <tests-dir> [extra swiftc args...]
# -D DEBUG matches `swift test`, which builds debug: the sources gate their
# env-var overrides on DEBUG and the tests exercise those overrides.
suite() {
    local name="$1" sources="$2" tests="$3"; shift 3
    echo "== $name =="
    # macOS bash 3.2 has no globstar; find(1) does the recursive walk.
    local lib_sources=()
    while IFS= read -r file; do lib_sources+=("$file"); done \
        < <(find "$ROOT/$sources" -name '*.swift' | sort)
    local bundle_shim=()
    if grep -rq 'Bundle\.module' "$ROOT/$sources"; then
        bundle_shim=("$WORK/bundle-module.swift")
    fi
    swiftc -D DEBUG -emit-module -emit-library -static -module-name "$name" \
        -target "$TARGET" -enable-testing -o "$WORK/lib$name.a" \
        "${lib_sources[@]}" ${bundle_shim[@]+"${bundle_shim[@]}"} "$@"
    mkdir -p "$WORK/$name"
    python3 "$ROOT/scripts/generate-xctest-manifest.py" \
        "$ROOT/$tests" "$WORK/$name/main.swift"
    swiftc -D DEBUG -target "$TARGET" -I "$WORK" -L "$WORK" -lXCTest "-l$name" \
        -o "$WORK/run-$name" "$ROOT/$tests"/*.swift "$WORK/$name/main.swift" "$@"
    BV_SHIM_RESOURCES="$ROOT/apps/macos/Sources/BridgeVMControl/Resources" \
        "$WORK/run-$name"
}

suite BridgeVMApp apps/macos/Sources/BridgeVMApp apps/macos/Tests/BridgeVMAppTests
suite BridgeVMControl apps/macos/Sources/BridgeVMControl apps/macos/Tests/BridgeVMControlTests
suite AppleVzRunnerCore apps/macos/Sources/AppleVzRunnerCore apps/macos/Tests/AppleVzRunnerTests \
    -framework Virtualization

echo "PASS: all three shim suites (651 test functions)"
