#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
APP="${BRIDGEVM_MACOS_APP:-$ROOT/target/macos/BridgeVMApp.app}"
DMG="${BRIDGEVM_MACOS_DMG:-$ROOT/target/macos/BridgeVM.dmg}"
OUT="${BRIDGEVM_MACOS_ARTIFACT_MANIFEST:-$ROOT/target/macos/BridgeVM-artifacts.txt}"
APPLE_VZ_RUNNER="$APP/Contents/Helpers/AppleVzRunner"
BRIDGEVMD="$APP/Contents/Helpers/bridgevmd"
LIGHTVM_RUNNER="$APP/Contents/Helpers/lightvm-runner"
APP_EXECUTABLE="$APP/Contents/MacOS/BridgeVMApp"
APP_NOTARY_SUBMIT_JSON="${BRIDGEVM_MACOS_APP_NOTARY_SUBMIT_JSON:-}"
APP_NOTARY_LOG_JSON="${BRIDGEVM_MACOS_APP_NOTARY_LOG_JSON:-}"
DMG_NOTARY_SUBMIT_JSON="${BRIDGEVM_MACOS_DMG_NOTARY_SUBMIT_JSON:-}"
DMG_NOTARY_LOG_JSON="${BRIDGEVM_MACOS_DMG_NOTARY_LOG_JSON:-}"
APP_ONLY=0
POSITIONAL=()

usage() {
  cat >&2 <<'EOF'
usage: packaging/macos/write-artifact-manifest.sh [--app-only] [APP] [DMG] [OUT]

Writes a release-audit manifest for BridgeVM macOS artifacts. The manifest
records paths, sizes, SHA-256 digests, selected Info.plist metadata, codesign
details, Gatekeeper assessment output, and stapler validation output. It is a
recording tool, not a release gate: use verify-release-candidate.sh for pass/fail
release-candidate validation.

With --app-only, writes only app bundle and helper metadata/checks. In that
mode positional arguments are [APP] [OUT], and no DMG path is required or
inspected.

Environment:
  BRIDGEVM_MACOS_APP                app bundle path, defaults to target/macos/BridgeVMApp.app
  BRIDGEVM_MACOS_DMG                dmg path, defaults to target/macos/BridgeVM.dmg
  BRIDGEVM_MACOS_ARTIFACT_MANIFEST  output path, defaults to target/macos/BridgeVM-artifacts.txt
  BRIDGEVM_MACOS_APP_NOTARY_SUBMIT_JSON
                                    optional app notarytool submit JSON sidecar
  BRIDGEVM_MACOS_APP_NOTARY_LOG_JSON
                                    optional app notarytool log JSON sidecar
  BRIDGEVM_MACOS_DMG_NOTARY_SUBMIT_JSON
                                    optional DMG notarytool submit JSON sidecar
  BRIDGEVM_MACOS_DMG_NOTARY_LOG_JSON
                                    optional DMG notarytool log JSON sidecar
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --app-only)
      APP_ONLY=1
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

if [[ "$APP_ONLY" == "1" && "${#POSITIONAL[@]}" -gt 2 ]]; then
  usage
  exit 2
fi
if [[ "$APP_ONLY" == "0" && "${#POSITIONAL[@]}" -gt 3 ]]; then
  usage
  exit 2
fi
if [[ "${#POSITIONAL[@]}" -ge 1 ]]; then
  APP="${POSITIONAL[0]}"
  APPLE_VZ_RUNNER="$APP/Contents/Helpers/AppleVzRunner"
  BRIDGEVMD="$APP/Contents/Helpers/bridgevmd"
  LIGHTVM_RUNNER="$APP/Contents/Helpers/lightvm-runner"
  APP_EXECUTABLE="$APP/Contents/MacOS/BridgeVMApp"
fi
if [[ "$APP_ONLY" == "1" && "${#POSITIONAL[@]}" -ge 2 ]]; then
  OUT="${POSITIONAL[1]}"
