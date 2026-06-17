#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-template-media.XXXXXX")"
LOCAL_VM="template-media-local"
UNSAFE_VM="template-media-unsafe"
SOCKET_VM="template-media-socket"
TEMPLATE_ID="ubuntu-arm64-installer"
LOCAL_BUNDLE="$STORE/vms/$LOCAL_VM.vmbridge"
UNSAFE_BUNDLE="$STORE/vms/$UNSAFE_VM.vmbridge"
SOCKET_BUNDLE="$STORE/vms/$SOCKET_VM.vmbridge"
LOCAL_INSTALLER="$LOCAL_BUNDLE/installers/ubuntu-arm64.iso"
UNSAFE_INSTALLER="$UNSAFE_BUNDLE/../escaped-installer.iso"
UNSAFE_ESCAPED_FILE="$STORE/vms/escaped-installer.iso"
SOCKET_INSTALLER="$SOCKET_BUNDLE/installers/ubuntu-arm64.iso"
LOCAL_DISK="$LOCAL_BUNDLE/disks/root.qcow2"
LOCAL_LAUNCH_SPEC="$LOCAL_BUNDLE/metadata/apple-vz-launch.json"
FIXTURE="$STORE/local-installer.iso"
DOWNLOAD_URL="https://example.invalid/bridgevm/ubuntu-arm64.iso"
SOCKET="$STORE/run/bridgevmd.sock"
DAEMON_LOG="$STORE/bridgevmd.log"
DAEMON_PID=""
PRESERVE_STORE=1

printf "bridgevm template boot media fixture\n" >"$FIXTURE"
EXPECTED_SHA256="$(shasum -a 256 "$FIXTURE" | awk '{print $1}')"
BAD_SHA256="0000000000000000000000000000000000000000000000000000000000000000"
FIXTURE_BYTES="$(wc -c <"$FIXTURE" | tr -d ' ')"

bridgevm() {
  cargo run --quiet -p bridgevm-cli -- --store "$STORE" "$@"
}

bridgevmd() {
  cargo run --quiet -p bridgevm-daemon -- --store "$STORE"
}

bridgevm_socket() {
  cargo run --quiet -p bridgevm-cli -- --socket "$SOCKET" "$@"
}

fail() {
  echo "FAIL: $*" >&2
  echo "Store preserved at $STORE" >&2
  if [[ -f "$DAEMON_LOG" ]]; then
    echo "Daemon log: $DAEMON_LOG" >&2
  fi
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

assert_not_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  case "$haystack" in
    *"$needle"*) fail "$label unexpectedly included '$needle'; got: $haystack" ;;
  esac
}

assert_file_contains() {
  local file="$1"
  local needle="$2"
  local label="$3"
  [[ -f "$file" ]] || fail "$label missing file $file"
  grep -Fq "$needle" "$file" || fail "$label missing '$needle' in $file"
}

assert_fails_contains() {
  local label="$1"
  local expected="$2"
  shift 2

  local stdout="$STORE/$label.stdout"
  local stderr="$STORE/$label.stderr"
  if "$@" >"$stdout" 2>"$stderr"; then
    fail "$label unexpectedly succeeded"
  fi

  local output
  output="$(cat "$stdout" "$stderr")"
  ASSERT_OUTPUT="$output"
  assert_contains "$output" "$expected" "$label"
}

assert_no_launch_claim() {
  local output="$1"
  local label="$2"

  assert_not_contains "$output" "Engine:" "$label"
  assert_not_contains "$output" "Launch ready:" "$label"
  assert_not_contains "$output" "Launch blockers:" "$label"
  assert_not_contains "$output" "Fast Mode spawn requires BRIDGEVM_APPLE_VZ_RUNNER" "$label"
}

assert_missing_template_media() {
  local label="$1"
  local output="$2"
  local vm="$3"
  local installer="$4"

  assert_contains "$output" "VM: $vm" "$label"
  assert_contains "$output" "Boot mode: linux-installer" "$label"
  assert_contains "$output" "Installer image: $installer" "$label"
  assert_contains "$output" "Installer image exists: false" "$label"
}

