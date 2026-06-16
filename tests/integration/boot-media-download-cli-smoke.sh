#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-media-download.XXXXXX")"
LOCAL_VM="media-download-local"
SOCKET_VM="media-download-socket"
FIXTURE="$STORE/source-installer.iso"
EXPECTED_SHA256="$(
  printf "bridgevm boot media fixture\n" >"$FIXTURE"
  shasum -a 256 "$FIXTURE" | awk '{print $1}'
)"
BAD_SHA256="0000000000000000000000000000000000000000000000000000000000000000"
FIXTURE_BYTES="$(wc -c <"$FIXTURE" | tr -d ' ')"
HTTP_PORT="$(
  python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()'
)"
FIXTURE_URL="http://127.0.0.1:$HTTP_PORT/source-installer.iso"

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
  if [[ -n "${DAEMON_LOG:-}" && -f "$DAEMON_LOG" ]]; then
    echo "Daemon log: $DAEMON_LOG" >&2
  fi
  exit 1
}

cleanup() {
  if [[ -n "${HTTP_PID:-}" ]]; then
    kill "$HTTP_PID" 2>/dev/null || true
    wait "$HTTP_PID" 2>/dev/null || true
  fi
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
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

assert_file_contains() {
  local file="$1"
  local needle="$2"
  local label="$3"
  [[ -f "$file" ]] || fail "$label missing file $file"
  grep -Fq "$needle" "$file" || fail "$label missing '$needle' in $file"
}

assert_download_contract() {
  local label="$1"
  local vm="$2"
  local output="$3"
  local status_output="$4"
  local destination="$STORE/vms/$vm.vmbridge/media/ubuntu.iso"
  local result_metadata="$STORE/vms/$vm.vmbridge/metadata/boot-media/installer-image-download-result.json"
  local plan_metadata="$STORE/vms/$vm.vmbridge/metadata/boot-media/installer-image-download.json"

  assert_contains "$output" "Downloaded boot media for $vm" "$label download output"
  assert_contains "$output" "Boot media kind: installer-image" "$label download output"
  assert_contains "$output" "URL: $FIXTURE_URL" "$label download output"
  assert_contains "$output" "Destination: $destination" "$label download output"
  assert_contains "$output" "Downloaded: true" "$label download output"
  assert_contains "$output" "Replaced existing media: false" "$label download output"
  assert_contains "$output" "Bytes: $FIXTURE_BYTES" "$label download output"
  assert_contains "$output" "Expected SHA-256: $EXPECTED_SHA256" "$label download output"
  assert_contains "$output" "Actual SHA-256: $EXPECTED_SHA256" "$label download output"
  assert_contains "$output" "Verified: true" "$label download output"

  cmp "$FIXTURE" "$destination" >/dev/null || fail "$label destination content did not match fixture"
  assert_file_contains "$plan_metadata" "\"url\": \"$FIXTURE_URL\"" "$label download plan metadata"
  assert_file_contains "$plan_metadata" "\"expected_sha256\": \"$EXPECTED_SHA256\"" "$label download plan metadata"
  assert_file_contains "$result_metadata" '"downloaded": true' "$label download result metadata"
  assert_file_contains "$result_metadata" '"verified": true' "$label download result metadata"
  assert_file_contains "$result_metadata" "\"bytes\": $FIXTURE_BYTES" "$label download result metadata"
  assert_file_contains "$result_metadata" "\"actual_sha256\": \"$EXPECTED_SHA256\"" "$label download result metadata"
  assert_file_contains "$result_metadata" '"curl"' "$label download result metadata"
  [[ ! -e "$STORE/vms/$vm.vmbridge/media/.ubuntu.iso.download" ]] \
    || fail "$label left the temporary download file behind"

  assert_contains "$status_output" "VM: $vm" "$label status output"
  assert_contains "$status_output" "Path: $destination" "$label status output"
  assert_contains "$status_output" "Exists: true" "$label status output"
  assert_contains "$status_output" "Last download URL: $FIXTURE_URL" "$label status output"
  assert_contains "$status_output" "Last download expected SHA-256: $EXPECTED_SHA256" "$label status output"
  assert_contains "$status_output" "Last download completed: true" "$label status output"
  assert_contains "$status_output" "Last download bytes: $FIXTURE_BYTES" "$label status output"
}

assert_checksum_failure_preserves_existing_media() {
  local vm="$1"
  local destination="$STORE/vms/$vm.vmbridge/media/ubuntu.iso"
  local result_metadata="$STORE/vms/$vm.vmbridge/metadata/boot-media/installer-image-download-result.json"

  local bad_plan_output
  bad_plan_output="$(bridgevm media download-plan "$vm" --url "$FIXTURE_URL" --sha256 "$BAD_SHA256")"
  assert_contains "$bad_plan_output" "Planned boot media download for $vm" "checksum failure plan output"
  assert_contains "$bad_plan_output" "Destination exists: true" "checksum failure plan output"
  assert_contains "$bad_plan_output" "Destination bytes: $FIXTURE_BYTES" "checksum failure plan output"

  local failed_download_output
  if failed_download_output="$(bridgevm media download "$vm" 2>&1)"; then
    fail "checksum mismatch download unexpectedly succeeded"
  fi
  assert_contains "$failed_download_output" "downloaded boot media SHA-256 mismatch" \
    "checksum failure download output"

  cmp "$FIXTURE" "$destination" >/dev/null \
    || fail "checksum failure replaced existing media"
  assert_file_contains "$result_metadata" '"downloaded": false' \
    "checksum failure result metadata"
  assert_file_contains "$result_metadata" '"verified": false' \
    "checksum failure result metadata"
  assert_file_contains "$result_metadata" '"replaced": true' \
    "checksum failure result metadata"
  assert_file_contains "$result_metadata" "\"bytes\": $FIXTURE_BYTES" \
    "checksum failure result metadata"
  assert_file_contains "$result_metadata" "\"expected_sha256\": \"$BAD_SHA256\"" \
    "checksum failure result metadata"
  assert_file_contains "$result_metadata" "\"actual_sha256\": \"$EXPECTED_SHA256\"" \
    "checksum failure result metadata"

  local failed_status_output
  failed_status_output="$(bridgevm media status "$vm")"
  assert_contains "$failed_status_output" "Exists: true" "checksum failure status output"
  assert_contains "$failed_status_output" "Bytes: $FIXTURE_BYTES" "checksum failure status output"
  assert_contains "$failed_status_output" "Last download expected SHA-256: $BAD_SHA256" \
    "checksum failure status output"
  assert_contains "$failed_status_output" "Last download completed: false" \
    "checksum failure status output"
  assert_contains "$failed_status_output" "Last download bytes: $FIXTURE_BYTES" \
    "checksum failure status output"
}

create_fast_installer_vm() {
  local runner="$1"
  local vm="$2"
  "$runner" create "$vm" \
    --os ubuntu \
    --arch arm64 \
    --mode fast \
    --boot-mode linux-installer \
    --installer-image media/ubuntu.iso >/dev/null
}

trap cleanup EXIT

python3 -m http.server "$HTTP_PORT" --bind 127.0.0.1 --directory "$STORE" >"$STORE/http.log" 2>&1 &
HTTP_PID=$!

for _ in {1..100}; do
  if curl --silent --fail "$FIXTURE_URL" >/dev/null 2>&1; then
    break
  fi
  sleep 0.05
done

curl --silent --fail "$FIXTURE_URL" >/dev/null \
  || fail "loopback fixture server was not ready"

create_fast_installer_vm bridgevm "$LOCAL_VM"
local_plan_output="$(bridgevm media download-plan "$LOCAL_VM" --url "$FIXTURE_URL" --sha256 "$EXPECTED_SHA256")"
assert_contains "$local_plan_output" "Planned boot media download for $LOCAL_VM" "local download plan output"
assert_contains "$local_plan_output" "Destination exists: false" "local download plan output"
local_download_output="$(bridgevm media download "$LOCAL_VM")"
local_status_output="$(bridgevm media status "$LOCAL_VM")"
assert_download_contract "local CLI" "$LOCAL_VM" "$local_download_output" "$local_status_output"
assert_checksum_failure_preserves_existing_media "$LOCAL_VM"

SOCKET="$STORE/run/bridgevmd.sock"
DAEMON_LOG="$STORE/bridgevmd.log"

bridgevmd >"$DAEMON_LOG" 2>&1 &
DAEMON_PID=$!

for _ in {1..100}; do
  if [[ -S "$SOCKET" ]]; then
    break
  fi
  sleep 0.05
done

[[ -S "$SOCKET" ]] || fail "daemon socket was not ready"

create_fast_installer_vm bridgevm_socket "$SOCKET_VM"
socket_plan_output="$(bridgevm_socket media download-plan "$SOCKET_VM" --url "$FIXTURE_URL" --sha256 "$EXPECTED_SHA256")"
assert_contains "$socket_plan_output" "Planned boot media download for $SOCKET_VM" "socket download plan output"
assert_contains "$socket_plan_output" "Destination exists: false" "socket download plan output"
socket_download_output="$(bridgevm_socket media download "$SOCKET_VM")"
socket_status_output="$(bridgevm_socket media status "$SOCKET_VM")"
assert_download_contract "socket API" "$SOCKET_VM" "$socket_download_output" "$socket_status_output"

echo "PASS: boot media download CLI/socket integration smoke ($STORE)"
