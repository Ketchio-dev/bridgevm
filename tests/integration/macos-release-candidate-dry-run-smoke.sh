#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$ROOT/packaging/macos/build-release-candidate.sh"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

assert_contains() {
  local haystack="$1"
  local needle="$2"
  [[ "$haystack" == *"$needle"* ]] || fail "expected dry-run output to contain: $needle"
}

assert_not_contains() {
  local haystack="$1"
  local needle="$2"
  [[ "$haystack" != *"$needle"* ]] || fail "expected dry-run output not to contain: $needle"
}

assert_ordered_patterns() {
  local haystack="$1"
  shift
  local previous=0
  local pattern
  local line
  for pattern in "$@"; do
    line="$(printf '%s\n' "$haystack" | awk -v pattern="$pattern" -v previous="$previous" \
      'index($0, pattern) && NR > previous { print NR; exit }')"
    [[ -n "$line" ]] || fail "expected pattern after line $previous: $pattern"
    previous="$line"
  done
}

assert_count() {
  local haystack="$1"
  local pattern="$2"
  local expected="$3"
  local actual
  actual="$(printf '%s\n' "$haystack" | awk -v pattern="$pattern" \
    'index($0, pattern) { count++ } END { print count + 0 }')"
  [[ "$actual" == "$expected" ]] || {
    fail "expected '$pattern' count $expected; got $actual"
  }
}

[[ -x "$SCRIPT" ]] || fail "missing executable release candidate script: $SCRIPT"

set +e
missing_output="$("$SCRIPT" --dry-run 2>&1)"
missing_status=$?
set -e
[[ "$missing_status" -eq 2 ]] || fail "missing-input dry-run should exit 2, got $missing_status"
assert_contains "$missing_output" "Missing required release input"

set +e
missing_icon_output="$(
  env \
    BRIDGEVM_RELEASE_CODESIGN_IDENTITY="Developer ID Application: Example Corp (TEAMID)" \
    BRIDGEVM_NOTARYTOOL_KEYCHAIN_PROFILE="bridgevm-notary-profile" \
    BRIDGEVM_BUNDLE_IDENTIFIER="com.example.bridgevm" \
    BRIDGEVM_BUNDLE_SHORT_VERSION="1.0.0" \
    BRIDGEVM_BUNDLE_VERSION="100" \
    "$SCRIPT" --dry-run 2>&1
)"
missing_icon_status=$?
set -e
[[ "$missing_icon_status" -eq 2 ]] || fail "missing-icon dry-run should exit 2, got $missing_icon_status"
assert_contains "$missing_icon_output" "Missing required release input: BRIDGEVM_MACOS_ICON_FILE"

set +e
missing_identity_output="$(
  env \
    BRIDGEVM_NOTARYTOOL_KEYCHAIN_PROFILE="bridgevm-notary-profile" \
    BRIDGEVM_BUNDLE_IDENTIFIER="com.example.bridgevm" \
    BRIDGEVM_BUNDLE_SHORT_VERSION="1.0.0" \
    BRIDGEVM_BUNDLE_VERSION="100" \
    BRIDGEVM_MACOS_ICON_FILE="/tmp/BridgeVM.icns" \
    "$SCRIPT" --dry-run 2>&1
)"
missing_identity_status=$?
set -e
[[ "$missing_identity_status" -eq 2 ]] || fail "missing-identity dry-run should exit 2, got $missing_identity_status"
assert_contains "$missing_identity_output" "Missing required release input: BRIDGEVM_RELEASE_CODESIGN_IDENTITY"

set +e
invalid_identity_output="$(
  env \
    BRIDGEVM_RELEASE_CODESIGN_IDENTITY="Apple Development: Example Corp (TEAMID)" \
    BRIDGEVM_NOTARYTOOL_KEYCHAIN_PROFILE="bridgevm-notary-profile" \
    BRIDGEVM_BUNDLE_IDENTIFIER="com.example.bridgevm" \
    BRIDGEVM_BUNDLE_SHORT_VERSION="1.0.0" \
    BRIDGEVM_BUNDLE_VERSION="100" \
    BRIDGEVM_MACOS_ICON_FILE="/tmp/BridgeVM.icns" \
    "$SCRIPT" --dry-run 2>&1
)"
invalid_identity_status=$?
set -e
[[ "$invalid_identity_status" -eq 2 ]] || fail "invalid-identity dry-run should exit 2, got $invalid_identity_status"
assert_contains "$invalid_identity_output" "BRIDGEVM_RELEASE_CODESIGN_IDENTITY must be a Developer ID Application identity"
assert_contains "$invalid_identity_output" "Apple Development: Example Corp (TEAMID)"

