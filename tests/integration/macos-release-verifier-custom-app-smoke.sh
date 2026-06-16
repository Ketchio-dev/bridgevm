#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-release-verifier-custom-app.XXXXXX")"
OUT_DIR="$WORKDIR/out"
APP="$OUT_DIR/BridgeVM.app"
DMG="$OUT_DIR/BridgeVM-custom.dmg"
BAD_DMG="$OUT_DIR/BridgeVM-missing-applications-link.dmg"
BAD_VOLUME_DMG="$OUT_DIR/BridgeVM-wrong-volume.dmg"
BAD_EXTRA_APP_DMG="$OUT_DIR/BridgeVM-extra-app.dmg"
STAGE="$WORKDIR/stage"
BAD_STAGE="$WORKDIR/bad-stage"
BAD_VOLUME_STAGE="$WORKDIR/bad-volume-stage"
BAD_EXTRA_APP_STAGE="$WORKDIR/bad-extra-app-stage"
FAKE_BIN="$WORKDIR/fake-bin"

cleanup() {
  rm -rf "$WORKDIR"
}
trap cleanup EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

assert_dmg_detached() {
  local dmg="$1"
  local dmg_real
  dmg_real="$(cd "$(dirname "$dmg")" && pwd -P)/$(basename "$dmg")"

  if hdiutil info | grep -F "$dmg" >/dev/null || hdiutil info | grep -F "$dmg_real" >/dev/null; then
    fail "DMG remained attached after verifier cleanup: $dmg"
  fi
}

mkdir -p "$OUT_DIR" "$STAGE" "$BAD_STAGE" "$BAD_VOLUME_STAGE" "$BAD_EXTRA_APP_STAGE" "$FAKE_BIN"
printf '#!/bin/sh\nexit 0\n' >"$FAKE_BIN/open"
chmod +x "$FAKE_BIN/open"

help_output="$("$ROOT/packaging/macos/verify-release-candidate.sh" --help 2>&1)"
case "$help_output" in
  *"--launch-smoke"*) ;;
  *) fail "verifier help did not document --launch-smoke: $help_output" ;;
esac
case "$help_output" in
  *"--quarantine-smoke"*) ;;
  *) fail "verifier help did not document --quarantine-smoke: $help_output" ;;
esac
case "$help_output" in
  *"BRIDGEVM_MACOS_LAUNCH_SMOKE"*"set to 1 to enable the optional LaunchServices smoke"*) ;;
  *) fail "verifier help did not document launch smoke opt-in behavior: $help_output" ;;
esac
case "$help_output" in
  *"BRIDGEVM_MACOS_QUARANTINE_SMOKE"*"set to 1 to enable the optional quarantined DMG smoke"*) ;;
  *) fail "verifier help did not document quarantined smoke opt-in behavior: $help_output" ;;
esac
case "$help_output" in
  *"BRIDGEVM_MACOS_OPEN_TOOL"*"defaults to open"*) ;;
  *) fail "verifier help did not document launch smoke opener override: $help_output" ;;
esac
for cleanup_script in \
  "$ROOT/packaging/macos/verify-release-candidate.sh" \
  "$ROOT/packaging/macos/build-debug-dmg.sh" \
  "$ROOT/tests/integration/macos-debug-dmg-custom-app-name-smoke.sh"; do
  if grep -F '(hdiutil detach "$target"' "$cleanup_script" >/dev/null; then
    fail "bounded detach watchdog backgrounds a shell instead of hdiutil directly: $cleanup_script"
  fi
done

env \
  BRIDGEVM_MACOS_BUNDLE_DIR="$OUT_DIR" \
  BRIDGEVM_MACOS_APP_NAME="BridgeVM" \
  BRIDGEVM_BUNDLE_IDENTIFIER="dev.bridgevm.release-verifier-smoke" \
  "$ROOT/packaging/macos/build-debug-app-bundle.sh" >/dev/null

[[ -d "$APP" ]] || fail "expected custom app bundle was not built: $APP"

ditto "$APP" "$STAGE/BridgeVM.app"
ln -s /Applications "$STAGE/Applications"
hdiutil create \
  -volname BridgeVM \
  -srcfolder "$STAGE" \
  -ov \
  -format UDZO \
  "$DMG" >/dev/null

ditto "$APP" "$BAD_STAGE/BridgeVM.app"
hdiutil create \
  -volname BridgeVM \
  -srcfolder "$BAD_STAGE" \
  -ov \
  -format UDZO \
  "$BAD_DMG" >/dev/null

ditto "$APP" "$BAD_VOLUME_STAGE/BridgeVM.app"
ln -s /Applications "$BAD_VOLUME_STAGE/Applications"
hdiutil create \
  -volname WrongBridgeVM \
  -srcfolder "$BAD_VOLUME_STAGE" \
  -ov \
  -format UDZO \
  "$BAD_VOLUME_DMG" >/dev/null

