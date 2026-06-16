#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
APP="${BRIDGEVM_MACOS_APP:-$ROOT/target/macos/BridgeVMApp.app}"
DMG="${BRIDGEVM_MACOS_DMG:-$ROOT/target/macos/BridgeVM.dmg}"
DMG_VOLUME="${BRIDGEVM_MACOS_DMG_VOLUME:-BridgeVM}"
EXPECT_DEBUG_BOUNDARY=0
LAUNCH_SMOKE="${BRIDGEVM_MACOS_LAUNCH_SMOKE:-0}"
QUARANTINE_SMOKE="${BRIDGEVM_MACOS_QUARANTINE_SMOKE:-0}"
LAUNCH_SMOKE_TIMEOUT_TENTHS="${BRIDGEVM_MACOS_LAUNCH_SMOKE_TIMEOUT_TENTHS:-100}"
OPEN_TOOL="${BRIDGEVM_MACOS_OPEN_TOOL:-open}"
POSITIONAL=()

usage() {
  cat >&2 <<'EOF'
usage: packaging/macos/verify-release-candidate.sh [--expect-debug-boundary] [--launch-smoke] [--quarantine-smoke] [APP] [DMG]

Verifies BridgeVM macOS app and DMG artifacts against public distribution
requirements. The default mode expects a real release candidate: valid code
signature, valid DMG, bundled AppleVzRunner helper signature and entitlement,
Gatekeeper acceptance, and stapled notarization tickets.

Use --expect-debug-boundary for local debug artifacts. That mode still verifies
the app and DMG structure, then passes only when public release gates reject the
artifacts, making the debug-vs-release boundary explicit and testable.

Use --launch-smoke on an interactive macOS release host to mount the DMG,
launch the contained app through LaunchServices, and verify that a main window
appears before publishing.

Use --quarantine-smoke on an interactive macOS release host to copy the DMG,
apply the quarantine xattr that downloaded artifacts receive, mount the copied
DMG, launch the contained app, and verify that a main window appears.

Environment:
  BRIDGEVM_MACOS_APP    app bundle path, defaults to target/macos/BridgeVMApp.app
  BRIDGEVM_MACOS_DMG    dmg path, defaults to target/macos/BridgeVM.dmg
  BRIDGEVM_MACOS_DMG_VOLUME
                        expected mounted DMG volume name, defaults to BridgeVM
  BRIDGEVM_MACOS_LAUNCH_SMOKE
                        set to 1 to enable the optional LaunchServices smoke
  BRIDGEVM_MACOS_QUARANTINE_SMOKE
                        set to 1 to enable the optional quarantined DMG smoke
  BRIDGEVM_MACOS_LAUNCH_SMOKE_TIMEOUT_TENTHS
                        launch smoke timeout in tenths of a second, defaults to 100
  BRIDGEVM_MACOS_OPEN_TOOL
                        LaunchServices opener for launch smoke, defaults to open
  BRIDGEVM_RELEASE_TEAM_ID
                        optional expected Developer ID team identifier
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --expect-debug-boundary)
      EXPECT_DEBUG_BOUNDARY=1
      shift
      ;;
    --launch-smoke)
      LAUNCH_SMOKE=1
      shift
      ;;
    --quarantine-smoke)
      QUARANTINE_SMOKE=1
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    -*)
      usage
      exit 2
      ;;
    *)
      POSITIONAL+=("$1")
      shift
      ;;
  esac
done

if [[ "${#POSITIONAL[@]}" -gt 2 ]]; then
  usage
  exit 2
fi
if [[ "${#POSITIONAL[@]}" -ge 1 ]]; then
  APP="${POSITIONAL[0]}"
fi
if [[ "${#POSITIONAL[@]}" -ge 2 ]]; then
  DMG="${POSITIONAL[1]}"
fi

run_base_check() {
  echo "==> $*"
  "$@"
}