set +e
missing_identity_team_without_pin_output="$(
  env \
    BRIDGEVM_RELEASE_CODESIGN_IDENTITY="Developer ID Application: Example Corp" \
    BRIDGEVM_NOTARYTOOL_KEYCHAIN_PROFILE="bridgevm-notary-profile" \
    BRIDGEVM_BUNDLE_IDENTIFIER="com.example.bridgevm" \
    BRIDGEVM_BUNDLE_SHORT_VERSION="1.0.0" \
    BRIDGEVM_BUNDLE_VERSION="100" \
    BRIDGEVM_MACOS_ICON_FILE="/tmp/BridgeVM.icns" \
    "$SCRIPT" --dry-run 2>&1
)"
missing_identity_team_without_pin_status=$?
set -e
[[ "$missing_identity_team_without_pin_status" -eq 2 ]] || fail "missing-identity-team-without-pin dry-run should exit 2, got $missing_identity_team_without_pin_status"
assert_contains "$missing_identity_team_without_pin_output" "BRIDGEVM_RELEASE_CODESIGN_IDENTITY must end with a parenthesized Apple team identifier"
assert_contains "$missing_identity_team_without_pin_output" "Developer ID Application: Example Corp"

set +e
missing_notary_output="$(
  env \
    BRIDGEVM_RELEASE_CODESIGN_IDENTITY="Developer ID Application: Example Corp (TEAMID)" \
    BRIDGEVM_BUNDLE_IDENTIFIER="com.example.bridgevm" \
    BRIDGEVM_BUNDLE_SHORT_VERSION="1.0.0" \
    BRIDGEVM_BUNDLE_VERSION="100" \
    BRIDGEVM_MACOS_ICON_FILE="/tmp/BridgeVM.icns" \
    "$SCRIPT" --dry-run 2>&1
)"
missing_notary_status=$?
set -e
[[ "$missing_notary_status" -eq 2 ]] || fail "missing-notary dry-run should exit 2, got $missing_notary_status"
assert_contains "$missing_notary_output" "Missing required release input: BRIDGEVM_NOTARYTOOL_KEYCHAIN_PROFILE"

set +e
blank_notary_output="$(
  env \
    BRIDGEVM_RELEASE_CODESIGN_IDENTITY="Developer ID Application: Example Corp (TEAMID)" \
    BRIDGEVM_NOTARYTOOL_KEYCHAIN_PROFILE="   " \
    BRIDGEVM_BUNDLE_IDENTIFIER="com.example.bridgevm" \
    BRIDGEVM_BUNDLE_SHORT_VERSION="1.0.0" \
    BRIDGEVM_BUNDLE_VERSION="100" \
    BRIDGEVM_MACOS_ICON_FILE="/tmp/BridgeVM.icns" \
    "$SCRIPT" --dry-run 2>&1
)"
blank_notary_status=$?
set -e
[[ "$blank_notary_status" -eq 2 ]] || fail "blank-notary dry-run should exit 2, got $blank_notary_status"
assert_contains "$blank_notary_output" "Missing required release input: BRIDGEVM_NOTARYTOOL_KEYCHAIN_PROFILE"

set +e
invalid_icon_output="$(
  env \
    BRIDGEVM_RELEASE_CODESIGN_IDENTITY="Developer ID Application: Example Corp (TEAMID)" \
    BRIDGEVM_NOTARYTOOL_KEYCHAIN_PROFILE="bridgevm-notary-profile" \
    BRIDGEVM_BUNDLE_IDENTIFIER="com.example.bridgevm" \
    BRIDGEVM_BUNDLE_SHORT_VERSION="1.0.0" \
    BRIDGEVM_BUNDLE_VERSION="100" \
    BRIDGEVM_MACOS_ICON_FILE="/tmp/BridgeVM.png" \
    "$SCRIPT" --dry-run 2>&1
)"
invalid_icon_status=$?
set -e
[[ "$invalid_icon_status" -eq 2 ]] || fail "invalid-icon dry-run should exit 2, got $invalid_icon_status"
assert_contains "$invalid_icon_output" "BridgeVM release icon must use the .icns extension: /tmp/BridgeVM.png"

