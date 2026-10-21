#!/usr/bin/env bash
# Contract for scripts/swift-testing-toolchain.sh: swift-testing paths come from
# the compiler's own developer directory, never a fixed CommandLineTools path,
# and an incomplete or unknown toolchain fails closed.
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
source "$ROOT/scripts/swift-testing-toolchain.sh"
WORK=$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-swift-testing.XXXXXX")
WORK=$(cd "$WORK" && pwd -P)
trap 'rm -rf "$WORK"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }
expect() { [[ "$2" == "$3" ]] || fail "$1: got '$2', want '$3'"; }
refuses() { if swift_testing_toolchain "$2" 2>/dev/null; then fail "$1 was accepted"; fi; }

X="$WORK/Xcode.app/Contents/Developer"
T="$X/Toolchains/XcodeDefault.xctoolchain/usr"
P="$X/Platforms/MacOSX.platform/Developer"
C="$WORK/CommandLineTools"
mkdir -p "$T/bin" "$T/lib/swift/host/plugins/testing" "$P/Library/Frameworks/Testing.framework" \
    "$C/usr/bin" "$C/usr/lib/swift/host/plugins/testing" "$C/Library/Developer/Frameworks/Testing.framework" \
    "$WORK/opt/bin"
touch "$T/bin/swiftc" "$T/lib/swift/host/plugins/testing/libTestingMacros.dylib" \
    "$C/usr/bin/swiftc" "$C/usr/lib/swift/host/plugins/testing/libTestingMacros.dylib" "$WORK/opt/bin/swiftc"

swift_testing_toolchain "$T/bin/swiftc"
expect "Xcode plugin" "$SWIFT_TESTING_MACROS" "$T/lib/swift/host/plugins/testing/libTestingMacros.dylib"
expect "Xcode frameworks" "$SWIFT_TESTING_FRAMEWORKS" "$P/Library/Frameworks"
expect "Xcode interop" "$SWIFT_TESTING_INTEROP_LIBS" "$P/usr/lib"
expect "Xcode runtime" "$SWIFT_TESTING_RUNTIME_LIBS" "$T/lib/swift/macosx"

swift_testing_toolchain "$C/usr/bin/swiftc"
expect "CLT plugin" "$SWIFT_TESTING_MACROS" "$C/usr/lib/swift/host/plugins/testing/libTestingMacros.dylib"
expect "CLT frameworks" "$SWIFT_TESTING_FRAMEWORKS" "$C/Library/Developer/Frameworks"
expect "CLT interop" "$SWIFT_TESTING_INTEROP_LIBS" "$C/Library/Developer/usr/lib"

refuses "unknown compiler layout" "$WORK/opt/bin/swiftc"
rm "$T/lib/swift/host/plugins/testing/libTestingMacros.dylib"
refuses "Xcode compiler without its own plugin" "$T/bin/swiftc"
rmdir "$C/Library/Developer/Frameworks/Testing.framework"
refuses "CLT compiler without its own Testing.framework" "$C/usr/bin/swiftc"

# The real active compiler resolves inside its own toolchain.
if [[ "$(uname -s)" == Darwin ]] && command -v swiftc >/dev/null 2>&1; then
    swift_testing_toolchain
    compiler=$(command -v swiftc)
    [[ "$compiler" != /usr/bin/swiftc ]] || compiler=$(xcrun --find swiftc)
    usr="$(cd "$(dirname "$compiler")/.." && pwd -P)"
    expect "active plugin" "$SWIFT_TESTING_MACROS" "$usr/lib/swift/host/plugins/testing/libTestingMacros.dylib"
fi
echo "PASS: swift-testing toolchain resolution"
