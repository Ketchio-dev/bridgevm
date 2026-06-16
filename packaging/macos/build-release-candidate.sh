#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
. "$ROOT/packaging/macos/app-name.sh"
OUT_DIR="${BRIDGEVM_MACOS_RELEASE_DIR:-$ROOT/target/macos-release}"
APP_NAME="${BRIDGEVM_MACOS_APP_NAME:-BridgeVM}"
bridgevm_validate_macos_app_name "$APP_NAME" BRIDGEVM_MACOS_APP_NAME || exit 2
APP="$OUT_DIR/$APP_NAME.app"
APPLE_VZ_RUNNER="$APP/Contents/Helpers/AppleVzRunner"
DMG="${BRIDGEVM_MACOS_DMG:-$OUT_DIR/BridgeVM.dmg}"
MANIFEST="${BRIDGEVM_MACOS_ARTIFACT_MANIFEST:-$OUT_DIR/BridgeVM-artifacts.txt}"
APP_NOTARY_ZIP="${BRIDGEVM_MACOS_APP_NOTARY_ZIP:-$OUT_DIR/$APP_NAME-notary.zip}"
APP_NOTARY_SUBMIT_JSON="${BRIDGEVM_MACOS_APP_NOTARY_SUBMIT_JSON:-$OUT_DIR/$APP_NAME-notary-submit.json}"
APP_NOTARY_LOG_JSON="${BRIDGEVM_MACOS_APP_NOTARY_LOG_JSON:-$OUT_DIR/$APP_NAME-notary-log.json}"
DMG_NOTARY_SUBMIT_JSON="${BRIDGEVM_MACOS_DMG_NOTARY_SUBMIT_JSON:-$OUT_DIR/BridgeVM-dmg-notary-submit.json}"
DMG_NOTARY_LOG_JSON="${BRIDGEVM_MACOS_DMG_NOTARY_LOG_JSON:-$OUT_DIR/BridgeVM-dmg-notary-log.json}"
VOLUME_NAME="${BRIDGEVM_MACOS_DMG_VOLUME:-BridgeVM}"
IDENTITY="${BRIDGEVM_RELEASE_CODESIGN_IDENTITY:-${BRIDGEVM_CODESIGN_IDENTITY:-}}"
NOTARY_PROFILE="${BRIDGEVM_NOTARYTOOL_KEYCHAIN_PROFILE:-${BRIDGEVM_NOTARYTOOL_PROFILE:-}}"
DRY_RUN=0