set +e
mismatched_team_output="$(
  env \
    BRIDGEVM_RELEASE_CODESIGN_IDENTITY="Developer ID Application: Example Corp (ABCDE12345)" \
    BRIDGEVM_RELEASE_TEAM_ID="ZZZZZ99999" \
    BRIDGEVM_NOTARYTOOL_KEYCHAIN_PROFILE="bridgevm-notary-profile" \
    BRIDGEVM_BUNDLE_IDENTIFIER="com.example.bridgevm" \
    BRIDGEVM_BUNDLE_SHORT_VERSION="1.0.0" \
    BRIDGEVM_BUNDLE_VERSION="100" \
    BRIDGEVM_MACOS_ICON_FILE="/tmp/BridgeVM.icns" \
    "$SCRIPT" --dry-run 2>&1
)"
mismatched_team_status=$?
set -e
[[ "$mismatched_team_status" -eq 2 ]] || fail "mismatched-team dry-run should exit 2, got $mismatched_team_status"
assert_contains "$mismatched_team_output" "BRIDGEVM_RELEASE_TEAM_ID does not match signing identity team"
assert_contains "$mismatched_team_output" "expected ZZZZZ99999, identity has ABCDE12345"

set +e
missing_identity_team_output="$(
  env \
    BRIDGEVM_RELEASE_CODESIGN_IDENTITY="Developer ID Application: Example Corp" \
    BRIDGEVM_RELEASE_TEAM_ID="TEAMID" \
    BRIDGEVM_NOTARYTOOL_KEYCHAIN_PROFILE="bridgevm-notary-profile" \
    BRIDGEVM_BUNDLE_IDENTIFIER="com.example.bridgevm" \
    BRIDGEVM_BUNDLE_SHORT_VERSION="1.0.0" \
    BRIDGEVM_BUNDLE_VERSION="100" \
    BRIDGEVM_MACOS_ICON_FILE="/tmp/BridgeVM.icns" \
    "$SCRIPT" --dry-run 2>&1
)"
missing_identity_team_status=$?
set -e
[[ "$missing_identity_team_status" -eq 2 ]] || fail "missing-identity-team dry-run should exit 2, got $missing_identity_team_status"
assert_contains "$missing_identity_team_output" "BRIDGEVM_RELEASE_TEAM_ID requires a signing identity ending with a parenthesized team identifier"
assert_contains "$missing_identity_team_output" "Developer ID Application: Example Corp"

output="$(
  env \
    BRIDGEVM_RELEASE_CODESIGN_IDENTITY="Developer ID Application: Example Corp (TEAMID)" \
    BRIDGEVM_RELEASE_TEAM_ID="TEAMID" \
    BRIDGEVM_NOTARYTOOL_KEYCHAIN_PROFILE="bridgevm-notary-profile" \
    BRIDGEVM_BUNDLE_IDENTIFIER="com.example.bridgevm" \
    BRIDGEVM_BUNDLE_SHORT_VERSION="1.0.0" \
    BRIDGEVM_BUNDLE_VERSION="100" \
    BRIDGEVM_MACOS_ICON_FILE="/tmp/BridgeVM.icns" \
    "$SCRIPT" --dry-run
)"
verifier="$(printf '%q' "$ROOT/packaging/macos/verify-release-candidate.sh")"

