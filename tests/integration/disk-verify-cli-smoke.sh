#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-disk-verify.XXXXXX")"
FAKE_BIN="$STORE/bin"
VM_LOCAL="legacy-verify-local"
VM_SOCKET="legacy-verify-socket"
VM_RAW="legacy-verify-raw"
VM_MISSING="legacy-verify-missing"
VM_CHECK_FAIL="legacy-verify-check-fail"

mkdir -p "$FAKE_BIN"

cat >"$FAKE_BIN/qemu-img" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

case "${1:-}" in
  create)
    path="${@: -2:1}"
    mkdir -p "$(dirname "$path")"
    printf 'fake qcow2\n' >"$path"
    echo "created $path"
    ;;
  check)
    path="${@: -1}"
    [[ -f "$path" ]] || {
      echo "missing disk $path" >&2
      exit 3
    }
    if [[ "${BRIDGEVM_FAKE_QEMU_IMG_CHECK_FAIL:-}" == "1" ]]; then
      echo "forced qemu-img check failure for $path" >&2
      exit 7
    fi
    bytes="$(wc -c <"$path" | tr -d ' ')"
    printf '{"filename":"%s","format":"qcow2","check-errors":0,"image-end-offset":%s}\n' "$path" "$bytes"
    ;;
  *)
    echo "unsupported qemu-img invocation: $*" >&2
    exit 64
    ;;
esac
SH
chmod +x "$FAKE_BIN/qemu-img"

export PATH="$FAKE_BIN:$PATH"

bridgevm() {
  cargo run --quiet -p bridgevm-cli -- --store "$STORE" "$@"
}

bridgevmd() {
  cargo run --quiet -p bridgevm-daemon -- --store "$STORE"
}

bridgevm_socket() {
  cargo run --quiet -p bridgevm-cli -- --socket "$SOCKET" "$@"
}

bridgevm_check_fail() {
  BRIDGEVM_FAKE_QEMU_IMG_CHECK_FAIL=1 bridgevm "$@"
}

fail() {
  echo "FAIL: $*" >&2
  echo "Store preserved at $STORE" >&2
  if [[ -n "${DAEMON_LOG:-}" && -f "$DAEMON_LOG" ]]; then
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

assert_file_not_exists() {
  local file="$1"
  local label="$2"
  [[ ! -e "$file" ]] || fail "$label unexpectedly created $file"
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

stop_daemon() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
}

assert_verify_contract() {
  local label="$1"
  local vm="$2"
  local output="$3"
  local bundle="$STORE/vms/$vm.vmbridge"
  local metadata="$bundle/metadata/last-disk-verify.json"
  local active="$bundle/disks/root.qcow2"
  local bytes
  bytes="$(wc -c <"$active" | tr -d ' ')"

  assert_contains "$output" "Disk verify command: qemu-img check --output=json $active" "$label"
  assert_contains "$output" "Disk verify status:" "$label"
  assert_contains "$output" "Active disk: $active" "$label"
  assert_contains "$output" "Active disk source: primary" "$label"
  assert_contains "$output" '"check-errors": 0' "$label"
  assert_contains "$output" "\"image-end-offset\": $bytes" "$label"

  [[ -f "$metadata" ]] || fail "$label metadata missing: $metadata"
  grep -q '"command"' "$metadata" || fail "$label metadata omitted command"
  grep -q '"check-errors": 0' "$metadata" || fail "$label metadata omitted check-errors"
  grep -q "\"image-end-offset\": $bytes" "$metadata" \
    || fail "$label metadata omitted image-end-offset"
  grep -q '"active_disk"' "$metadata" || fail "$label metadata omitted active disk"
}

trap stop_daemon EXIT

bridgevm create "$VM_LOCAL" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
bridgevm disk create "$VM_LOCAL" >/dev/null
local_output="$(bridgevm disk verify "$VM_LOCAL")"
assert_verify_contract "local CLI verify" "$VM_LOCAL" "$local_output"

bridgevm create "$VM_RAW" --os ubuntu --arch x86_64 --mode compatibility --disk 1MiB >/dev/null
raw_bundle="$STORE/vms/$VM_RAW.vmbridge"
raw_disk="$raw_bundle/disks/root.raw"
perl -0pi -e 's#path: disks/root\.qcow2#path: disks/root.raw#; s#format: qcow2#format: raw#' \
  "$raw_bundle/manifest.yaml"
assert_fails_contains \
  "raw-verify-rejected" \
  "disk verification requires qemu-img-managed formats; raw disk is prepared directly: $raw_disk" \
  bridgevm disk verify "$VM_RAW"
raw_output="$ASSERT_OUTPUT"
assert_not_contains "$raw_output" "Disk verify command:" "raw verify rejection"
assert_not_contains "$raw_output" "forced qemu-img check failure" "raw verify rejection"
[[ -f "$raw_disk" ]] || fail "raw verify rejection did not prepare raw disk"
assert_file_not_exists \
  "$raw_bundle/metadata/last-disk-verify.json" \
  "raw verify rejection"

bridgevm create "$VM_MISSING" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
missing_disk="$STORE/vms/$VM_MISSING.vmbridge/disks/root.qcow2"
assert_fails_contains \
  "missing-active-disk-rejected" \
  "primary disk is missing: $missing_disk" \
  bridgevm disk verify "$VM_MISSING"
missing_output="$ASSERT_OUTPUT"
assert_not_contains "$missing_output" "Disk verify command:" "missing active disk rejection"
assert_file_not_exists \
  "$STORE/vms/$VM_MISSING.vmbridge/metadata/last-disk-verify.json" \
  "missing active disk rejection"

bridgevm create "$VM_CHECK_FAIL" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
bridgevm disk create "$VM_CHECK_FAIL" >/dev/null
assert_fails_contains \
  "qemu-img-check-failure" \
  "disk verification command failed" \
  bridgevm_check_fail disk verify "$VM_CHECK_FAIL"
check_fail_output="$ASSERT_OUTPUT"
assert_contains "$check_fail_output" "qemu-img" "qemu-img check failure"
assert_contains "$check_fail_output" "check" "qemu-img check failure"
assert_contains "$check_fail_output" "forced qemu-img check failure" "qemu-img check failure"
assert_file_not_exists \
  "$STORE/vms/$VM_CHECK_FAIL.vmbridge/metadata/last-disk-verify.json" \
  "qemu-img check failure"

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

bridgevm create "$VM_SOCKET" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
bridgevm_socket disk create "$VM_SOCKET" >/dev/null
socket_output="$(bridgevm_socket disk verify "$VM_SOCKET")"
assert_verify_contract "socket verify" "$VM_SOCKET" "$socket_output"

echo "PASS: disk verification CLI/socket integration smoke ($STORE)"