run_release_gate() {
  local label="$1"
  shift
  local output

  if output="$("$@" 2>&1)"; then
    printf 'PASS: %s\n' "$label"
    return 0
  fi

  printf 'FAIL: %s\n' "$label"
  if [[ -n "$output" ]]; then
    printf '%s\n' "$output" | sed 's/^/  /'
  fi
  return 1
}

verify_hardened_runtime() {
  local path="$1"
  local output
  output="$(codesign -dvv "$path" 2>&1)" || return 1
  case "$output" in
    *"flags="*"runtime"*) return 0 ;;
    *) return 1 ;;
  esac
}

verify_developer_id_signature() {
  local path="$1"
  local expected_team="${BRIDGEVM_RELEASE_TEAM_ID:-}"
  local output
  output="$(codesign -dvvv "$path" 2>&1)" || return 1
  case "$output" in
    *"Authority=Developer ID Application:"*) ;;
    *) return 1 ;;
  esac
  if [[ -n "$expected_team" ]]; then
    case "$output" in
      *"TeamIdentifier=$expected_team"*) ;;
      *) return 1 ;;
    esac
  fi
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

stop_processes() {
  local pids="$1"
  local pid

  while IFS= read -r pid; do
    [[ -n "$pid" ]] || continue
    stop_process "$pid"
  done <<< "$pids"
}