assert_missing_template_status() {
  local label="$1"
  local output="$2"
  local vm="$3"
  local installer="$4"

  assert_contains "$output" "VM: $vm" "$label"
  assert_contains "$output" "Boot media kind: installer-image" "$label"
  assert_contains "$output" "Path: $installer" "$label"
  assert_contains "$output" "Exists: false" "$label"
  assert_contains "$output" "Bytes: unknown" "$label"
  assert_contains "$output" "Last import: none" "$label"
  assert_contains "$output" "Last verification: none" "$label"
  assert_contains "$output" "Last download plan: none" "$label"
  assert_contains "$output" "Last download result: none" "$label"
}

assert_imported_template_status() {
  local label="$1"
  local output="$2"
  local vm="$3"
  local installer="$4"

  assert_contains "$output" "VM: $vm" "$label"
  assert_contains "$output" "Boot media kind: installer-image" "$label"
  assert_contains "$output" "Path: $installer" "$label"
  assert_contains "$output" "Exists: true" "$label"
  assert_contains "$output" "Bytes: $FIXTURE_BYTES" "$label"
  assert_contains "$output" "Last import source: $FIXTURE" "$label"
  assert_contains "$output" "Last import bytes: $FIXTURE_BYTES" "$label"
  assert_contains "$output" "Last verification expected: $EXPECTED_SHA256" "$label"
  assert_contains "$output" "Last verification actual: $EXPECTED_SHA256" "$label"
  assert_contains "$output" "Last verification passed: true" "$label"
  assert_contains "$output" "Last download URL: $DOWNLOAD_URL" "$label"
  assert_contains "$output" "Last download expected SHA-256: $EXPECTED_SHA256" "$label"
  assert_contains "$output" "Last download result: none" "$label"
}

stop_daemon() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
  if [[ "$PRESERVE_STORE" == "0" ]]; then
    rm -rf "$STORE"
  fi
}

trap stop_daemon EXIT

bridgevm create "$LOCAL_VM" --template "$TEMPLATE_ID" >/dev/null

local_boot_media="$(bridgevm boot-media "$LOCAL_VM")"
assert_missing_template_media "local missing boot-media" "$local_boot_media" "$LOCAL_VM" "$LOCAL_INSTALLER"

local_status="$(bridgevm media status "$LOCAL_VM")"
assert_missing_template_status "local missing media status" "$local_status" "$LOCAL_VM" "$LOCAL_INSTALLER"

local_prepare="$(bridgevm prepare-run "$LOCAL_VM")"
assert_contains "$local_prepare" "Engine: lightvm" "local missing prepare-run"
assert_contains "$local_prepare" "Dry run: true" "local missing prepare-run"
assert_contains "$local_prepare" "Launch spec: $LOCAL_LAUNCH_SPEC" "local missing prepare-run"
assert_contains "$local_prepare" "Launch ready: false" "local missing prepare-run"
assert_contains "$local_prepare" "missing-primary-disk" "local missing prepare-run"
assert_contains "$local_prepare" "$LOCAL_DISK" "local missing prepare-run"
assert_contains "$local_prepare" "missing-installer-image" "local missing prepare-run"
assert_contains "$local_prepare" "$LOCAL_INSTALLER" "local missing prepare-run"
assert_file_contains "$LOCAL_LAUNCH_SPEC" '"ready": false' "local missing launch spec"
assert_file_contains "$LOCAL_LAUNCH_SPEC" '"missing-installer-image"' "local missing launch spec"

assert_fails_contains \
  "local-verify-missing-media" \
  "failed to read boot media $LOCAL_INSTALLER" \
  bridgevm media verify "$LOCAL_VM" --sha256 "$EXPECTED_SHA256"
assert_no_launch_claim "$ASSERT_OUTPUT" "local verify missing media"

assert_fails_contains \
  "local-import-missing-source" \
  "failed to read source media $STORE/missing.iso" \
  bridgevm media import "$LOCAL_VM" --source "$STORE/missing.iso"
assert_no_launch_claim "$ASSERT_OUTPUT" "local import missing source"

