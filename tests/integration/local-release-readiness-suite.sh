#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
APP="$ROOT/target/macos/BridgeVMApp.app"
DMG="$ROOT/target/macos/BridgeVM.dmg"
ARTIFACT_MANIFEST="$ROOT/target/macos/BridgeVM-artifacts.txt"
APP_ONLY_ARTIFACT_MANIFEST="$ROOT/target/macos/BridgeVM-app-artifacts.txt"
WITH_METADATA_SMOKES=0
WITH_GUI_LAUNCH=0
WITH_LOCALLY_USABLE_APP=0
APP_ONLY=0

usage() {
  cat >&2 <<'EOF'
usage: tests/integration/local-release-readiness-suite.sh [--with-metadata-smokes] [--locally-usable-app] [--app-only] [--with-gui-launch]

Runs the local release-readiness lane for the current BridgeVM workspace.

Default checks:
  - Rust formatting
  - Rust workspace tests with default features disabled
  - macOS Swift package tests
  - local debug BridgeVMApp.app bundle build, signature verification, and bundled AppleVzRunner verification
  - credential-free clean debug app rebuild smoke
  - local debug BridgeVM.dmg image build and mounted-content verification
  - credential-free bundle metadata override smoke
  - credential-free bundled AppleVzRunner helper verification smoke
  - credential-free bundled bridgevmd supervisor/helper environment smoke
  - credential-free release credential preflight smoke
  - credential-free release-candidate command dry-run smoke
  - credential-free Apple VZ live opt-in default-skip smoke
  - credential-free Apple VZ preserved live-evidence verifier smoke
  - credential-free QEMU preserved live-evidence verifier smoke
  - credential-free release verifier custom app bundle smoke
  - credential-free AppleVzRunner artifact manifest smoke
  - local artifact manifest with hashes, helper metadata, and signing/notarization diagnostics
  - public-release gate boundary check for local debug artifacts

Optional checks:
  --with-metadata-smokes   run the metadata-safe smoke suite
  --locally-usable-app     require LaunchServices-free GUI app startup to show
                           a main window, supervise the bundled bridgevmd child,
                           and answer socket doctor; does not mount or launch
                           the DMG
  --app-only               skip DMG build, mounted-DMG verifier, and DMG launch
                           gates for faster local app usability checks; writes
                           an app-only artifact manifest instead
  --with-gui-launch        launch the generated .app through LaunchServices and
                           verify that app, DMG, and quarantined DMG launch
                           paths show a main window
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --with-metadata-smokes)
      WITH_METADATA_SMOKES=1
      shift
      ;;
    --locally-usable-app)
      WITH_LOCALLY_USABLE_APP=1
      shift
      ;;
    --app-only)
      APP_ONLY=1
      shift
      ;;
    --with-gui-launch)
      WITH_GUI_LAUNCH=1
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

if [[ "$APP_ONLY" == "1" && "$WITH_GUI_LAUNCH" == "1" ]]; then
  echo "--app-only conflicts with --with-gui-launch because mounted and quarantined DMG launch checks require a DMG." >&2
  exit 2
fi

run() {
  echo "==> $*"
  "$@"
}

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

assert_contains_file() {
  local file="$1"
  local needle="$2"
  local label="$3"
  grep -Fq "$needle" "$file" || fail "$label missing expected text: $needle"
}

assert_not_contains_file() {
  local file="$1"
  local needle="$2"
  local label="$3"
  if grep -Fq "$needle" "$file"; then
    fail "$label included unexpected text: $needle"
  fi
}