process_ids_under_path() {
  local path="$1"
  local path_real="${2:-$1}"
  ps -axo pid=,command= | awk -v path="$path" -v real="$path_real" '
    function normalize(value) {
      gsub(/\/+/, "/", value)
      sub(/^\/private\/var\//, "/var/", value)
      return value
    }
    BEGIN {
      path = normalize(path)
      real = normalize(real)
    }
    {
      pid = $1
      sub(/^[[:space:]]*[0-9]+[[:space:]]+/, "", $0)
      command = normalize($0)
      if (index(command, path) == 1 || index(command, real) == 1) {
        print pid
      }
    }
  '
}

stop_processes_under_path() {
  local path="$1"
  local path_real="${2:-$1}"
  local pid

  for pid in $(process_ids_under_path "$path" "$path_real" || true); do
    stop_process "$pid"
  done
}

is_mountpoint_mounted() {
  local mount_dir="$1"
  local mount_dir_real="${2:-$1}"
  mount | grep -F " on $mount_dir " >/dev/null 2>&1 || mount | grep -F " on $mount_dir_real " >/dev/null 2>&1
}

hdiutil_detach_bounded() {
  local target="$1"
  local mode="${2:-}"
  local pid

  if [[ "$mode" == "-force" ]]; then
    hdiutil detach "$target" -force -quiet >/dev/null 2>&1 &
  else
    hdiutil detach "$target" -quiet >/dev/null 2>&1 &
  fi
  pid=$!
  for _ in {1..30}; do
    if ! kill -0 "$pid" 2>/dev/null; then
      wait "$pid" 2>/dev/null || true
      return 0
    fi
    sleep 0.1
  done
  disown "$pid" 2>/dev/null || true
  kill "$pid" 2>/dev/null || true
  sleep 0.1
  kill -9 "$pid" 2>/dev/null || true
  return 1
}

detach_mount() {
  local device="$1"
  local mount_dir="$2"
  local mount_dir_real="${3:-$2}"
  local target

  for target in "$mount_dir_real" "$mount_dir" "$device"; do
    [[ -n "$target" ]] || continue
    hdiutil_detach_bounded "$target" || true
    is_mountpoint_mounted "$mount_dir" "$mount_dir_real" || return 0
  done
  for target in "$mount_dir_real" "$mount_dir" "$device"; do
    [[ -n "$target" ]] || continue
    hdiutil_detach_bounded "$target" -force || true
    is_mountpoint_mounted "$mount_dir" "$mount_dir_real" || return 0
  done
  if is_mountpoint_mounted "$mount_dir" "$mount_dir_real"; then
    return 1
  fi
  return 0
}

dmg_devices() {
  local dmg="$1"
  local dmg_real
  dmg_real="$(cd "$(dirname "$dmg")" && pwd -P)/$(basename "$dmg")"

  hdiutil info | awk -v dmg="$dmg" -v dmg_real="$dmg_real" '
    function normalize(value) {
      gsub(/\/+/, "/", value)
      sub(/^\/private\/var\//, "/var/", value)
      return value
    }
    BEGIN {
      dmg = normalize(dmg)
      dmg_real = normalize(dmg_real)
      matching = 0
    }
    /^================================================$/ {
      matching = 0
      next
    }
    /^image-path[[:space:]]*:/ || /^image-alias[[:space:]]*:/ {
      image = $0
      sub(/^[^:]+:[[:space:]]*/, "", image)
      image = normalize(image)
      if (image == dmg || image == dmg_real) {
        matching = 1
      }
      next
    }
    matching && /^\/dev\/disk[0-9]+[[:space:]]/ {
      print $1
      matching = 0
    }
  '
}

detach_dmg_instances() {
  local dmg="$1"
  local device

  for device in $(dmg_devices "$dmg" || true); do
    hdiutil_detach_bounded "$device" || hdiutil_detach_bounded "$device" -force || true
  done
}

cleanup_mounted_dir() {
  local device="$1"
  local mount_dir="$2"
  local mount_dir_real="${3:-$2}"

  detach_mount "$device" "$mount_dir" "$mount_dir_real" || true
  if ! is_mountpoint_mounted "$mount_dir" "$mount_dir_real"; then
    rm -rf "$mount_dir"
  fi
}

cleanup_launch_smoke() {
  local launched_pids="$1"
  local device="$2"
  local mount_dir="$3"
  local mount_dir_real="${4:-$3}"

  stop_processes "$launched_pids"
  stop_processes_under_path "$mount_dir" "$mount_dir_real"
  cleanup_mounted_dir "$device" "$mount_dir" "$mount_dir_real"
}

finish_launch_smoke() {
  local smoke_status="$1"
  local launched_pids="$2"
  local device="$3"
  local mount_dir="$4"
  local mount_dir_real="${5:-$4}"
  local detach_status

  trap - RETURN
  detach_status=0
  stop_processes "$launched_pids"
  stop_processes_under_path "$mount_dir" "$mount_dir_real"
  detach_mount "$device" "$mount_dir" "$mount_dir_real" || detach_status=$?
  if ! is_mountpoint_mounted "$mount_dir" "$mount_dir_real"; then
    rm -rf "$mount_dir"
  fi
  if [[ "$smoke_status" -eq 0 && "$detach_status" -ne 0 ]]; then
    echo "BridgeVM launch smoke could not detach mounted DMG: $mount_dir" >&2
    return "$detach_status"
  fi
  return "$smoke_status"
}

attach_dmg_readonly_at_mountpoint() {
  local dmg="$1"
  local mount_dir="$2"
  local mount_dir_real="${3:-$2}"
  local attach_output
  local attach_status
  local attempt

  for attempt in {1..6}; do
    detach_dmg_instances "$dmg"
    sleep 0.2

    set +e
    attach_output="$(hdiutil attach "$dmg" -readonly -nobrowse -noautoopen -noautoopenro -mountpoint "$mount_dir" 2>&1)"
    attach_status=$?
    set -e
    if [[ "$attach_status" -eq 0 ]]; then
      printf '%s\n' "$attach_output" | awk \
        -v mount_dir="$mount_dir" \
        -v mount_dir_real="$mount_dir_real" '
          $NF == mount_dir || $NF == mount_dir_real { mounted_device = $1 }
          { last_device = $1 }
          END {
            if (mounted_device != "") {
              print mounted_device
            } else {
              print last_device
            }
          }
        '
      return 0
    fi

    detach_dmg_instances "$dmg"
    case "$attach_output" in
      *"Resource busy"*)
        sleep "$attempt"
        continue
        ;;
      *)
        printf '%s\n' "$attach_output" >&2
        return "$attach_status"
        ;;
    esac
  done

  printf '%s\n' "$attach_output" >&2
  return "$attach_status"
}