assert_fails_contains \
  "local-missing-kind" \
  "boot media kind kernel is not present in this VM plan" \
  bridgevm media download-plan "$LOCAL_VM" --kind kernel --url "$DOWNLOAD_URL"
assert_no_launch_claim "$ASSERT_OUTPUT" "local missing media kind"

bridgevm create "$UNSAFE_VM" \
  --os ubuntu \
  --arch arm64 \
  --mode fast \
  --boot-mode linux-installer \
  --installer-image ../escaped-installer.iso >/dev/null

unsafe_boot_media="$(bridgevm boot-media "$UNSAFE_VM")"
assert_missing_template_media \
  "local unsafe boot-media status" \
  "$unsafe_boot_media" \
  "$UNSAFE_VM" \
  "$UNSAFE_INSTALLER"

assert_fails_contains \
  "local-unsafe-import-path" \
  "outside VM bundle" \
  bridgevm media import "$UNSAFE_VM" --source "$FIXTURE"
assert_no_launch_claim "$ASSERT_OUTPUT" "local unsafe import path"
[[ ! -e "$UNSAFE_ESCAPED_FILE" ]] || fail "unsafe import wrote outside the VM bundle"
[[ ! -e "$UNSAFE_BUNDLE/metadata/boot-media/installer-image.json" ]] \
  || fail "unsafe import wrote boot-media metadata"

assert_fails_contains \
  "local-unsafe-download-plan-path" \
  "outside VM bundle" \
  bridgevm media download-plan "$UNSAFE_VM" --url "$DOWNLOAD_URL"
assert_no_launch_claim "$ASSERT_OUTPUT" "local unsafe download-plan path"
[[ ! -e "$UNSAFE_BUNDLE/metadata/boot-media/installer-image-download.json" ]] \
  || fail "unsafe download-plan wrote boot-media metadata"

local_import="$(bridgevm media import "$LOCAL_VM" --source "$FIXTURE")"
assert_contains "$local_import" "Imported boot media for $LOCAL_VM" "local import"
assert_contains "$local_import" "Boot media kind: installer-image" "local import"
assert_contains "$local_import" "Source: $FIXTURE" "local import"
assert_contains "$local_import" "Destination: $LOCAL_INSTALLER" "local import"
assert_contains "$local_import" "Bytes: $FIXTURE_BYTES" "local import"
assert_contains "$local_import" "Replaced existing media: false" "local import"
cmp "$FIXTURE" "$LOCAL_INSTALLER" >/dev/null || fail "local import copied unexpected bytes"

assert_fails_contains \
  "local-verify-bad-sha" \
  "boot media SHA-256 mismatch for $LOCAL_INSTALLER" \
  bridgevm media verify "$LOCAL_VM" --sha256 "$BAD_SHA256"
assert_no_launch_claim "$ASSERT_OUTPUT" "local verify bad sha"

local_verify="$(bridgevm media verify "$LOCAL_VM" --sha256 "$EXPECTED_SHA256")"
assert_contains "$local_verify" "Verified boot media for $LOCAL_VM" "local verify"
assert_contains "$local_verify" "Boot media kind: installer-image" "local verify"
assert_contains "$local_verify" "Path: $LOCAL_INSTALLER" "local verify"
assert_contains "$local_verify" "Expected SHA-256: $EXPECTED_SHA256" "local verify"
assert_contains "$local_verify" "Actual SHA-256: $EXPECTED_SHA256" "local verify"
assert_contains "$local_verify" "Verified: true" "local verify"

local_plan="$(bridgevm media download-plan "$LOCAL_VM" --url "$DOWNLOAD_URL" --sha256 "$EXPECTED_SHA256")"
assert_contains "$local_plan" "Planned boot media download for $LOCAL_VM" "local download plan"
assert_contains "$local_plan" "URL: $DOWNLOAD_URL" "local download plan"
assert_contains "$local_plan" "Destination: $LOCAL_INSTALLER" "local download plan"
assert_contains "$local_plan" "Destination exists: true" "local download plan"
assert_contains "$local_plan" "Destination bytes: $FIXTURE_BYTES" "local download plan"
assert_contains "$local_plan" "Expected SHA-256: $EXPECTED_SHA256" "local download plan"
assert_contains "$local_plan" "Last import source: $FIXTURE" "local download plan"
assert_contains "$local_plan" "Last verification passed: true" "local download plan"
[[ ! -e "$LOCAL_BUNDLE/metadata/boot-media/installer-image-download-result.json" ]] \
  || fail "local download-plan executed a media download"

