#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$ROOT/packaging/macos/preflight-release-credentials.sh"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  [[ "$haystack" == *"$needle"* ]] || fail "$label missing '$needle'; got: $haystack"
}

WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-release-preflight.XXXXXX")"
cleanup() {
  rm -rf "$WORKDIR"
}
trap cleanup EXIT

FAKE_BIN="$WORKDIR/bin"
mkdir -p "$FAKE_BIN"

cat >"$FAKE_BIN/security" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
if [[ "$*" != "find-identity -v -p codesigning" ]]; then
  echo "unexpected security invocation: $*" >&2
  exit 64
fi
if [[ "${BRIDGEVM_FAKE_SECURITY_MISSING:-}" == "1" ]]; then
  echo "  1) ABCDEF1234567890 \"Apple Development: Example Corp (TEAMID)\""
else
  echo "  1) ABCDEF1234567890 \"Developer ID Application: Example Corp (TEAMID)\""
fi
echo "     1 valid identities found"
SH
chmod +x "$FAKE_BIN/security"

cat >"$FAKE_BIN/xcrun" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
if [[ "$*" != "notarytool history --keychain-profile bridgevm-notary-profile --output-format json" ]]; then
  echo "unexpected xcrun invocation: $*" >&2
  exit 64
fi
if [[ "${BRIDGEVM_FAKE_NOTARY_PROFILE_MISSING:-}" == "1" ]]; then
  echo "No Keychain password item found for profile: bridgevm-notary-profile" >&2
  exit 1
fi
printf '{"history":[]}\n'
SH
chmod +x "$FAKE_BIN/xcrun"

ICON="$WORKDIR/BridgeVM.icns"
printf 'fake icns\n' >"$ICON"

preflight_env=(
  "BRIDGEVM_RELEASE_CODESIGN_IDENTITY=Developer ID Application: Example Corp (TEAMID)"
  "BRIDGEVM_RELEASE_TEAM_ID=TEAMID"
  "BRIDGEVM_NOTARYTOOL_KEYCHAIN_PROFILE=bridgevm-notary-profile"
  "BRIDGEVM_BUNDLE_IDENTIFIER=com.example.bridgevm"
  "BRIDGEVM_BUNDLE_SHORT_VERSION=1.0.0"
  "BRIDGEVM_BUNDLE_VERSION=100"
  "BRIDGEVM_MACOS_ICON_FILE=$ICON"
  "BRIDGEVM_SECURITY_TOOL=$FAKE_BIN/security"
  "BRIDGEVM_XCRUN_TOOL=$FAKE_BIN/xcrun"
)

[[ -x "$SCRIPT" ]] || fail "missing executable preflight script: $SCRIPT"

dry_output="$(env "${preflight_env[@]}" "$SCRIPT" --dry-run)"
assert_contains "$dry_output" "PASS: release credential preflight dry-run" "dry-run preflight"
assert_contains "$dry_output" "Would run: $FAKE_BIN/security find-identity -v -p codesigning" "dry-run preflight"
assert_contains "$dry_output" "Would run: $FAKE_BIN/xcrun notarytool history --keychain-profile bridgevm-notary-profile --output-format json" "dry-run preflight"

output="$(env "${preflight_env[@]}" "$SCRIPT")"
assert_contains "$output" "PASS: release credential preflight" "release preflight"
assert_contains "$output" "Developer ID identity: Developer ID Application: Example Corp (TEAMID)" "release preflight"
assert_contains "$output" "Notary profile: bridgevm-notary-profile" "release preflight"

set +e
missing_identity_output="$(env "${preflight_env[@]}" BRIDGEVM_FAKE_SECURITY_MISSING=1 "$SCRIPT" 2>&1)"
missing_identity_status=$?
set -e
[[ "$missing_identity_status" -eq 1 ]] || fail "missing identity preflight should exit 1, got $missing_identity_status"
assert_contains "$missing_identity_output" "Developer ID signing identity is not available in the keychain" "missing identity preflight"

set +e
missing_notary_output="$(env "${preflight_env[@]}" BRIDGEVM_FAKE_NOTARY_PROFILE_MISSING=1 "$SCRIPT" 2>&1)"
missing_notary_status=$?
set -e
[[ "$missing_notary_status" -eq 1 ]] || fail "missing notary preflight should exit 1, got $missing_notary_status"
assert_contains "$missing_notary_output" "notarytool keychain profile is not available or is not usable" "missing notary preflight"
assert_contains "$missing_notary_output" "No Keychain password item found for profile" "missing notary preflight"

set +e
missing_icon_output="$(env "${preflight_env[@]}" BRIDGEVM_MACOS_ICON_FILE="$WORKDIR/missing.icns" "$SCRIPT" 2>&1)"
missing_icon_status=$?
set -e
[[ "$missing_icon_status" -eq 2 ]] || fail "missing icon preflight should exit 2, got $missing_icon_status"
assert_contains "$missing_icon_output" "BridgeVM release icon is missing" "missing icon preflight"

set +e
mismatched_team_output="$(env "${preflight_env[@]}" BRIDGEVM_RELEASE_TEAM_ID=ZZZZZ99999 "$SCRIPT" --dry-run 2>&1)"
mismatched_team_status=$?
set -e
[[ "$mismatched_team_status" -eq 2 ]] || fail "mismatched team dry-run should exit 2, got $mismatched_team_status"
assert_contains "$mismatched_team_output" "BRIDGEVM_RELEASE_TEAM_ID does not match signing identity team" "mismatched team preflight"

echo "PASS: macOS release credential preflight smoke"