assert_contains "$output" "build-debug-app-bundle.sh"
assert_contains "$output" "preflight-release-credentials.sh --dry-run"
assert_contains "$output" "PASS: release credential preflight dry-run"
assert_contains "$output" "BRIDGEVM_MACOS_BUILD_CONFIGURATION=release"
assert_contains "$output" "BRIDGEVM_CODESIGN_IDENTITY=Developer\\ ID\\ Application:\\ Example\\ Corp\\ \\(TEAMID\\)"
assert_contains "$output" "BRIDGEVM_MACOS_ICON_FILE=/tmp/BridgeVM.icns"
assert_contains "$output" "build-sign-apple-vz-runner.sh"
assert_contains "$output" "AppleVzRunner.release.entitlements"
assert_contains "$output" "--release --output"
assert_contains "$output" "Contents/Helpers/AppleVzRunner"
assert_contains "$output" "build-sign-hvf-windows-probe.sh"
assert_contains "$output" "HvfRunner.release.entitlements"
assert_contains "$output" "Contents/Applications/BridgeVMControl.app"
assert_contains "$output" "target/release/examples/hvf_gic_boot_probe"
assert_contains "$output" "--verify-only"
assert_contains "$output" "codesign --force --options runtime"
assert_contains "$output" "Contents/Helpers/bridgevmd"
assert_contains "$output" "Contents/Helpers/lightvm-runner"
assert_contains "$output" "ditto -c -k --keepParent"
assert_contains "$output" "BridgeVM-notary.zip"
assert_contains "$output" "hdiutil create"
assert_contains "$output" "xcrun notarytool submit"
assert_contains "$output" "--output-format json"
assert_contains "$output" "BridgeVM-notary-submit.json"
assert_contains "$output" "BridgeVM-notary-log.json"
assert_contains "$output" "BridgeVM-dmg-notary-submit.json"
assert_contains "$output" "BridgeVM-dmg-notary-log.json"
assert_contains "$output" "--keychain-profile bridgevm-notary-profile --wait"
assert_contains "$output" "BRIDGEVM_RELEASE_TEAM_ID=TEAMID $verifier"
assert_contains "$output" "verify notarytool status Accepted"
assert_contains "$output" "xcrun stapler staple"
assert_contains "$output" "xcrun stapler validate"
assert_contains "$output" "write-artifact-manifest.sh"
assert_contains "$output" "verify-release-candidate.sh"
assert_count "$output" "xcrun notarytool submit" 2
assert_count "$output" "verify notarytool status Accepted" 2
assert_count "$output" "xcrun stapler staple" 2
assert_count "$output" "xcrun stapler validate" 2
assert_ordered_patterns "$output" \
  "preflight-release-credentials.sh --dry-run" \
  "build-debug-app-bundle.sh" \
  "build-sign-apple-vz-runner.sh" \
  "--verify-only" \
  "Contents/Helpers/bridgevmd" \
  "Contents/Helpers/lightvm-runner" \
  "codesign --force --options runtime" \
  "BridgeVM-notary.zip" \
  "xcrun notarytool submit" \
  "xcrun notarytool log" \
  "verify notarytool status Accepted" \
  "xcrun stapler staple" \
  "xcrun stapler validate" \
  "hdiutil create" \
  "codesign --force --sign" \
  "xcrun notarytool submit" \
  "xcrun notarytool log" \
  "verify notarytool status Accepted" \
  "xcrun stapler staple" \
  "xcrun stapler validate" \
  "write-artifact-manifest.sh" \
  "verify-release-candidate.sh"

custom_output="$(
  env \
    BRIDGEVM_RELEASE_CODESIGN_IDENTITY="Developer ID Application: Example Corp (TEAMID)" \
    BRIDGEVM_NOTARYTOOL_KEYCHAIN_PROFILE="bridgevm-notary-profile" \
    BRIDGEVM_BUNDLE_IDENTIFIER="com.example.bridgevm" \
    BRIDGEVM_BUNDLE_SHORT_VERSION="1.0.0" \
    BRIDGEVM_BUNDLE_VERSION="100" \
    BRIDGEVM_MACOS_ICON_FILE="/tmp/BridgeVM.icns" \
    BRIDGEVM_MACOS_APP_NAME="BridgeVM Pro" \
    "$SCRIPT" --dry-run
)"

custom_app="$(printf '%q' "$ROOT/target/macos-release/BridgeVM Pro.app")"
custom_app_notary_zip="$(printf '%q' "$ROOT/target/macos-release/BridgeVM Pro-notary.zip")"
default_dmg="$(printf '%q' "$ROOT/target/macos-release/BridgeVM.dmg")"
default_manifest="$(printf '%q' "$ROOT/target/macos-release/BridgeVM-artifacts.txt")"