usage() {
  cat >&2 <<'EOF'
usage: packaging/macos/build-release-candidate.sh [--dry-run]

Builds a macOS release-candidate app/DMG using externally supplied Developer
ID signing and notarization inputs. Use --dry-run to print the credentialed
commands without running them.

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
  BRIDGEVM_MACOS_RELEASE_DIR          output directory, defaults to target/macos-release
  BRIDGEVM_MACOS_APP_NAME             .app basename, defaults to BridgeVM
  BRIDGEVM_MACOS_DMG                  output DMG path, defaults to release dir/BridgeVM.dmg
  BRIDGEVM_MACOS_APP_NOTARY_ZIP       app notarization zip path, defaults to
                                      release dir/<app>-notary.zip
  BRIDGEVM_MACOS_APP_NOTARY_SUBMIT_JSON
                                      app notarytool submit JSON path
  BRIDGEVM_MACOS_APP_NOTARY_LOG_JSON  app notarytool log JSON path
  BRIDGEVM_MACOS_DMG_NOTARY_SUBMIT_JSON
                                      DMG notarytool submit JSON path
  BRIDGEVM_MACOS_DMG_NOTARY_LOG_JSON  DMG notarytool log JSON path
  BRIDGEVM_MACOS_DMG_VOLUME           mounted volume name, defaults to BridgeVM
  BRIDGEVM_RELEASE_TEAM_ID            optional Developer ID team identifier
                                      passed to final release verification
  BRIDGEVM_BUNDLE_DISPLAY_NAME        display name, defaults to BridgeVM
  BRIDGEVM_BUNDLE_NAME                bundle name, defaults to BridgeVM
  BRIDGEVM_BUNDLE_COPYRIGHT           optional copyright string
  BRIDGEVM_APPLE_VZ_ENTITLEMENTS      AppleVzRunner release entitlements path,
                                      defaults to apps/macos/AppleVzRunner.release.entitlements
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

require_env() {
  local name="$1"
  local value="${!name:-}"
  [[ -n "$value" ]] || {
    echo "Missing required release input: $name" >&2
    exit 2
  }
}

is_blank() {
  local value="$1"
  [[ -z "${value//[[:space:]]/}" ]]
}

quote_args() {
  local arg
  for arg in "$@"; do
    printf ' %q' "$arg"
  done
  printf '\n'
}

run() {
  if [[ "$DRY_RUN" == "1" ]]; then
    printf '+'
    quote_args "$@"
  else
    echo "==> $*"
    "$@"
  fi
}

notarize_and_record() {
  local artifact="$1"
  local submit_json="$2"
  local log_json="$3"
  local submission_id
  local submission_status

  if [[ "$DRY_RUN" == "1" ]]; then
    printf '+ xcrun notarytool submit %q --keychain-profile %q --wait --output-format json > %q\n' \
      "$artifact" "$NOTARY_PROFILE" "$submit_json"
    printf '+ xcrun notarytool log <submission-id-from-%q> --keychain-profile %q --output-format json > %q\n' \
      "$submit_json" "$NOTARY_PROFILE" "$log_json"
    printf '+ verify notarytool status Accepted in %q\n' "$submit_json"
    return 0
  fi

  mkdir -p "$(dirname "$submit_json")" "$(dirname "$log_json")"
  echo "==> xcrun notarytool submit $artifact --keychain-profile $NOTARY_PROFILE --wait --output-format json"
  xcrun notarytool submit "$artifact" \
    --keychain-profile "$NOTARY_PROFILE" \
    --wait \
    --output-format json | tee "$submit_json"
  submission_id="$(/usr/bin/plutil -extract id raw -o - "$submit_json" 2>/dev/null || true)"
  [[ -n "$submission_id" ]] || {
    echo "notarytool submit output did not include a submission id: $submit_json" >&2
    exit 1
  }
  echo "==> xcrun notarytool log $submission_id --keychain-profile $NOTARY_PROFILE --output-format json"
  xcrun notarytool log "$submission_id" \
    --keychain-profile "$NOTARY_PROFILE" \
    --output-format json >"$log_json"
  submission_status="$(/usr/bin/plutil -extract status raw -o - "$submit_json" 2>/dev/null || true)"
  if [[ "$submission_status" != "Accepted" ]]; then
    echo "notarytool submission was not accepted for $artifact: ${submission_status:-<missing status>}" >&2
    echo "notarytool submit JSON: $submit_json" >&2
    echo "notarytool log JSON: $log_json" >&2
    exit 1
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
  if [[ -z "${BRIDGEVM_RELEASE_TEAM_ID:-}" && ! "$IDENTITY" =~ \([[:alnum:]]+\)$ ]]; then
    echo "BRIDGEVM_RELEASE_CODESIGN_IDENTITY must end with a parenthesized Apple team identifier, for example: Developer ID Application: Example Corp (TEAMID)" >&2
    echo "Got: $IDENTITY" >&2
    exit 2
  fi
}

validate_notary_profile() {
  if is_blank "$NOTARY_PROFILE"; then
    echo "Missing required release input: BRIDGEVM_NOTARYTOOL_KEYCHAIN_PROFILE" >&2
    exit 2
  fi
}

validate_release_team_id() {
  local expected_team_id="${BRIDGEVM_RELEASE_TEAM_ID:-}"
  local identity_team_id
  [[ -n "$expected_team_id" ]] || return 0
  if [[ "$IDENTITY" =~ \(([[:alnum:]]+)\)$ ]]; then
    identity_team_id="${BASH_REMATCH[1]}"
    if [[ "$identity_team_id" != "$expected_team_id" ]]; then
      echo "BRIDGEVM_RELEASE_TEAM_ID does not match signing identity team: expected $expected_team_id, identity has $identity_team_id" >&2
      exit 2
    fi
  else
    echo "BRIDGEVM_RELEASE_TEAM_ID requires a signing identity ending with a parenthesized team identifier: $IDENTITY" >&2
    exit 2
  fi
}

require_env BRIDGEVM_BUNDLE_IDENTIFIER
require_env BRIDGEVM_BUNDLE_SHORT_VERSION
require_env BRIDGEVM_BUNDLE_VERSION
require_env BRIDGEVM_MACOS_ICON_FILE
validate_release_identity
validate_notary_profile
[[ "$BRIDGEVM_MACOS_ICON_FILE" == *.icns ]] || {
  echo "BridgeVM release icon must use the .icns extension: $BRIDGEVM_MACOS_ICON_FILE" >&2
  exit 2
}
validate_release_team_id

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

run mkdir -p \
  "$OUT_DIR" \
  "$(dirname "$APP_NOTARY_ZIP")" \
  "$(dirname "$APP_NOTARY_SUBMIT_JSON")" \
  "$(dirname "$APP_NOTARY_LOG_JSON")" \
  "$(dirname "$DMG")" \
  "$(dirname "$DMG_NOTARY_SUBMIT_JSON")" \
  "$(dirname "$DMG_NOTARY_LOG_JSON")" \
  "$(dirname "$MANIFEST")"

if [[ "$DRY_RUN" == "1" ]]; then
  printf '+'
  quote_args "$ROOT/packaging/macos/preflight-release-credentials.sh" --dry-run
  "$ROOT/packaging/macos/preflight-release-credentials.sh" --dry-run
else
  run "$ROOT/packaging/macos/preflight-release-credentials.sh"
fi

run env \
  "BRIDGEVM_MACOS_BUNDLE_DIR=$OUT_DIR" \
  "BRIDGEVM_MACOS_APP_NAME=$APP_NAME" \
  "BRIDGEVM_MACOS_BUILD_CONFIGURATION=release" \
  "BRIDGEVM_CODESIGN_IDENTITY=$IDENTITY" \
  "BRIDGEVM_MACOS_ICON_FILE=$BRIDGEVM_MACOS_ICON_FILE" \
  "BRIDGEVM_BUNDLE_DISPLAY_NAME=${BRIDGEVM_BUNDLE_DISPLAY_NAME:-BridgeVM}" \
  "BRIDGEVM_BUNDLE_NAME=${BRIDGEVM_BUNDLE_NAME:-BridgeVM}" \
  "BRIDGEVM_BUNDLE_IDENTIFIER=$BRIDGEVM_BUNDLE_IDENTIFIER" \
  "BRIDGEVM_BUNDLE_SHORT_VERSION=$BRIDGEVM_BUNDLE_SHORT_VERSION" \
  "BRIDGEVM_BUNDLE_VERSION=$BRIDGEVM_BUNDLE_VERSION" \
  "BRIDGEVM_BUNDLE_COPYRIGHT=${BRIDGEVM_BUNDLE_COPYRIGHT:-}" \
  "$ROOT/packaging/macos/build-debug-app-bundle.sh"

run env \
  "BRIDGEVM_CODESIGN_IDENTITY=$IDENTITY" \
  "BRIDGEVM_APPLE_VZ_ENTITLEMENTS=${BRIDGEVM_APPLE_VZ_ENTITLEMENTS:-$ROOT/apps/macos/AppleVzRunner.release.entitlements}" \
  "$ROOT/apps/macos/scripts/build-sign-apple-vz-runner.sh" \
  --release \
  --output "$APPLE_VZ_RUNNER"

run "$ROOT/apps/macos/scripts/build-sign-apple-vz-runner.sh" --verify-only "$APPLE_VZ_RUNNER"
run codesign --force --options runtime --sign "$IDENTITY" "$APP/Contents/Helpers/bridgevmd"
run codesign --force --options runtime --sign "$IDENTITY" "$APP/Contents/Helpers/lightvm-runner"
run codesign --force --options runtime --sign "$IDENTITY" "$APP"
run "$ROOT/packaging/macos/build-debug-app-bundle.sh" --verify-only "$APP"
run ditto -c -k --keepParent "$APP" "$APP_NOTARY_ZIP"
notarize_and_record "$APP_NOTARY_ZIP" "$APP_NOTARY_SUBMIT_JSON" "$APP_NOTARY_LOG_JSON"
run xcrun stapler staple "$APP"
run xcrun stapler validate "$APP"

STAGE="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-release-dmg-stage.XXXXXX")"
cleanup_stage() {
  rm -rf "$STAGE"
}
trap cleanup_stage EXIT