verify_app_only_artifact_manifest() {
  local manifest="$1"

  [[ -f "$manifest" ]] || fail "app-only artifact manifest was not written: $manifest"
  assert_contains_file "$manifest" "mode=app-only" "app-only artifact manifest"
  assert_contains_file "$manifest" "app.path=$APP" "app-only app metadata"
  assert_contains_file "$manifest" "app_executable.present=true" "app-only executable metadata"
  assert_contains_file "$manifest" "app_executable.executable=true" "app-only executable metadata"
  assert_contains_file "$manifest" "apple_vz_runner.present=true" "app-only AppleVzRunner metadata"
  assert_contains_file "$manifest" "apple_vz_runner.executable=true" "app-only AppleVzRunner metadata"
  assert_contains_file "$manifest" "bridgevmd.present=true" "app-only bridgevmd metadata"
  assert_contains_file "$manifest" "bridgevmd.executable=true" "app-only bridgevmd metadata"
  assert_contains_file "$manifest" "lightvm_runner.present=true" "app-only lightvm-runner metadata"
  assert_contains_file "$manifest" "lightvm_runner.executable=true" "app-only lightvm-runner metadata"
  assert_contains_file "$manifest" "app_codesign_verify.exit=0" "app-only app signature verification"
  assert_contains_file "$manifest" "apple_vz_runner_entitlements.exit=0" "app-only AppleVzRunner entitlement recording"
  assert_contains_file "$manifest" "com.apple.security.virtualization" "app-only AppleVzRunner virtualization entitlement"
  assert_not_contains_file "$manifest" "dmg.path=" "app-only DMG metadata"
  assert_not_contains_file "$manifest" "dmg_hdiutil_verify.exit=" "app-only DMG verification recording"
  assert_not_contains_file "$manifest" "dmg_notary_submit_json" "app-only DMG notary metadata"
}

find_bridgevm_window() {
  local pid="$1"
  swift -e 'import CoreGraphics
let pid = Int32(CommandLine.arguments[1])!
let opts = CGWindowListOption(arrayLiteral: .optionAll)
let list = CGWindowListCopyWindowInfo(opts, kCGNullWindowID) as? [[String: Any]] ?? []
for window in list {
    guard
        let ownerPid = window[kCGWindowOwnerPID as String] as? Int32,
        ownerPid == pid,
        let layer = window[kCGWindowLayer as String] as? Int,
        layer == 0,
        let bounds = window[kCGWindowBounds as String] as? [String: Any],
        let width = bounds["Width"] as? Double,
        let height = bounds["Height"] as? Double,
        width >= 800,
        height >= 500
    else {
        continue
    }
    print("pid=\(pid) window=\(window[kCGWindowNumber as String] ?? "?") width=\(Int(width)) height=\(Int(height)) owner=\(window[kCGWindowOwnerName as String] ?? "?") name=\(window[kCGWindowName as String] ?? "")")
    exit(0)
}
exit(1)' "$pid"
}

