#!/usr/bin/env bash
# A16: prove a release build cannot be redirected by environment or PATH.
#
# The compiled artifact is checked, not the source. `#if DEBUG` is easy to
# write and easy to get wrong -- a stray `#else`, a target built with the wrong
# configuration, or a helper the guard does not cover all look correct in a
# diff. A string that is absent from the release object cannot be read at run
# time by any code path.
#
# The debug build is checked too, as a control: if the strings were missing
# from both, this check would pass while proving nothing.
set -uo pipefail

REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$REPO"

BUILD=apps/macos/.build/arm64-apple-macosx
OBJECTS=(
  "BridgeVMControl.build/VTPMStateSecurity.swift.o"
  "BridgeVMControl.build/HvfEngineSession.swift.o"
)

# Each of these lets something outside the signed bundle decide what the app
# runs, or where it reads the repository from.
FORBIDDEN_IN_RELEASE=(
  "BRIDGEVM_REPO_ROOT"
  "BRIDGEVM_SWTPM_BIN"
  "/opt/homebrew/bin/swtpm"
  "/usr/local/bin/swtpm"
)

status=0
missing_build=0

count_in() {
  local config="$1" needle="$2" total=0 obj
  for obj in "${OBJECTS[@]}"; do
    local path="$BUILD/$config/$obj"
    [[ -f "$path" ]] || { missing_build=1; continue; }
    total=$(( total + $(strings "$path" 2>/dev/null | grep -c -- "$needle") ))
  done
  printf '%s' "$total"
}

for needle in "${FORBIDDEN_IN_RELEASE[@]}"; do
  release_hits=$(count_in release "$needle")
  debug_hits=$(count_in debug "$needle")

  if [[ "$missing_build" == 1 ]]; then
    echo "SKIP: build both configurations first:" >&2
    echo "  swift build --package-path apps/macos" >&2
    echo "  swift build -c release --package-path apps/macos" >&2
    echo "release overrides: SKIP (artifacts absent)"
    exit 0
  fi

  if [[ "$release_hits" != 0 ]]; then
    echo "FAIL: $needle is reachable in the release build ($release_hits refs)" >&2
    status=1
  elif [[ "$debug_hits" == 0 ]]; then
    # Absent from both means the check is measuring nothing.
    echo "FAIL: $needle is absent from the debug build too; this check is vacuous" >&2
    status=1
  else
    printf '  %-26s release=0 debug=%s\n' "$needle" "$debug_hits"
  fi
done

if [[ "$status" == 0 ]]; then
  echo "release overrides: PASS (${#FORBIDDEN_IN_RELEASE[@]} overrides debug-only)"
else
  echo "release overrides: FAIL" >&2
fi
exit "$status"
