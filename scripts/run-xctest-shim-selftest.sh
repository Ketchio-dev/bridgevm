#!/usr/bin/env bash
# Builds the XCTest shim and proves it can fail.
#
# The only thing that makes a shim usable is that assertions which should fail
# do fail. A shim that quietly passes everything is worse than no shim, because
# it turns unknown state into false confidence. This runs 29 checks: one
# falsification per assertion, plus a control that it does not fire spuriously.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SHIM="$ROOT/apps/macos/XCTestShim"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

TARGET=arm64-apple-macosx14.0
swiftc -emit-module -emit-library -static -module-name XCTest -target "$TARGET" \
    -o "$WORK/libXCTest.a" "$SHIM/XCTest.swift" "$SHIM/Runner.swift"
swiftc -I "$WORK" -L "$WORK" -lXCTest -target "$TARGET" \
    -o "$WORK/selftest" "$SHIM/SelfTest.swift"
"$WORK/selftest"
