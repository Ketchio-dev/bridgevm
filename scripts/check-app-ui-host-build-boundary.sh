#!/bin/bash
# Compile-only boundary: the opt-in host must never become a release product.
set -euo pipefail
umask 077
fail() { echo "app UI host build boundary: FAIL: $*" >&2; exit 1; }
if [[ $# != 4 || $1 != --diagnostic-binary || $3 != --output ]]; then
  echo 'usage: check-app-ui-host-build-boundary.sh --diagnostic-binary ABSOLUTE_BINARY --output ABSOLUTE_NEW_DIRECTORY' >&2
  exit 2
fi
diagnostic_binary=$2
boundary_output=$4
[[ $(uname -s) == Darwin ]] || fail 'macOS compiler required'
[[ "$diagnostic_binary" == /* && -f "$diagnostic_binary" && ! -L "$diagnostic_binary" ]] || fail 'diagnostic binary must be an absolute regular file'
[[ "$boundary_output" == /* && ! -e "$boundary_output" && ! -L "$boundary_output" ]] || fail 'output must be a new absolute directory'
[[ -d "$(dirname "$boundary_output")" ]] || fail 'output parent must already exist'
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
marker='native-app-ui-host-identity'
refusal='BRIDGEVM_APP_UI_HOST requires a separate DEBUG diagnostic build'
mkdir -m 700 "$boundary_output"
LC_ALL=C /usr/bin/file -b "$diagnostic_binary" > "$boundary_output/diagnostic-filetype.txt"
/usr/bin/grep -Eq '^Mach-O 64-bit executable (arm64|x86_64)$' "$boundary_output/diagnostic-filetype.txt" || fail 'positive control is not a native executable'
LC_ALL=C /usr/bin/grep -aFq "$marker" "$diagnostic_binary" || fail 'diagnostic positive control has no host identity marker'
shasum -a 256 "$diagnostic_binary" > "$boundary_output/diagnostic-binary.sha256"

ordinary_scratch="$boundary_output/ordinary-release"
if ! swift build --package-path "$repo_root/apps/macos" --scratch-path "$ordinary_scratch" \
    --configuration release --product BridgeVMControl > "$boundary_output/ordinary-release.log" 2>&1; then
  fail "ordinary release compilation failed; see $boundary_output/ordinary-release.log"
fi
ordinary_bin_dir=$(swift build --package-path "$repo_root/apps/macos" --scratch-path "$ordinary_scratch" \
  --configuration release --show-bin-path)
ordinary_binary="$ordinary_bin_dir/BridgeVMControl"
[[ -f "$ordinary_binary" && ! -L "$ordinary_binary" ]] || fail 'ordinary release product is absent'
if LC_ALL=C /usr/bin/grep -aFq "$marker" "$ordinary_binary"; then
  fail 'diagnostic host identity marker survived in the ordinary release product'
else
  marker_status=$?
  [[ $marker_status == 1 ]] || fail 'ordinary release marker inspection failed'
fi
shasum -a 256 "$ordinary_binary" > "$boundary_output/ordinary-release.sha256"

if swift build --package-path "$repo_root/apps/macos" --scratch-path "$boundary_output/forbidden-release" \
    --configuration release --product BridgeVMControl -Xswiftc -DBRIDGEVM_APP_UI_HOST \
    > "$boundary_output/forbidden-release.log" 2>&1; then
  fail 'release compilation incorrectly accepted BRIDGEVM_APP_UI_HOST'
else
  forbidden_status=$?
fi
printf '%s\n' "$forbidden_status" > "$boundary_output/forbidden-release.exit-status"
LC_ALL=C /usr/bin/grep -Eq "BridgeVMControlMain\\.swift:[0-9]+:[0-9]+: error: ${refusal}$" \
  "$boundary_output/forbidden-release.log" || fail 'release rejection did not contain the exact diagnostic-only compiler refusal'
printf '%s\n' 'app UI host build boundary: PASS (positive debug marker, absent release marker, exact release refusal; no app executed)' \
  | tee "$boundary_output/result.txt"