process_ids_under_path() {
  local path="$1"
  ps -axo pid=,command= | awk -v path="$path" '
    function normalize(value) {
      gsub(/\/+/, "/", value)
      sub(/^\/private\/var\//, "/var/", value)
      return value
    }
    BEGIN { path = normalize(path) }
    {
      pid = $1
      sub(/^[[:space:]]*[0-9]+[[:space:]]+/, "", $0)
      command = normalize($0)
      if (index(command, path) == 1) {
        print pid
      }
    }
  '
}

stop_process() {
  local pid="$1"
  [[ -n "$pid" ]] || return 0
  kill "$pid" 2>/dev/null || true
  for _ in {1..50}; do
    if ! kill -0 "$pid" 2>/dev/null; then
      wait "$pid" 2>/dev/null || true
      return 0
    fi
    sleep 0.05
  done
  kill -9 "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
}

stop_processes_under_path() {
  local path="$1"
  local pid
  for pid in $(process_ids_under_path "$path" || true); do
    stop_process "$pid"
  done
}

cleanup_gui_pid=""
cleanup() {
  if [[ -n "$cleanup_gui_pid" ]]; then
    stop_process "$cleanup_gui_pid"
  fi
  stop_processes_under_path "$APP/Contents/Helpers"
}
trap cleanup EXIT

cd "$ROOT"

run cargo fmt --check
run cargo test --workspace --no-default-features
run swift test --package-path "$ROOT/apps/macos" --jobs 1
run "$ROOT/packaging/macos/build-debug-app-bundle.sh"
run "$ROOT/packaging/macos/build-debug-app-bundle.sh" --verify-only "$APP"
run "$ROOT/apps/macos/scripts/build-sign-apple-vz-runner.sh" --verify-only "$APP/Contents/Helpers/AppleVzRunner"
run "$ROOT/tests/integration/macos-debug-app-clean-build-smoke.sh"
run "$ROOT/tests/integration/macos-app-name-validation-smoke.sh"
run "$ROOT/tests/integration/macos-metadata-overrides-smoke.sh"
run "$ROOT/tests/integration/macos-bundle-helper-verify-smoke.sh"
if [[ "$WITH_LOCALLY_USABLE_APP" == "1" ]]; then
  run env \
    BRIDGEVM_MACOS_BUNDLED_DAEMON_REQUIRE_GUI=1 \
    BRIDGEVM_MACOS_BUNDLED_DAEMON_REQUIRE_WINDOW=1 \
    "$ROOT/tests/integration/macos-bundled-daemon-supervisor-smoke.sh"
else
run "$ROOT/tests/integration/macos-bundled-daemon-supervisor-smoke.sh"
fi
run "$ROOT/tests/integration/macos-release-credentials-preflight-smoke.sh"
run "$ROOT/tests/integration/macos-release-candidate-dry-run-smoke.sh"
run "$ROOT/tests/integration/apple-vz-live-opt-in-skip-smoke.sh"
run "$ROOT/tests/integration/apple-vz-live-evidence-verifier-smoke.sh"
run "$ROOT/tests/integration/qemu-live-evidence-verifier-smoke.sh"
run "$ROOT/tests/integration/macos-artifact-manifest-apple-vz-runner-smoke.sh"
if [[ "$APP_ONLY" == "0" ]]; then
  run "$ROOT/tests/integration/macos-release-verifier-custom-app-smoke.sh"
  run "$ROOT/tests/integration/macos-debug-dmg-custom-app-name-smoke.sh"
  run "$ROOT/packaging/macos/build-debug-dmg.sh"
  run "$ROOT/packaging/macos/build-debug-dmg.sh" --verify-only "$DMG"
  run "$ROOT/packaging/macos/write-artifact-manifest.sh" "$APP" "$DMG" "$ARTIFACT_MANIFEST"
  run "$ROOT/packaging/macos/verify-release-candidate.sh" --expect-debug-boundary "$APP" "$DMG"
else
  run "$ROOT/packaging/macos/write-artifact-manifest.sh" --app-only "$APP" "$APP_ONLY_ARTIFACT_MANIFEST"
  verify_app_only_artifact_manifest "$APP_ONLY_ARTIFACT_MANIFEST"
  echo "==> Skipping DMG build, mounted-DMG verifier, and DMG launch gates (--app-only)"
fi

if [[ "$WITH_METADATA_SMOKES" == "1" ]]; then
  run "$ROOT/tests/integration/metadata-safe-smoke-suite.sh"
fi

if [[ "$WITH_GUI_LAUNCH" == "1" ]]; then
  existing_gui_pids="$(pgrep -f "$APP/Contents/MacOS/BridgeVMApp" || true)"
  run open -n "$APP"
  for _ in {1..100}; do
    cleanup_gui_pid=""
    for candidate_pid in $(pgrep -f "$APP/Contents/MacOS/BridgeVMApp" || true); do
      case "$(printf '\n%s\n' "$existing_gui_pids")" in
        *$'\n'"$candidate_pid"$'\n'*) continue ;;
      esac
      if find_bridgevm_window "$candidate_pid"; then
        cleanup_gui_pid="$candidate_pid"
        break
      fi
    done
    [[ -z "$cleanup_gui_pid" ]] || break
    sleep 0.1
  done
  [[ -n "$cleanup_gui_pid" ]] || {
    echo "BridgeVMApp did not start from bundle: $APP" >&2
    exit 1
  }
  find_bridgevm_window "$cleanup_gui_pid" >/dev/null || {
    echo "BridgeVMApp started but no main window was detected" >&2
    exit 1
  }
  run "$ROOT/packaging/macos/verify-release-candidate.sh" \
    --expect-debug-boundary \
    --launch-smoke \
    "$APP" \
    "$DMG"
  run "$ROOT/packaging/macos/verify-release-candidate.sh" \
    --expect-debug-boundary \
    --quarantine-smoke \
    "$APP" \
    "$DMG"
  kill "$cleanup_gui_pid" 2>/dev/null || true
  wait "$cleanup_gui_pid" 2>/dev/null || true
  cleanup_gui_pid=""
fi

echo "PASS: local release-readiness suite"
