#!/bin/bash
# Apple XCTest coverage for the explicitly compiled diagnostic host; no app launch.
set -euo pipefail
umask 077
if [[ $# != 2 || $1 != --scratch-path || $2 != /* ]]; then
  echo 'usage: check-app-ui-host-contracts.sh --scratch-path ABSOLUTE_DIRECTORY' >&2
  exit 2
fi
scratch_path=$2
[[ ! -L "$scratch_path" ]] || { echo 'scratch directory must not be a symlink' >&2; exit 2; }
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
mkdir -p "$scratch_path"
record=$(mktemp -d "$scratch_path/host-contract-evidence.XXXXXX")
arch=$(uname -m)
[[ "$arch" == arm64 || "$arch" == x86_64 ]] || { echo "unsupported host architecture: $arch" >&2; exit 2; }
# This isolated diagnostic requires macOS 15; ordinary/release builds retain macOS 14.
swift_args=(--package-path "$repo_root/apps/macos" --scratch-path "$scratch_path"
            -Xswiftc -DBRIDGEVM_APP_UI_HOST -Xswiftc -target -Xswiftc "$arch-apple-macosx15.0")
swift build "${swift_args[@]}" --product BridgeVMControl 2>&1 | tee "$record/build.log"
if ! swift test "${swift_args[@]}" --list-tests > "$record/discovery.txt" 2> "$record/discovery.log"; then
  cat "$record/discovery.log" >&2
  exit 1
fi
python3 - "$record/discovery.txt" <<'PY'
from pathlib import Path
import sys

prefix = "BridgeVMAppUIHostTests.AppUIHostContractTests/"
methods = {
    "testAcceptsOnlyExactRequestWithoutWritingToOutput",
    "testRejectsNonemptyOrNonprivateOutputWithoutRemovingEvidence",
    "testRejectsSymlinkedParentAndOutput",
    "testCancellationMarkerRefusesWithoutChangingOutputAdmission",
    "testFixtureConstructionHasNoDomainWorkOrLibraryWrites",
    "testIncompleteObservationCannotBecomeSuccessfulCompletion",
    "testUnpackagedTestProcessRefusesBeforeAppOrFixtureCreation",
}
expected = {prefix + method for method in methods}
actual = [line.strip() for line in Path(sys.argv[1]).read_text().splitlines()
          if line.strip().startswith("BridgeVMAppUIHostTests.")]
if len(actual) != 7 or set(actual) != expected:
    raise SystemExit(f"diagnostic host discovery differs: expected {sorted(expected)!r}; got {actual!r}")
print("diagnostic host discovery: exactly seven qualified Apple XCTest methods")
PY
swift test "${swift_args[@]}" --skip-build \
  --filter 'BridgeVMAppUIHostTests[.]AppUIHostContractTests' 2>&1 | tee "$record/tests.log"
host_binary="$(swift build "${swift_args[@]}" --show-bin-path | tee "$record/bin-path.txt")/BridgeVMControl"
shasum -a 256 "$host_binary" > "$record/host-binary.sha256"
echo "native app host contracts: PASS (seven discovered tests; no app launched); evidence: $record"
