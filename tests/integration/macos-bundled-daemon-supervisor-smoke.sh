#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILD_SCRIPT="$ROOT/packaging/macos/build-debug-app-bundle.sh"
HOST_PATH="$PATH"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "SKIP: macOS bundled daemon supervisor smoke requires Darwin"
  exit 0
fi

WORKDIR="$(mktemp -d "/tmp/bvmd-supervisor.XXXXXX")"
OUT_DIR="$WORKDIR/out"
STORE="$WORKDIR/store"
HOME_DIR="$WORKDIR/home"
TMP_DIR="$WORKDIR/tmp"
BUNDLE_ID="dev.bridgevm.bundled-daemon-smoke.$$"
APP_NAME="BridgeVMBundledDaemonSmoke"
APP="$OUT_DIR/$APP_NAME.app"
APP_EXEC="$APP/Contents/MacOS/BridgeVMApp"
BRIDGEVMD="$APP/Contents/Helpers/bridgevmd"
LIGHTVM_RUNNER="$APP/Contents/Helpers/lightvm-runner"
APPLE_VZ_RUNNER="$APP/Contents/Helpers/AppleVzRunner"
SOCKET="$STORE/run/bridgevmd.sock"
APP_STDOUT="$WORKDIR/app.stdout"
APP_STDERR="$WORKDIR/app.stderr"
DAEMON_STDOUT="$WORKDIR/bridgevmd.stdout"
DAEMON_STDERR="$WORKDIR/bridgevmd.stderr"
APP_PID=""
DAEMON_PID=""
PRESERVE_WORKDIR=0
REQUIRE_GUI="${BRIDGEVM_MACOS_BUNDLED_DAEMON_REQUIRE_GUI:-0}"
REQUIRE_WINDOW="${BRIDGEVM_MACOS_BUNDLED_DAEMON_REQUIRE_WINDOW:-0}"

cleanup() {
  stop_processes
  if [[ "$PRESERVE_WORKDIR" != "1" ]]; then
    rm -rf "$WORKDIR"
  fi
}
trap cleanup EXIT

fail() {
  PRESERVE_WORKDIR=1
  echo "FAIL: $*" >&2
  echo "Workdir preserved at $WORKDIR" >&2
  [[ ! -f "$APP_STDERR" ]] || echo "App stderr: $APP_STDERR" >&2
  [[ ! -f "$DAEMON_STDERR" ]] || echo "Daemon stderr: $DAEMON_STDERR" >&2
  exit 1
}

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  case "$haystack" in
    *"$needle"*) ;;
    *) fail "$label missing '$needle'; got: $haystack" ;;
  esac
}