local_imported_status="$(bridgevm media status "$LOCAL_VM")"
assert_imported_template_status \
  "local imported media status" \
  "$local_imported_status" \
  "$LOCAL_VM" \
  "$LOCAL_INSTALLER"

local_run_after_import="$(bridgevm run "$LOCAL_VM")"
assert_contains "$local_run_after_import" "Launch ready: false" "local run after import"
assert_contains "$local_run_after_import" "missing-primary-disk" "local run after import"
assert_contains "$local_run_after_import" "$LOCAL_DISK" "local run after import"
assert_not_contains "$local_run_after_import" "missing-installer-image" "local run after import"

bridgevmd >"$DAEMON_LOG" 2>&1 &
DAEMON_PID=$!

for _ in {1..100}; do
  if [[ -S "$SOCKET" ]]; then
    break
  fi
  if ! kill -0 "$DAEMON_PID" 2>/dev/null; then
    fail "daemon exited before socket became ready"
  fi
  sleep 0.05
done
[[ -S "$SOCKET" ]] || fail "daemon socket was not ready"

bridgevm_socket create "$SOCKET_VM" --template "$TEMPLATE_ID" >/dev/null

socket_boot_media="$(bridgevm_socket boot-media "$SOCKET_VM")"
assert_missing_template_media \
  "socket missing boot-media" \
  "$socket_boot_media" \
  "$SOCKET_VM" \
  "$SOCKET_INSTALLER"

socket_status="$(bridgevm_socket media status "$SOCKET_VM")"
assert_missing_template_status \
  "socket missing media status" \
  "$socket_status" \
  "$SOCKET_VM" \
  "$SOCKET_INSTALLER"

assert_fails_contains \
  "socket-verify-missing-media" \
  "failed to read boot media $SOCKET_INSTALLER" \
  bridgevm_socket media verify "$SOCKET_VM" --sha256 "$EXPECTED_SHA256"
assert_no_launch_claim "$ASSERT_OUTPUT" "socket verify missing media"

socket_import="$(bridgevm_socket media import "$SOCKET_VM" --source "$FIXTURE")"
assert_contains "$socket_import" "Imported boot media for $SOCKET_VM" "socket import"
assert_contains "$socket_import" "Destination: $SOCKET_INSTALLER" "socket import"
cmp "$FIXTURE" "$SOCKET_INSTALLER" >/dev/null || fail "socket import copied unexpected bytes"

socket_verify="$(bridgevm_socket media verify "$SOCKET_VM" --sha256 "$EXPECTED_SHA256")"
assert_contains "$socket_verify" "Verified boot media for $SOCKET_VM" "socket verify"
assert_contains "$socket_verify" "Verified: true" "socket verify"

socket_plan="$(bridgevm_socket media download-plan "$SOCKET_VM" --url "$DOWNLOAD_URL" --sha256 "$EXPECTED_SHA256")"
assert_contains "$socket_plan" "Planned boot media download for $SOCKET_VM" "socket download plan"
assert_contains "$socket_plan" "URL: $DOWNLOAD_URL" "socket download plan"
assert_contains "$socket_plan" "Destination exists: true" "socket download plan"
[[ ! -e "$SOCKET_BUNDLE/metadata/boot-media/installer-image-download-result.json" ]] \
  || fail "socket download-plan executed a media download"

socket_imported_status="$(bridgevm_socket media status "$SOCKET_VM")"
assert_imported_template_status \
  "socket imported media status" \
  "$socket_imported_status" \
  "$SOCKET_VM" \
  "$SOCKET_INSTALLER"

PRESERVE_STORE=0
echo "PASS: Fast Mode template boot-media CLI/socket smoke ($STORE)"