process_ids_for_executable() {
  local executable="$1"
  local executable_real="${2:-$1}"
  ps -axo pid=,command= | awk -v exe="$executable" -v real="$executable_real" '
    {
      pid = $1
      sub(/^[[:space:]]*[0-9]+[[:space:]]+/, "", $0)
      if ($0 == exe || index($0, exe " ") == 1 || $0 == real || index($0, real " ") == 1) {
        print pid
      }
    }
  '
}

process_ids_for_command_name() {
  local command_name="$1"
  ps -axo pid=,comm= | awk -v command_name="$command_name" '
    {
      pid = $1
      sub(/^[[:space:]]*[0-9]+[[:space:]]+/, "", $0)
      name = $0
      sub(/^.*\//, "", name)
      if (name == command_name) {
        print pid
      }
    }
  '
}

is_existing_pid() {
  local pid="$1"
  local existing_pids="$2"
  case "$(printf '\n%s\n' "$existing_pids")" in
    *$'\n'"$pid"$'\n'*) return 0 ;;
    *) return 1 ;;
  esac
}

verify_dmg_contains_app() {
  local dmg="$1"
  local app_basename="$2"
  local mount_dir
  local device
  local mount_dir_real
  local status
  mount_dir="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-release-dmg.XXXXXX")"
  mount_dir_real="$(cd "$(dirname "$mount_dir")" && pwd -P)/$(basename "$mount_dir")"
  device=""
  trap 'cleanup_mounted_dir "$device" "$mount_dir" "$mount_dir_real"; trap - RETURN' RETURN

  device="$(attach_dmg_readonly_at_mountpoint "$dmg" "$mount_dir" "$mount_dir_real")"
  [[ -n "$device" ]] || {
    echo "Failed to attach BridgeVM DMG: $dmg" >&2
    return 1
  }
  local volume_name
  volume_name="$(diskutil info "$device" | awk -F': *' '/Volume Name:/ { print $2; exit }')"
  [[ "$volume_name" == "$DMG_VOLUME" ]] || {
    echo "BridgeVM DMG volume name mismatch: expected $DMG_VOLUME, got ${volume_name:-<missing>}" >&2
    return 1
  }
  local app_count
  local unexpected_app
  app_count="$(find "$mount_dir" -maxdepth 1 -name '*.app' -type d | wc -l | tr -d '[:space:]')"
  [[ "$app_count" == "1" ]] || {
    echo "BridgeVM DMG must contain exactly one top-level app bundle, found $app_count" >&2
    return 1
  }
  unexpected_app="$(find "$mount_dir" -maxdepth 1 -name '*.app' -type d ! -name "$app_basename" -print -quit)"
  [[ -z "$unexpected_app" ]] || {
    echo "BridgeVM DMG contains unexpected app bundle: $(basename "$unexpected_app")" >&2
    return 1
  }
  set +e
  "$ROOT/packaging/macos/build-debug-app-bundle.sh" --verify-only "$mount_dir/$app_basename" >/dev/null
  status=$?
  set -e
  [[ "$status" -eq 0 ]] || return "$status"
  [[ -L "$mount_dir/Applications" ]] || {
    echo "BridgeVM DMG is missing Applications symlink" >&2
    return 1
  }
  [[ "$(readlink "$mount_dir/Applications")" == "/Applications" ]] || {
    echo "BridgeVM DMG Applications symlink does not target /Applications" >&2
    return 1
  }
  return "$status"
}

