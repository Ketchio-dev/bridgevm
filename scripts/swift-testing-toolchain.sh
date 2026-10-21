# Resolves swift-testing from the developer directory of the swiftc on PATH.
# Sourced by run-swift-tests.sh and run-xctest-shim-suites.sh.
#
# Testing.framework, its TestingMacros plugin and lib_TestingInterop must match
# the compiler. Hosted image macos-26-arm64/20261013.0424 pairs Xcode 26.6 with
# newer Command Line Tools; their plugin cannot load into Xcode's plugin server
# (missing SwiftDiagnostics symbol), so fixed CommandLineTools paths broke there.
#
# Sets SWIFT_TESTING_FRAMEWORKS, SWIFT_TESTING_MACROS, SWIFT_TESTING_INTEROP_LIBS
# and SWIFT_TESTING_RUNTIME_LIBS. Prints FAIL and returns 1 when unresolvable.
# shellcheck disable=SC2034 # the variables are the callers' outputs
swift_testing_toolchain() {
    local compiler="${1:-}" usr developer platform
    [[ -n "$compiler" ]] || compiler=$(command -v swiftc) || {
        echo "FAIL: swiftc not found on PATH" >&2; return 1; }
    # /usr/bin/swiftc is the xcrun shim; resolve the compiler it runs.
    if [[ "$compiler" == /usr/bin/swiftc ]]; then
        compiler=$(xcrun --find swiftc) || { echo "FAIL: xcrun cannot find swiftc" >&2; return 1; }
    fi
    compiler="$(cd "$(dirname "$compiler")" && pwd -P)/$(basename "$compiler")"
    usr=${compiler%/bin/swiftc}
    case "$usr" in
        */Toolchains/*.xctoolchain/usr)
            developer=${usr%/Toolchains/*}
            platform="$developer/Platforms/MacOSX.platform/Developer"
            SWIFT_TESTING_FRAMEWORKS="$platform/Library/Frameworks"
            SWIFT_TESTING_INTEROP_LIBS="$platform/usr/lib" ;;
        /*/usr)
            developer=${usr%/usr}
            SWIFT_TESTING_FRAMEWORKS="$developer/Library/Developer/Frameworks"
            SWIFT_TESTING_INTEROP_LIBS="$developer/Library/Developer/usr/lib" ;;
        *) echo "FAIL: unrecognised swiftc layout: $compiler" >&2; return 1 ;;
    esac
    SWIFT_TESTING_MACROS="$usr/lib/swift/host/plugins/testing/libTestingMacros.dylib"
    SWIFT_TESTING_RUNTIME_LIBS="$usr/lib/swift/macosx"
    [[ -d "$SWIFT_TESTING_FRAMEWORKS/Testing.framework" ]] || {
        echo "FAIL: Testing.framework not found at $SWIFT_TESTING_FRAMEWORKS for $compiler" >&2; return 1; }
    [[ -f "$SWIFT_TESTING_MACROS" ]] || {
        echo "FAIL: TestingMacros plugin not found at $SWIFT_TESTING_MACROS for $compiler" >&2; return 1; }
}