elif [[ "$APP_ONLY" == "0" && "${#POSITIONAL[@]}" -ge 2 ]]; then
  DMG="${POSITIONAL[1]}"
fi
if [[ "$APP_ONLY" == "0" && "${#POSITIONAL[@]}" -ge 3 ]]; then
  OUT="${POSITIONAL[2]}"
fi

[[ -d "$APP" ]] || {
  echo "BridgeVM app bundle is missing: $APP" >&2
  exit 1
}
if [[ "$APP_ONLY" == "0" ]]; then
  [[ -f "$DMG" ]] || {
    echo "BridgeVM DMG is missing: $DMG" >&2
    exit 1
  }
fi

sha256() {
  shasum -a 256 "$1" | awk '{ print $1 }'
}

size_bytes() {
  stat -f '%z' "$1"
}

plist_value() {
  local key="$1"
  /usr/libexec/PlistBuddy -c "Print :$key" "$APP/Contents/Info.plist" 2>/dev/null || true
}

record_command() {
  local label="$1"
  shift
  local tmp
  local status
  tmp="$(mktemp "${TMPDIR:-/tmp}/bridgevm-manifest.XXXXXX")"

  set +e
  "$@" >"$tmp" 2>&1
  status=$?
  set -e

  {
    printf '%s.exit=%s\n' "$label" "$status"
    if [[ -s "$tmp" ]]; then
      printf '%s.output<<EOF\n' "$label"
      sed 's/[[:space:]]*$//' "$tmp"
      printf 'EOF\n'
    else
      printf '%s.output=\n' "$label"
    fi
  } >>"$OUT"
  rm -f "$tmp"
}

record_helper_artifact() {
  local key="$1"
  local path="$2"
  printf '%s.path=%s\n' "$key" "$path"
  if [[ -f "$path" ]]; then
    printf '%s.present=true\n' "$key"
    if [[ -x "$path" ]]; then
      printf '%s.executable=true\n' "$key"
    else
      printf '%s.executable=false\n' "$key"
    fi
    printf '%s.size_bytes=%s\n' "$key" "$(size_bytes "$path")"
    printf '%s.sha256=%s\n' "$key" "$(sha256 "$path")"
  else
    printf '%s.present=false\n' "$key"
  fi
}

record_optional_file_artifact() {
  local key="$1"
  local path="$2"
  [[ -n "$path" ]] || return 0
  printf '%s.path=%s\n' "$key" "$path"
  if [[ -f "$path" ]]; then
    printf '%s.present=true\n' "$key"
    printf '%s.size_bytes=%s\n' "$key" "$(size_bytes "$path")"
    printf '%s.sha256=%s\n' "$key" "$(sha256 "$path")"
  else
    printf '%s.present=false\n' "$key"
  fi
}

record_optional_notary_json_artifact() {
  local key="$1"
  local path="$2"
  record_optional_file_artifact "$key" "$path"
  [[ -n "$path" && -f "$path" ]] || return 0

  local id
  local status
  id="$(/usr/bin/plutil -extract id raw -o - "$path" 2>/dev/null || true)"
  status="$(/usr/bin/plutil -extract status raw -o - "$path" 2>/dev/null || true)"
  [[ -z "$id" ]] || printf '%s.id=%s\n' "$key" "$id"
  [[ -z "$status" ]] || printf '%s.status=%s\n' "$key" "$status"
}

mkdir -p "$(dirname "$OUT")"