verify_mounted_dmg_app_release_gates() {
  local dmg="$1"
  local app_basename="$2"
  local mount_dir
  local device
  local mount_dir_real
  local failures
  mount_dir="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-release-mounted-app.XXXXXX")"
  mount_dir_real="$(cd "$(dirname "$mount_dir")" && pwd -P)/$(basename "$mount_dir")"
  device=""
  failures=0
  trap 'cleanup_mounted_dir "$device" "$mount_dir" "$mount_dir_real"; trap - RETURN' RETURN

  device="$(attach_dmg_readonly_at_mountpoint "$dmg" "$mount_dir" "$mount_dir_real")"
  [[ -n "$device" ]] || {
    echo "Failed to attach BridgeVM DMG for mounted app release gates: $dmg" >&2
    return 1
  }

  run_release_gate "Gatekeeper mounted app assessment" \
    spctl --assess --type execute --verbose=4 "$mount_dir/$app_basename" \
    || failures=$((failures + 1))
  run_release_gate "Stapled mounted app notarization ticket" \
    xcrun stapler validate "$mount_dir/$app_basename" \
    || failures=$((failures + 1))
  run_release_gate "Mounted app AppleVzRunner helper signature and entitlement" \
    "$ROOT/apps/macos/scripts/build-sign-apple-vz-runner.sh" \
    --verify-only "$mount_dir/$app_basename/Contents/Helpers/AppleVzRunner" \
    || failures=$((failures + 1))
  run_release_gate "Mounted app AppleVzRunner Developer ID signature" \
    verify_developer_id_signature "$mount_dir/$app_basename/Contents/Helpers/AppleVzRunner" \
    || failures=$((failures + 1))
  run_release_gate "Mounted app AppleVzRunner hardened runtime" \
    verify_hardened_runtime "$mount_dir/$app_basename/Contents/Helpers/AppleVzRunner" \
    || failures=$((failures + 1))
  run_release_gate "Mounted app bridgevmd helper signature" \
    codesign --verify --strict "$mount_dir/$app_basename/Contents/Helpers/bridgevmd" \
    || failures=$((failures + 1))
  run_release_gate "Mounted app bridgevmd Developer ID signature" \
    verify_developer_id_signature "$mount_dir/$app_basename/Contents/Helpers/bridgevmd" \
    || failures=$((failures + 1))
  run_release_gate "Mounted app bridgevmd hardened runtime" \
    verify_hardened_runtime "$mount_dir/$app_basename/Contents/Helpers/bridgevmd" \
    || failures=$((failures + 1))
  run_release_gate "Mounted app lightvm-runner helper signature" \
    codesign --verify --strict "$mount_dir/$app_basename/Contents/Helpers/lightvm-runner" \
    || failures=$((failures + 1))
  run_release_gate "Mounted app lightvm-runner Developer ID signature" \
    verify_developer_id_signature "$mount_dir/$app_basename/Contents/Helpers/lightvm-runner" \
    || failures=$((failures + 1))
  run_release_gate "Mounted app lightvm-runner hardened runtime" \
    verify_hardened_runtime "$mount_dir/$app_basename/Contents/Helpers/lightvm-runner" \
    || failures=$((failures + 1))

  return "$failures"
}

