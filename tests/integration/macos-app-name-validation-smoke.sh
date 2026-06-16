#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-app-name-validation.XXXXXX")"
OUT_DIR="$WORKDIR/out"
SENTINEL="$WORKDIR/sentinel.app"

cleanup() {
  rm -rf "$WORKDIR"
}
trap cleanup EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

assert_invalid_app_name() {
  local label="$1"
  local script="$2"
  local app_name="$3"
  shift 3
  local output
  local status

  mkdir -p "$OUT_DIR" "$SENTINEL"
  printf 'keep\n' >"$SENTINEL/KEEP"

  set +e
  output="$(
    env \
      BRIDGEVM_MACOS_BUNDLE_DIR="$OUT_DIR" \
      BRIDGEVM_MACOS_RELEASE_DIR="$OUT_DIR" \
      BRIDGEVM_MACOS_APP_NAME="$app_name" \
      BRIDGEVM_RELEASE_CODESIGN_IDENTITY="Developer ID Application: Example Corp (TEAMID)" \
      BRIDGEVM_NOTARYTOOL_KEYCHAIN_PROFILE="bridgevm-notary-profile" \
      BRIDGEVM_BUNDLE_IDENTIFIER="com.example.bridgevm" \
      BRIDGEVM_BUNDLE_SHORT_VERSION="1.0.0" \
      BRIDGEVM_BUNDLE_VERSION="100" \
      BRIDGEVM_MACOS_ICON_FILE="/tmp/BridgeVM.icns" \
      "$script" "$@" 2>&1
  )"
  status=$?
  set -e

  [[ "$status" -eq 2 ]] || fail "$label exited $status instead of 2: $output"
  [[ "$output" == *"BRIDGEVM_MACOS_APP_NAME"* ]] || {
    fail "$label did not explain the invalid app-name env var: $output"
  }
  [[ -f "$SENTINEL/KEEP" ]] || fail "$label removed an outside sentinel"
}

assert_invalid_app_name \
  "debug app path traversal" \
  "$ROOT/packaging/macos/build-debug-app-bundle.sh" \
  "../sentinel"

assert_invalid_app_name \
  "debug dmg app path traversal" \
  "$ROOT/packaging/macos/build-debug-dmg.sh" \
  "../sentinel"

assert_invalid_app_name \
  "release candidate path traversal" \
  "$ROOT/packaging/macos/build-release-candidate.sh" \
  "../sentinel" \
  --dry-run

assert_invalid_app_name \
  "debug app suffix" \
  "$ROOT/packaging/macos/build-debug-app-bundle.sh" \
  "BridgeVM.app"

assert_invalid_app_name \
  "release blank app name" \
  "$ROOT/packaging/macos/build-release-candidate.sh" \
  "   " \
  --dry-run

echo "PASS: macOS app name validation smoke"