{
  printf 'BridgeVM macOS artifact manifest\n'
  printf 'generated_utc=%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  printf 'workspace=%s\n' "$ROOT"
  if [[ "$APP_ONLY" == "1" ]]; then
    printf 'mode=app-only\n'
  else
    printf 'mode=app-and-dmg\n'
  fi
  printf '\n[artifacts]\n'
  printf 'app.path=%s\n' "$APP"
  record_helper_artifact app_executable "$APP_EXECUTABLE"
  printf 'app.executable=%s\n' "$APP_EXECUTABLE"
  printf 'app.size_bytes=%s\n' "$(du -sk "$APP" | awk '{ print $1 * 1024 }')"
  printf 'app.executable_sha256=%s\n' "$(sha256 "$APP_EXECUTABLE")"
  record_helper_artifact apple_vz_runner "$APPLE_VZ_RUNNER"
  record_helper_artifact bridgevmd "$BRIDGEVMD"
  record_helper_artifact lightvm_runner "$LIGHTVM_RUNNER"
  if [[ "$APP_ONLY" == "0" ]]; then
    printf 'dmg.path=%s\n' "$DMG"
    printf 'dmg.size_bytes=%s\n' "$(size_bytes "$DMG")"
    printf 'dmg.sha256=%s\n' "$(sha256 "$DMG")"
  fi
  record_optional_notary_json_artifact app_notary_submit_json "$APP_NOTARY_SUBMIT_JSON"
  record_optional_notary_json_artifact app_notary_log_json "$APP_NOTARY_LOG_JSON"
  if [[ "$APP_ONLY" == "0" ]]; then
    record_optional_notary_json_artifact dmg_notary_submit_json "$DMG_NOTARY_SUBMIT_JSON"
    record_optional_notary_json_artifact dmg_notary_log_json "$DMG_NOTARY_LOG_JSON"
  fi
  printf '\n[plist]\n'
  printf 'CFBundleIdentifier=%s\n' "$(plist_value CFBundleIdentifier)"
  printf 'CFBundleName=%s\n' "$(plist_value CFBundleName)"
  printf 'CFBundleDisplayName=%s\n' "$(plist_value CFBundleDisplayName)"
  printf 'CFBundleIconFile=%s\n' "$(plist_value CFBundleIconFile)"
  printf 'CFBundleShortVersionString=%s\n' "$(plist_value CFBundleShortVersionString)"
  printf 'CFBundleVersion=%s\n' "$(plist_value CFBundleVersion)"
  printf 'LSMinimumSystemVersion=%s\n' "$(plist_value LSMinimumSystemVersion)"
  printf '\n[checks]\n'
} >"$OUT"

record_command app_codesign_verify codesign --verify --deep --strict "$APP"
record_command app_codesign_details codesign -dv --verbose=4 "$APP"
if [[ -f "$APPLE_VZ_RUNNER" ]]; then
  record_command apple_vz_runner_codesign_verify codesign --verify --strict "$APPLE_VZ_RUNNER"
  record_command apple_vz_runner_codesign_details codesign -dv --verbose=4 "$APPLE_VZ_RUNNER"
  record_command apple_vz_runner_entitlements codesign -d --entitlements :- "$APPLE_VZ_RUNNER"
fi
if [[ -f "$BRIDGEVMD" ]]; then
  record_command bridgevmd_codesign_verify codesign --verify --strict "$BRIDGEVMD"
  record_command bridgevmd_codesign_details codesign -dv --verbose=4 "$BRIDGEVMD"
fi
if [[ -f "$LIGHTVM_RUNNER" ]]; then
  record_command lightvm_runner_codesign_verify codesign --verify --strict "$LIGHTVM_RUNNER"
  record_command lightvm_runner_codesign_details codesign -dv --verbose=4 "$LIGHTVM_RUNNER"
fi
record_command app_gatekeeper spctl --assess --type execute --verbose=4 "$APP"
record_command app_stapler xcrun stapler validate "$APP"
if [[ "$APP_ONLY" == "0" ]]; then
  record_command dmg_codesign_verify codesign --verify --strict "$DMG"
  record_command dmg_codesign_details codesign -dv --verbose=4 "$DMG"
  record_command dmg_hdiutil_verify hdiutil verify "$DMG"
  record_command dmg_gatekeeper spctl --assess --type open --context context:primary-signature --verbose=4 "$DMG"
  record_command dmg_stapler xcrun stapler validate "$DMG"
fi

printf '%s\n' "$OUT"
