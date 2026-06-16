#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
IDENTITY="${BRIDGEVM_RELEASE_CODESIGN_IDENTITY:-${BRIDGEVM_CODESIGN_IDENTITY:-}}"
NOTARY_PROFILE="${BRIDGEVM_NOTARYTOOL_KEYCHAIN_PROFILE:-${BRIDGEVM_NOTARYTOOL_PROFILE:-}}"
SECURITY_TOOL="${BRIDGEVM_SECURITY_TOOL:-security}"
XCRUN_TOOL="${BRIDGEVM_XCRUN_TOOL:-xcrun}"
DRY_RUN=0

usage() {
  cat >&2 <<'EOF'
usage: packaging/macos/preflight-release-credentials.sh [--dry-run]

Checks the local macOS release signing inputs before the slower release build:
Developer ID Application identity, optional team pin, notarytool keychain
profile, bundle metadata, and final .icns icon.

Required environment:
  BRIDGEVM_RELEASE_CODESIGN_IDENTITY  Developer ID Application identity
                                     (BRIDGEVM_CODESIGN_IDENTITY also accepted)
  BRIDGEVM_NOTARYTOOL_KEYCHAIN_PROFILE
                                      notarytool keychain profile
                                     (BRIDGEVM_NOTARYTOOL_PROFILE also accepted)
  BRIDGEVM_BUNDLE_IDENTIFIER          release bundle identifier
  BRIDGEVM_BUNDLE_SHORT_VERSION       marketing version
  BRIDGEVM_BUNDLE_VERSION             build number
  BRIDGEVM_MACOS_ICON_FILE            final .icns icon file

Optional environment:
  BRIDGEVM_RELEASE_TEAM_ID            expected Developer ID team identifier
  BRIDGEVM_SECURITY_TOOL              override security tool for tests
  BRIDGEVM_XCRUN_TOOL                 override xcrun tool for tests
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      usage
      exit 2
      ;;
  esac
done

is_blank() {
  local value="$1"
  [[ -z "${value//[[:space:]]/}" ]]
}

require_env() {
  local name="$1"
  local value="${!name:-}"
  if is_blank "$value"; then
    echo "Missing required release input: $name" >&2
    exit 2
  fi
}

identity_team_id() {
  local identity="$1"
  if [[ "$identity" =~ \(([[:alnum:]]+)\)$ ]]; then
    printf '%s\n' "${BASH_REMATCH[1]}"
  fi
}

validate_release_identity() {
  if is_blank "$IDENTITY"; then
    echo "Missing required release input: BRIDGEVM_RELEASE_CODESIGN_IDENTITY" >&2
    exit 2
  fi
  if [[ "$IDENTITY" != Developer\ ID\ Application:\ * ]]; then
    echo "BRIDGEVM_RELEASE_CODESIGN_IDENTITY must be a Developer ID Application identity, for example: Developer ID Application: Example Corp (TEAMID)" >&2
    echo "Got: $IDENTITY" >&2
    exit 2
  fi
  if [[ -z "${BRIDGEVM_RELEASE_TEAM_ID:-}" && -z "$(identity_team_id "$IDENTITY")" ]]; then
    echo "BRIDGEVM_RELEASE_CODESIGN_IDENTITY must end with a parenthesized Apple team identifier, for example: Developer ID Application: Example Corp (TEAMID)" >&2
    echo "Got: $IDENTITY" >&2
    exit 2
  fi
}

validate_release_team_id() {
  local expected_team_id="${BRIDGEVM_RELEASE_TEAM_ID:-}"
  local actual_team_id
  [[ -n "$expected_team_id" ]] || return 0
  actual_team_id="$(identity_team_id "$IDENTITY")"
  if [[ -z "$actual_team_id" ]]; then
    echo "BRIDGEVM_RELEASE_TEAM_ID requires a signing identity ending with a parenthesized team identifier: $IDENTITY" >&2
    exit 2
  fi
  if [[ "$actual_team_id" != "$expected_team_id" ]]; then
    echo "BRIDGEVM_RELEASE_TEAM_ID does not match signing identity team: expected $expected_team_id, identity has $actual_team_id" >&2
    exit 2
  fi
}

validate_notary_profile() {
  if is_blank "$NOTARY_PROFILE"; then
    echo "Missing required release input: BRIDGEVM_NOTARYTOOL_KEYCHAIN_PROFILE" >&2
    exit 2
  fi
}

validate_icon_input() {
  [[ "$BRIDGEVM_MACOS_ICON_FILE" == *.icns ]] || {
    echo "BridgeVM release icon must use the .icns extension: $BRIDGEVM_MACOS_ICON_FILE" >&2
    exit 2
  }
  if [[ "$DRY_RUN" != "1" ]]; then
    [[ -f "$BRIDGEVM_MACOS_ICON_FILE" ]] || {
      echo "BridgeVM release icon is missing: $BRIDGEVM_MACOS_ICON_FILE" >&2
      exit 2
    }
    [[ -s "$BRIDGEVM_MACOS_ICON_FILE" ]] || {
      echo "BridgeVM release icon is empty: $BRIDGEVM_MACOS_ICON_FILE" >&2
      exit 2
    }
  fi
}

verify_identity_available() {
  local output
  if ! output="$("$SECURITY_TOOL" find-identity -v -p codesigning 2>&1)"; then
    echo "Unable to inspect code signing identities with $SECURITY_TOOL" >&2
    printf '%s\n' "$output" >&2
    exit 1
  fi
  if [[ "$output" != *"$IDENTITY"* ]]; then
    echo "Developer ID signing identity is not available in the keychain: $IDENTITY" >&2
    exit 1
  fi
}

verify_notary_profile_available() {
  local output
  if ! output="$("$XCRUN_TOOL" notarytool history --keychain-profile "$NOTARY_PROFILE" --output-format json 2>&1)"; then
    echo "notarytool keychain profile is not available or is not usable: $NOTARY_PROFILE" >&2
    printf '%s\n' "$output" >&2
    exit 1
  fi
}

require_env BRIDGEVM_BUNDLE_IDENTIFIER
require_env BRIDGEVM_BUNDLE_SHORT_VERSION
require_env BRIDGEVM_BUNDLE_VERSION
require_env BRIDGEVM_MACOS_ICON_FILE
validate_release_identity
validate_notary_profile
validate_release_team_id
validate_icon_input

if [[ "$DRY_RUN" == "1" ]]; then
  echo "PASS: release credential preflight dry-run"
  echo "Developer ID identity: $IDENTITY"
  echo "Developer ID team: ${BRIDGEVM_RELEASE_TEAM_ID:-$(identity_team_id "$IDENTITY")}"
  echo "Notary profile: $NOTARY_PROFILE"
  echo "Bundle identifier: $BRIDGEVM_BUNDLE_IDENTIFIER"
  echo "Bundle short version: $BRIDGEVM_BUNDLE_SHORT_VERSION"
  echo "Bundle version: $BRIDGEVM_BUNDLE_VERSION"
  echo "Icon: $BRIDGEVM_MACOS_ICON_FILE"
  echo "Would run: $SECURITY_TOOL find-identity -v -p codesigning"
  echo "Would run: $XCRUN_TOOL notarytool history --keychain-profile $NOTARY_PROFILE --output-format json"
  exit 0
fi

verify_identity_available
verify_notary_profile_available

echo "PASS: release credential preflight"
echo "Developer ID identity: $IDENTITY"
echo "Developer ID team: ${BRIDGEVM_RELEASE_TEAM_ID:-$(identity_team_id "$IDENTITY")}"
echo "Notary profile: $NOTARY_PROFILE"
echo "Bundle identifier: $BRIDGEVM_BUNDLE_IDENTIFIER"
echo "Bundle short version: $BRIDGEVM_BUNDLE_SHORT_VERSION"
echo "Bundle version: $BRIDGEVM_BUNDLE_VERSION"
echo "Icon: $BRIDGEVM_MACOS_ICON_FILE"