stop_one() {
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

stop_processes() {
  stop_one "$APP_PID"
  stop_one "$DAEMON_PID"
  APP_PID=""
  DAEMON_PID=""
}

wait_for_socket() {
  local owner_pid="$1"
  local label="$2"
  for _ in {1..150}; do
    [[ ! -S "$SOCKET" ]] || return 0
    if [[ -n "$owner_pid" ]] && ! kill -0 "$owner_pid" 2>/dev/null; then
      fail "$label exited before daemon socket became ready"
    fi
    sleep 0.1
  done
  fail "$label did not create daemon socket: $SOCKET"
}

wait_for_socket_or_return() {
  local owner_pid="$1"
  for _ in {1..150}; do
    [[ ! -S "$SOCKET" ]] || return 0
    if [[ -n "$owner_pid" ]] && ! kill -0 "$owner_pid" 2>/dev/null; then
      return 1
    fi
    sleep 0.1
  done
  return 1
}

find_child_bridgevmd() {
  local parent_pid="$1"
  ps -axo pid=,ppid=,command= | awk -v ppid="$parent_pid" -v helper="$BRIDGEVMD" '
    $2 == ppid && index($0, helper) {
      print $1
      exit
    }
  '
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

wait_for_window_or_return() {
  local owner_pid="$1"
  for _ in {1..100}; do
    find_bridgevm_window "$owner_pid" && return 0
    if [[ -n "$owner_pid" ]] && ! kill -0 "$owner_pid" 2>/dev/null; then
      return 1
    fi
    sleep 0.1
  done
  return 1
}

run_socket_doctor() {
  local label="$1"
  local output
  output="$(
    env \
      PATH="$HOST_PATH" \
      BRIDGEVM_HOME="$STORE" \
      cargo run --quiet -p bridgevm-cli -- --socket "$SOCKET" store doctor
  )"
  assert_contains "$output" "BridgeVM store: $STORE" "$label doctor output"
  assert_contains "$output" "VM bundles: $STORE/vms" "$label doctor output"
}

seed_isolated_defaults() {
  mkdir -p "$HOME_DIR" "$TMP_DIR"
  env \
    HOME="$HOME_DIR" \
    CFFIXED_USER_HOME="$HOME_DIR" \
    defaults write "$BUNDLE_ID" bridgevm.useMockInventory -bool false
  env \
    HOME="$HOME_DIR" \
    CFFIXED_USER_HOME="$HOME_DIR" \
    defaults write "$BUNDLE_ID" bridgevm.daemonSocketPath ""
  env \
    HOME="$HOME_DIR" \
    CFFIXED_USER_HOME="$HOME_DIR" \
    defaults write "$BUNDLE_ID" bridgevm.allowAppleVzRealStart -bool false
}

build_app() {
  env \
    BRIDGEVM_MACOS_BUNDLE_DIR="$OUT_DIR" \
    BRIDGEVM_MACOS_APP_NAME="$APP_NAME" \
    BRIDGEVM_BUNDLE_IDENTIFIER="$BUNDLE_ID" \
    BRIDGEVM_CODESIGN_IDENTITY=- \
    "$BUILD_SCRIPT" >/dev/null

  "$BUILD_SCRIPT" --verify-only "$APP" >/dev/null
  [[ -x "$APP_EXEC" ]] || fail "app executable missing: $APP_EXEC"
  [[ -x "$BRIDGEVMD" ]] || fail "bundled bridgevmd missing: $BRIDGEVMD"
  [[ -x "$LIGHTVM_RUNNER" ]] || fail "bundled lightvm-runner missing: $LIGHTVM_RUNNER"
  [[ -x "$APPLE_VZ_RUNNER" ]] || fail "bundled AppleVzRunner missing: $APPLE_VZ_RUNNER"
}

run_gui_supervisor_smoke() {
  seed_isolated_defaults
  env -i \
    PATH="/usr/bin:/bin:/usr/sbin:/sbin" \
    HOME="$HOME_DIR" \
    CFFIXED_USER_HOME="$HOME_DIR" \
    TMPDIR="$TMP_DIR/" \
    BRIDGEVM_HOME="$STORE" \
    "$APP_EXEC" >"$APP_STDOUT" 2>"$APP_STDERR" &
  APP_PID=$!

  wait_for_socket_or_return "$APP_PID" || return 1

  DAEMON_PID="$(find_child_bridgevmd "$APP_PID")"
  [[ -n "$DAEMON_PID" ]] || {
    echo "BridgeVMApp created a socket but no bundled bridgevmd child was found" >&2
    return 1
  }

  run_socket_doctor "gui supervisor"

  if [[ "$REQUIRE_WINDOW" == "1" ]]; then
    wait_for_window_or_return "$APP_PID" || {
      echo "BridgeVMApp started bundled bridgevmd but no main window was detected" >&2
      return 1
    }
  fi

  stop_processes
  echo "PASS: macOS bundled daemon supervisor GUI smoke"
}

run_helper_environment_smoke() {
  env -i \
    PATH="/usr/bin:/bin:/usr/sbin:/sbin" \
    HOME="$HOME_DIR" \
    TMPDIR="$TMP_DIR/" \
    BRIDGEVM_HOME="$STORE" \
    BRIDGEVM_LIGHTVM_RUNNER="$LIGHTVM_RUNNER" \
    BRIDGEVM_APPLE_VZ_RUNNER="$APPLE_VZ_RUNNER" \
    "$BRIDGEVMD" >"$DAEMON_STDOUT" 2>"$DAEMON_STDERR" &
  DAEMON_PID=$!

  wait_for_socket "$DAEMON_PID" "bundled bridgevmd"
  run_socket_doctor "helper environment"
  stop_processes
  echo "PASS: macOS bundled daemon helper environment smoke"
}

cd "$ROOT"
build_app

if [[ "${BRIDGEVM_MACOS_BUNDLED_DAEMON_FORCE_HELPER_ONLY:-0}" == "1" ]]; then
  if [[ "$REQUIRE_GUI" == "1" || "$REQUIRE_WINDOW" == "1" ]]; then
    fail "BRIDGEVM_MACOS_BUNDLED_DAEMON_FORCE_HELPER_ONLY conflicts with required GUI/window proof"
  fi
  run_helper_environment_smoke
  exit 0
fi

set +e
run_gui_supervisor_smoke
gui_status=$?
set -e

if [[ "$gui_status" -eq 0 ]]; then
  exit 0
fi

if [[ "$REQUIRE_GUI" == "1" || "$REQUIRE_WINDOW" == "1" ]]; then
  exit "$gui_status"
fi

echo "WARN: GUI launch path was unavailable; falling back to bundled helper environment smoke" >&2
stop_processes
rm -f "$SOCKET"
run_helper_environment_smoke