run ditto "$APP" "$STAGE/$APP_NAME.app"
run ln -s /Applications "$STAGE/Applications"
run hdiutil create -volname "$VOLUME_NAME" -srcfolder "$STAGE" -ov -format UDZO "$DMG"
run hdiutil verify "$DMG"
run codesign --force --sign "$IDENTITY" "$DMG"
notarize_and_record "$DMG" "$DMG_NOTARY_SUBMIT_JSON" "$DMG_NOTARY_LOG_JSON"
run xcrun stapler staple "$DMG"
run xcrun stapler validate "$DMG"
run env \
  "BRIDGEVM_MACOS_APP_NOTARY_SUBMIT_JSON=$APP_NOTARY_SUBMIT_JSON" \
  "BRIDGEVM_MACOS_APP_NOTARY_LOG_JSON=$APP_NOTARY_LOG_JSON" \
  "BRIDGEVM_MACOS_DMG_NOTARY_SUBMIT_JSON=$DMG_NOTARY_SUBMIT_JSON" \
  "BRIDGEVM_MACOS_DMG_NOTARY_LOG_JSON=$DMG_NOTARY_LOG_JSON" \
  "$ROOT/packaging/macos/write-artifact-manifest.sh" "$APP" "$DMG" "$MANIFEST"
run env \
  "BRIDGEVM_MACOS_DMG_VOLUME=$VOLUME_NAME" \
  "BRIDGEVM_RELEASE_TEAM_ID=${BRIDGEVM_RELEASE_TEAM_ID:-}" \
  "$ROOT/packaging/macos/verify-release-candidate.sh" "$APP" "$DMG"

printf '%s\n' "$DMG"
