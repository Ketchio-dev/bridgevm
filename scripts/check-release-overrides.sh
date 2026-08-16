#!/usr/bin/env bash
# A16: prove a release build cannot be redirected by environment or PATH.
#
# The compiled artifact is checked, not the source. `#if DEBUG` is easy to
# write and easy to get wrong. A string absent from the release object cannot be
# read there. Debug is a control: missing from both would prove nothing.
set -uo pipefail

REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$REPO" || exit 1

BUILD=apps/macos/.build/arm64-apple-macosx
# Every object, not a named list, which covers only today's readers.
objects_in() { find "$BUILD/$1" -name '*.o' 2>/dev/null; }

# Each of these lets something outside the signed bundle decide what the app
# runs, or where it reads the repository from.
FORBIDDEN_IN_RELEASE=(
  "BRIDGEVM_REPO_ROOT"
  "BRIDGEVM_SWTPM_BIN"
  "/usr/local/bin/swtpm"
)

# Known violation, recorded not hidden: QemuCompatBackend hardcodes all three
# and is reachable via .qemuCompat. Scanning two named objects hid them.
KNOWN_UNFIXED=(
  "/opt/homebrew/bin/swtpm" "/opt/homebrew/bin/qemu-system-aarch64"
  "/opt/homebrew/share/qemu/edk2-aarch64-code.fd"
)

status=0
missing_build=0

count_in() {
  local objects
  objects="$(objects_in "$1")"
  [[ -n "$objects" ]] || { missing_build=1; printf '0'; return; }
  xargs strings <<< "$objects" 2>/dev/null | grep -c -- "$2" || true
}

for needle in "${KNOWN_UNFIXED[@]}"; do
  [[ "$(count_in release "$needle")" != 0 ]] ||
    { echo "FAIL: $needle is fixed; remove it from KNOWN_UNFIXED" >&2; status=1; }
done
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
