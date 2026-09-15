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
swift build --package-path "$repo_root/apps/macos" --scratch-path "$scratch_path" \
  --product BridgeVMControl -Xswiftc -DBRIDGEVM_APP_UI_HOST 2>&1 | tee "$record/build.log"
if ! swift test --package-path "$repo_root/apps/macos" --scratch-path "$scratch_path" \
    -Xswiftc -DBRIDGEVM_APP_UI_HOST --list-tests > "$record/discovery.txt" 2> "$record/discovery.log"; then
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
swift test --package-path "$repo_root/apps/macos" --scratch-path "$scratch_path" \
  -Xswiftc -DBRIDGEVM_APP_UI_HOST --skip-build \
  --filter 'BridgeVMAppUIHostTests[.]AppUIHostContractTests' 2>&1 | tee "$record/tests.log"
swift build --package-path "$repo_root/apps/macos" --scratch-path "$scratch_path" \
  -Xswiftc -DBRIDGEVM_APP_UI_HOST --show-bin-path > "$record/bin-path.txt"
host_binary="$(cat "$record/bin-path.txt")/BridgeVMControl"
shasum -a 256 "$host_binary" > "$record/host-binary.sha256"
echo "native app host contracts: PASS (seven discovered tests; no app launched); evidence: $record"