verify_mounted_dmg_app_launch_smoke() {
  local dmg="$1"
  local app_basename="$2"
  local mount_dir
  local device
  local app_path
  local app_exec
  local existing_pids
  local launched_pids
  local app_exec_real
  local mount_dir_real
  mount_dir="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-release-launch.XXXXXX")"
  mount_dir_real="$(cd "$(dirname "$mount_dir")" && pwd -P)/$(basename "$mount_dir")"
  device=""
  launched_pids=""
  trap 'cleanup_launch_smoke "$launched_pids" "$device" "$mount_dir" "$mount_dir_real"; trap - RETURN' RETURN

  [[ "$(uname -s)" == "Darwin" ]] || {
    echo "BridgeVM launch smoke requires macOS" >&2
    return 1
  }

  device="$(attach_dmg_readonly_at_mountpoint "$dmg" "$mount_dir" "$mount_dir_real")"
  [[ -n "$device" ]] || {
    echo "Failed to attach BridgeVM DMG for launch smoke: $dmg" >&2
    return 1
  }

  app_path="$mount_dir/$app_basename"
  app_exec="$app_path/Contents/MacOS/BridgeVMApp"
  [[ -x "$app_exec" ]] || {
    echo "BridgeVM mounted app executable is missing or not executable: $app_exec" >&2
    return 1
  }
  app_exec_real="$(cd "$(dirname "$app_exec")" && pwd -P)/$(basename "$app_exec")"

  existing_pids="$(process_ids_for_executable "$app_exec" "$app_exec_real" || true)"
  existing_pids="${existing_pids}"$'\n'"$(process_ids_for_command_name "$(basename "$app_exec")" || true)"
  "$OPEN_TOOL" -n "$app_path"
  for ((i = 0; i < LAUNCH_SMOKE_TIMEOUT_TENTHS; i++)); do
    for candidate_pid in $(process_ids_for_executable "$app_exec" "$app_exec_real" || true); do
      is_existing_pid "$candidate_pid" "$existing_pids" && continue
      is_existing_pid "$candidate_pid" "$launched_pids" || launched_pids="${launched_pids}"$'\n'"$candidate_pid"
      if find_bridgevm_window "$candidate_pid"; then
        finish_launch_smoke 0 "$launched_pids" "$device" "$mount_dir" "$mount_dir_real"
        return $?
      fi
    done
    for candidate_pid in $(process_ids_for_command_name "$(basename "$app_exec")" || true); do
      is_existing_pid "$candidate_pid" "$existing_pids" && continue
      is_existing_pid "$candidate_pid" "$launched_pids" || launched_pids="${launched_pids}"$'\n'"$candidate_pid"
      if find_bridgevm_window "$candidate_pid"; then
        finish_launch_smoke 0 "$launched_pids" "$device" "$mount_dir" "$mount_dir_real"
        return $?
      fi
    done
    sleep 0.1
  done

  echo "BridgeVM mounted app did not present a main window from LaunchServices: $app_path" >&2
  finish_launch_smoke 1 "$launched_pids" "$device" "$mount_dir" "$mount_dir_real"
  return $?
}

verify_quarantined_dmg_app_launch_smoke() {
  local dmg="$1"
  local app_basename="$2"
  local work_dir
  local quarantined_dmg
  local quarantine_value
  local status

  [[ "$(uname -s)" == "Darwin" ]] || {
    echo "BridgeVM quarantined launch smoke requires macOS" >&2
    return 1
  }
  command -v xattr >/dev/null 2>&1 || {
    echo "BridgeVM quarantined launch smoke requires xattr" >&2
    return 1
  }

  work_dir="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-release-quarantine.XXXXXX")"
  quarantined_dmg="$work_dir/$(basename "$dmg")"
  trap 'rm -rf "$work_dir"; trap - RETURN' RETURN

  cp "$dmg" "$quarantined_dmg"
  quarantine_value="0081;$(date +%s);BridgeVM;$(basename "$dmg")"
  xattr -w com.apple.quarantine "$quarantine_value" "$quarantined_dmg"
  if ! xattr -p com.apple.quarantine "$quarantined_dmg" >/dev/null 2>&1; then
    echo "BridgeVM quarantined launch smoke could not apply quarantine xattr: $quarantined_dmg" >&2
    return 1
  fi

  set +e
  verify_mounted_dmg_app_launch_smoke "$quarantined_dmg" "$app_basename"
  status=$?
  set -e
  rm -rf "$work_dir"
  trap - RETURN
  return "$status"
}

