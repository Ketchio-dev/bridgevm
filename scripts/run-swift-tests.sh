#!/usr/bin/env bash
# Runs the swift-testing suites in apps/macos/Tests/*SwiftTests.
#
# Why this is not `swift test`: XCTest ships with Xcode, which is not installed
# on this machine (`/usr/bin/xcodebuild` is a shim, and no XCTest.framework
# exists anywhere on disk). `swift test` builds *every* test target, so the
# three XCTest targets fail the build before any swift-testing target can run,
# and adding a swift-testing target to Package.swift does not help.
#
# swift-testing ships with both CommandLineTools and Xcode, so the new suites
# are compiled directly against the sources they cover and run as a plain
# binary. When Xcode is installed this script can be replaced by `swift test`.
set -euo pipefail

REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$REPO"

# Testing.framework, the TestingMacros plugin (without it #expect/#require fail
# to expand) and lib_TestingInterop come from the active swiftc's developer
# directory; mixing toolchains fails to load the plugin.
source "$REPO/scripts/swift-testing-toolchain.sh"
swift_testing_toolchain
FRAMEWORKS=$SWIFT_TESTING_FRAMEWORKS
SWIFT_LIBS=$SWIFT_TESTING_RUNTIME_LIBS
MACROS=$SWIFT_TESTING_MACROS
TESTING_LIBS=$SWIFT_TESTING_INTEROP_LIBS

OUT=$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-check.XXXXXX")
trap 'rm -rf "$OUT"' EXIT

# Each suite lists the sources under test on its first line as:
#   // sources: path/one.swift path/two.swift
status=0
for suite in apps/macos/Tests/*SwiftTests/*.swift; do
  [[ -e "$suite" ]] || continue
  sources=$(sed -n '1s|^// sources: ||p' "$suite")
  [[ -n "$sources" ]] || {
    echo "FAIL: $suite has no '// sources:' header" >&2
    status=1
    continue
  }

  name=$(basename "$suite" .swift)
  # swift-testing needs an entry point; -parse-as-library alone gives none.
  cat > "$OUT/$name-main.swift" <<'ENTRY'
import Testing
@main struct Runner {
  static func main() async { await Testing.__swiftPMEntryPoint() as Never }
}
ENTRY
  # shellcheck disable=SC2086
  if ! swiftc -parse-as-library \
      -F "$FRAMEWORKS" -framework Testing \
      -load-plugin-library "$MACROS" \
      -Xlinker -rpath -Xlinker "$FRAMEWORKS" \
      -Xlinker -rpath -Xlinker "$SWIFT_LIBS" \
      -Xlinker -rpath -Xlinker "$TESTING_LIBS" \
      -o "$OUT/$name" $sources "$suite" "$OUT/$name-main.swift" 2>&1; then
    echo "FAIL: $name did not compile" >&2
    status=1
    continue
  fi

  echo "== $name"
  "$OUT/$name" --testing-library swift-testing || status=1
done

exit "$status"