assert_contains "$custom_output" "BRIDGEVM_MACOS_APP_NAME=BridgeVM\\ Pro"
assert_contains "$custom_output" "$custom_app"
assert_contains "$custom_output" "$custom_app_notary_zip"
assert_contains "$custom_output" "/bridgevm-release-dmg-stage."
assert_contains "$custom_output" "/BridgeVM\\ Pro.app"
assert_contains "$custom_output" "hdiutil create -volname BridgeVM"
assert_contains "$custom_output" "$default_dmg"
assert_contains "$custom_output" "write-artifact-manifest.sh $custom_app $default_dmg $default_manifest"
assert_contains "$custom_output" "verify-release-candidate.sh $custom_app $default_dmg"
assert_not_contains "$custom_output" "/target/macos-release/BridgeVM\\ Pro.dmg"
assert_not_contains "$custom_output" "/target/macos-release/BridgeVM\\ Pro-artifacts.txt"
assert_ordered_patterns "$custom_output" \
  "preflight-release-credentials.sh --dry-run" \
  "build-debug-app-bundle.sh" \
  "build-sign-apple-vz-runner.sh" \
  "--verify-only" \
  "codesign --force --options runtime" \
  "ditto -c -k --keepParent" \
  "xcrun notarytool submit" \
  "ditto" \
  "/bridgevm-release-dmg-stage." \
  "hdiutil create" \
  "BridgeVM.dmg" \
  "write-artifact-manifest.sh" \
  "verify-release-candidate.sh"

nested_dmg="$ROOT/target/macos-release/nested/dmg/BridgeVM-release.dmg"
nested_app_notary_zip="$ROOT/target/macos-release/nested/notary/BridgeVM-app.zip"
nested_app_submit="$ROOT/target/macos-release/nested/notary/app-submit.json"
nested_app_log="$ROOT/target/macos-release/nested/notary/app-log.json"
nested_dmg_submit="$ROOT/target/macos-release/nested/notary/dmg-submit.json"
nested_dmg_log="$ROOT/target/macos-release/nested/notary/dmg-log.json"
nested_manifest="$ROOT/target/macos-release/nested/audit/BridgeVM-artifacts.txt"
nested_output="$(
  env \
    BRIDGEVM_RELEASE_CODESIGN_IDENTITY="Developer ID Application: Example Corp (TEAMID)" \
    BRIDGEVM_NOTARYTOOL_KEYCHAIN_PROFILE="bridgevm-notary-profile" \
    BRIDGEVM_BUNDLE_IDENTIFIER="com.example.bridgevm" \
    BRIDGEVM_BUNDLE_SHORT_VERSION="1.0.0" \
    BRIDGEVM_BUNDLE_VERSION="100" \
    BRIDGEVM_MACOS_ICON_FILE="/tmp/BridgeVM.icns" \
    BRIDGEVM_MACOS_DMG="$nested_dmg" \
    BRIDGEVM_MACOS_DMG_VOLUME="BridgeVM Nightly" \
    BRIDGEVM_MACOS_APP_NOTARY_ZIP="$nested_app_notary_zip" \
    BRIDGEVM_MACOS_APP_NOTARY_SUBMIT_JSON="$nested_app_submit" \
    BRIDGEVM_MACOS_APP_NOTARY_LOG_JSON="$nested_app_log" \
    BRIDGEVM_MACOS_DMG_NOTARY_SUBMIT_JSON="$nested_dmg_submit" \
    BRIDGEVM_MACOS_DMG_NOTARY_LOG_JSON="$nested_dmg_log" \
    BRIDGEVM_MACOS_ARTIFACT_MANIFEST="$nested_manifest" \
    "$SCRIPT" --dry-run
)"

assert_contains "$nested_output" "mkdir -p"
assert_contains "$nested_output" "$(printf '%q' "$(dirname "$nested_app_notary_zip")")"
assert_contains "$nested_output" "$(printf '%q' "$(dirname "$nested_dmg")")"
assert_contains "$nested_output" "$(printf '%q' "$(dirname "$nested_manifest")")"
assert_contains "$nested_output" "$(printf '%q' "$nested_app_notary_zip")"
assert_contains "$nested_output" "$(printf '%q' "$nested_dmg")"
assert_contains "$nested_output" "$(printf '%q' "$nested_manifest")"
assert_contains "$nested_output" "hdiutil create -volname BridgeVM\\ Nightly"
assert_contains "$nested_output" "BRIDGEVM_MACOS_DMG_VOLUME=BridgeVM\\ Nightly"
assert_ordered_patterns "$nested_output" \
  "mkdir -p" \
  "preflight-release-credentials.sh --dry-run" \
  "build-debug-app-bundle.sh" \
  "ditto -c -k --keepParent" \
  "hdiutil create" \
  "write-artifact-manifest.sh"

echo "PASS: macOS release candidate dry-run smoke"