verify_quarantined_dmg_debug_gatekeeper_boundary() {
  local dmg="$1"
  local app_basename="$2"
  local work_dir
  local quarantined_dmg
  local quarantine_value
  local mount_dir
  local mount_dir_real
  local device
  local app_path
  local status

  [[ "$(uname -s)" == "Darwin" ]] || {
    echo "BridgeVM quarantined Gatekeeper boundary check requires macOS" >&2
    return 1
  }
  command -v xattr >/dev/null 2>&1 || {
    echo "BridgeVM quarantined Gatekeeper boundary check requires xattr" >&2
    return 1
  }

  work_dir="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-release-quarantine-boundary.XXXXXX")"
  mount_dir="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-release-quarantine-boundary-mount.XXXXXX")"
  mount_dir_real="$(cd "$(dirname "$mount_dir")" && pwd -P)/$(basename "$mount_dir")"
  quarantined_dmg="$work_dir/$(basename "$dmg")"
  device=""
  trap 'cleanup_mounted_dir "$device" "$mount_dir" "$mount_dir_real"; rm -rf "$work_dir"; trap - RETURN' RETURN

  cp "$dmg" "$quarantined_dmg"
  quarantine_value="0081;$(date +%s);BridgeVM;$(basename "$dmg")"
  xattr -w com.apple.quarantine "$quarantine_value" "$quarantined_dmg"
  if ! xattr -p com.apple.quarantine "$quarantined_dmg" >/dev/null 2>&1; then
    echo "BridgeVM quarantined Gatekeeper boundary check could not apply quarantine xattr: $quarantined_dmg" >&2
    return 1
  fi

  device="$(attach_dmg_readonly_at_mountpoint "$quarantined_dmg" "$mount_dir" "$mount_dir_real")"
  [[ -n "$device" ]] || {
    echo "Failed to attach quarantined BridgeVM DMG for Gatekeeper boundary check: $quarantined_dmg" >&2
    return 1
  }

  app_path="$mount_dir/$app_basename"
  [[ -d "$app_path" ]] || {
    echo "BridgeVM quarantined mounted app is missing: $app_path" >&2
    return 1
  }
  if ! xattr -p com.apple.quarantine "$app_path" >/dev/null 2>&1; then
    echo "BridgeVM quarantined DMG did not propagate quarantine to mounted app: $app_path" >&2
    return 1
  fi
  echo "PASS: Quarantined DMG propagated quarantine to mounted app"

  set +e
  spctl --assess --type execute --verbose=4 "$app_path" >/dev/null 2>&1
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    echo "Expected Gatekeeper to reject quarantined debug app, but assessment passed: $app_path" >&2
    return 1
  fi

  cleanup_mounted_dir "$device" "$mount_dir" "$mount_dir_real"
  rm -rf "$work_dir"
  trap - RETURN
  return 0
}

[[ -d "$APP" ]] || {
  echo "BridgeVM app bundle is missing: $APP" >&2
  exit 1
}
[[ -f "$DMG" ]] || {
  echo "BridgeVM DMG is missing: $DMG" >&2
  exit 1
}

run_base_check "$ROOT/packaging/macos/build-debug-app-bundle.sh" --verify-only "$APP"
run_base_check hdiutil verify "$DMG"
run_base_check verify_dmg_contains_app "$DMG" "$(basename "$APP")"

release_failures=0
mounted_app_failures=0
launch_smoke_failures=0
run_release_gate "Gatekeeper app assessment" spctl --assess --type execute --verbose=4 "$APP" || release_failures=$((release_failures + 1))
run_release_gate "Gatekeeper DMG assessment" spctl --assess --type open --context context:primary-signature --verbose=4 "$DMG" || release_failures=$((release_failures + 1))
run_release_gate "Stapled app notarization ticket" xcrun stapler validate "$APP" || release_failures=$((release_failures + 1))
run_release_gate "Stapled DMG notarization ticket" xcrun stapler validate "$DMG" || release_failures=$((release_failures + 1))
run_release_gate "BridgeVM app Developer ID signature" verify_developer_id_signature "$APP" || release_failures=$((release_failures + 1))
run_release_gate "BridgeVM DMG Developer ID signature" verify_developer_id_signature "$DMG" || release_failures=$((release_failures + 1))
run_release_gate "BridgeVM app hardened runtime" verify_hardened_runtime "$APP" || release_failures=$((release_failures + 1))
run_release_gate "Bundled AppleVzRunner helper signature and entitlement" \
  "$ROOT/apps/macos/scripts/build-sign-apple-vz-runner.sh" \
  --verify-only "$APP/Contents/Helpers/AppleVzRunner" \
  || release_failures=$((release_failures + 1))