ditto "$APP" "$BAD_EXTRA_APP_STAGE/BridgeVM.app"
ditto "$APP" "$BAD_EXTRA_APP_STAGE/BridgeVMExtra.app"
ln -s /Applications "$BAD_EXTRA_APP_STAGE/Applications"
hdiutil create \
  -volname BridgeVM \
  -srcfolder "$BAD_EXTRA_APP_STAGE" \
  -ov \
  -format UDZO \
  "$BAD_EXTRA_APP_DMG" >/dev/null

set +e
bad_verifier_output="$("$ROOT/packaging/macos/verify-release-candidate.sh" \
  --expect-debug-boundary \
  "$APP" \
  "$BAD_DMG" 2>&1)"
bad_verifier_status=$?
set -e
[[ "$bad_verifier_status" -ne 0 ]] || fail "verifier accepted a DMG without Applications symlink"
case "$bad_verifier_output" in
  *"BridgeVM DMG is missing Applications symlink"*) ;;
  *) fail "verifier did not report missing Applications symlink: $bad_verifier_output" ;;
esac
assert_dmg_detached "$BAD_DMG"

set +e
bad_volume_output="$("$ROOT/packaging/macos/verify-release-candidate.sh" \
  --expect-debug-boundary \
  "$APP" \
  "$BAD_VOLUME_DMG" 2>&1)"
bad_volume_status=$?
set -e
[[ "$bad_volume_status" -ne 0 ]] || fail "verifier accepted a DMG with the wrong volume name"
case "$bad_volume_output" in
  *"BridgeVM DMG volume name mismatch: expected BridgeVM, got WrongBridgeVM"*) ;;
  *) fail "verifier did not report wrong volume name: $bad_volume_output" ;;
esac
assert_dmg_detached "$BAD_VOLUME_DMG"

set +e
bad_extra_app_output="$("$ROOT/packaging/macos/verify-release-candidate.sh" \
  --expect-debug-boundary \
  "$APP" \
  "$BAD_EXTRA_APP_DMG" 2>&1)"
bad_extra_app_status=$?
set -e
[[ "$bad_extra_app_status" -ne 0 ]] || fail "verifier accepted a DMG with an extra top-level app bundle"
case "$bad_extra_app_output" in
  *"BridgeVM DMG must contain exactly one top-level app bundle, found 2"*) ;;
  *) fail "verifier did not report extra app bundle: $bad_extra_app_output" ;;
esac
assert_dmg_detached "$BAD_EXTRA_APP_DMG"

verifier_output="$("$ROOT/packaging/macos/verify-release-candidate.sh" \
  --expect-debug-boundary \
  "$APP" \
  "$DMG")"

case "$verifier_output" in
  *"BridgeVM DMG Developer ID signature"*) ;;
  *) fail "verifier output did not include DMG Developer ID gate" ;;
esac
case "$verifier_output" in
  *"Mounted app LaunchServices smoke"*)
    fail "verifier ran launch smoke without opt-in: $verifier_output"
    ;;
esac
case "$verifier_output" in
  *"Quarantined DMG LaunchServices smoke"*)
    fail "verifier ran quarantined launch smoke without opt-in: $verifier_output"
    ;;
esac

set +e
launch_smoke_output="$(env \
  BRIDGEVM_MACOS_LAUNCH_SMOKE=1 \
  BRIDGEVM_MACOS_LAUNCH_SMOKE_TIMEOUT_TENTHS=1 \
  BRIDGEVM_MACOS_OPEN_TOOL="$FAKE_BIN/open" \
  "$ROOT/packaging/macos/verify-release-candidate.sh" \
  --expect-debug-boundary \
  "$APP" \
  "$DMG" 2>&1)"
launch_smoke_status=$?
set -e
[[ "$launch_smoke_status" -ne 0 ]] || fail "verifier swallowed launch smoke failure under debug boundary: $launch_smoke_output"
case "$launch_smoke_output" in
  *"Mounted app LaunchServices smoke"*"BridgeVM macOS artifacts failed 1 launch smoke gate(s)."*) ;;
  *) fail "verifier did not report launch smoke failure under debug boundary: $launch_smoke_output" ;;
esac

set +e
quarantine_smoke_output="$(env \
  BRIDGEVM_MACOS_QUARANTINE_SMOKE=1 \
  "$ROOT/packaging/macos/verify-release-candidate.sh" \
  --expect-debug-boundary \
  "$APP" \
  "$DMG" 2>&1)"
quarantine_smoke_status=$?
set -e
if [[ "$quarantine_smoke_status" -eq 0 ]]; then
  case "$quarantine_smoke_output" in
    *"PASS: Quarantined DMG propagated quarantine to mounted app"*"PASS: Quarantined DMG Gatekeeper debug boundary"*"PASS: debug artifacts are structurally valid but not public release candidates"*) ;;
    *) fail "verifier did not report quarantined Gatekeeper debug boundary: $quarantine_smoke_output" ;;
  esac
else
  case "$quarantine_smoke_output" in
    *"BridgeVM quarantined DMG did not propagate quarantine to mounted app"*) ;;
    *) fail "verifier failed quarantined smoke without the expected propagation diagnostic: $quarantine_smoke_output" ;;
  esac
fi

echo "PASS: macOS release verifier custom app smoke"