run_release_gate "Bundled AppleVzRunner helper Developer ID signature" \
  verify_developer_id_signature "$APP/Contents/Helpers/AppleVzRunner" \
  || release_failures=$((release_failures + 1))
run_release_gate "Bundled AppleVzRunner helper hardened runtime" \
  verify_hardened_runtime "$APP/Contents/Helpers/AppleVzRunner" \
  || release_failures=$((release_failures + 1))
run_release_gate "Bundled bridgevmd helper signature" \
  codesign --verify --strict "$APP/Contents/Helpers/bridgevmd" \
  || release_failures=$((release_failures + 1))
run_release_gate "Bundled bridgevmd helper Developer ID signature" \
  verify_developer_id_signature "$APP/Contents/Helpers/bridgevmd" \
  || release_failures=$((release_failures + 1))
run_release_gate "Bundled bridgevmd helper hardened runtime" \
  verify_hardened_runtime "$APP/Contents/Helpers/bridgevmd" \
  || release_failures=$((release_failures + 1))
run_release_gate "Bundled lightvm-runner helper signature" \
  codesign --verify --strict "$APP/Contents/Helpers/lightvm-runner" \
  || release_failures=$((release_failures + 1))
run_release_gate "Bundled lightvm-runner helper Developer ID signature" \
  verify_developer_id_signature "$APP/Contents/Helpers/lightvm-runner" \
  || release_failures=$((release_failures + 1))
run_release_gate "Bundled lightvm-runner helper hardened runtime" \
  verify_hardened_runtime "$APP/Contents/Helpers/lightvm-runner" \
  || release_failures=$((release_failures + 1))
set +e
verify_mounted_dmg_app_release_gates "$DMG" "$(basename "$APP")"
mounted_app_failures=$?
set -e
release_failures=$((release_failures + mounted_app_failures))
if [[ "$LAUNCH_SMOKE" == "1" ]]; then
  run_release_gate "Mounted app LaunchServices smoke" \
    verify_mounted_dmg_app_launch_smoke "$DMG" "$(basename "$APP")" \
    || launch_smoke_failures=$((launch_smoke_failures + 1))
fi
if [[ "$QUARANTINE_SMOKE" == "1" ]]; then
  if [[ "$EXPECT_DEBUG_BOUNDARY" == "1" ]]; then
    run_release_gate "Quarantined DMG Gatekeeper debug boundary" \
      verify_quarantined_dmg_debug_gatekeeper_boundary "$DMG" "$(basename "$APP")" \
      || launch_smoke_failures=$((launch_smoke_failures + 1))
  else
    run_release_gate "Quarantined DMG LaunchServices smoke" \
      verify_quarantined_dmg_app_launch_smoke "$DMG" "$(basename "$APP")" \
      || launch_smoke_failures=$((launch_smoke_failures + 1))
  fi
fi
if [[ "$launch_smoke_failures" -ne 0 ]]; then
  echo "BridgeVM macOS artifacts failed $launch_smoke_failures launch smoke gate(s)." >&2
  exit 1
fi

if [[ "$EXPECT_DEBUG_BOUNDARY" == "1" ]]; then
  if [[ "$release_failures" -eq 0 ]]; then
    echo "Expected debug artifacts to fail at least one public release gate, but all passed." >&2
    exit 1
  fi
  echo "PASS: debug artifacts are structurally valid but not public release candidates"
  exit 0
fi

if [[ "$release_failures" -ne 0 ]]; then
  echo "BridgeVM macOS artifacts failed $release_failures public release gate(s)." >&2
  exit 1
fi

echo "PASS: BridgeVM macOS release candidate"
