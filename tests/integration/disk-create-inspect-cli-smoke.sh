#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-disk-create-inspect.XXXXXX")"
FAKE_BIN="$STORE/bin"
VM_LOCAL="disk-create-inspect-local"
VM_SOCKET="disk-create-inspect-socket"
VM_RAW="disk-create-inspect-raw"
VM_MISSING="disk-create-inspect-missing"
VM_CREATE_FAIL="disk-create-inspect-create-fail"
VM_INSPECT_FAIL="disk-create-inspect-inspect-fail"
DAEMON_PID=""

mkdir -p "$FAKE_BIN"

cat >"$FAKE_BIN/qemu-img" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

case "${1:-}" in
  create)
    path="${@: -2:1}"
    if [[ "${BRIDGEVM_FAKE_QEMU_IMG_CREATE_FAIL:-}" == "1" ]]; then
      echo "forced qemu-img create failure for $path" >&2
      exit 9
    fi
    mkdir -p "$(dirname "$path")"
    printf 'fake qcow2 primary\n' >"$path"
    echo "created $path"
    ;;
  info)
    path="${@: -1}"
    [[ -f "$path" ]] || {
      echo "missing disk $path" >&2
      exit 3
    }
    if [[ "${BRIDGEVM_FAKE_QEMU_IMG_INFO_FAIL:-}" == "1" ]]; then
      echo "forced qemu-img info failure for $path" >&2
      exit 8
    fi
    bytes="$(wc -c <"$path" | tr -d ' ')"
    printf '{"filename":"%s","format":"qcow2","virtual-size":85899345920,"actual-size":%s}\n' "$path" "$bytes"
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

bridgevm_create_fail() {
  BRIDGEVM_FAKE_QEMU_IMG_CREATE_FAIL=1 bridgevm "$@"
}

bridgevm_info_fail() {
  BRIDGEVM_FAKE_QEMU_IMG_INFO_FAIL=1 bridgevm "$@"
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
    *) ;;
  esac
}

assert_file_exists() {
  local file="$1"
  local label="$2"
  [[ -f "$file" ]] || fail "$label missing file: $file"
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

assert_prepare_contract() {
  local label="$1"
  local vm="$2"
  local output="$3"
  local bundle="$STORE/vms/$vm.vmbridge"
  local active="$bundle/disks/root.qcow2"
  local metadata="$bundle/metadata/primary-disk.json"

  assert_contains "$output" "Disk: $active" "$label"
  assert_contains "$output" "Disk format: qcow2" "$label"
  assert_contains "$output" "Disk size: 80GiB" "$label"
  assert_contains "$output" "Disk ready: false" "$label"
  assert_contains "$output" "Disk created: false" "$label"
  assert_contains "$output" "Disk create command: qemu-img create -f qcow2 $active 80GiB" "$label"
  assert_file_exists "$metadata" "$label"
  grep -q '"format": "qcow2"' "$metadata" || fail "$label metadata omitted qcow2 format"
  grep -q '"exists": false' "$metadata" || fail "$label metadata did not preserve not-ready state"
  assert_file_not_exists "$active" "$label prepare boundary"
}

assert_create_contract() {
  local label="$1"
  local vm="$2"
  local output="$3"
  local bundle="$STORE/vms/$vm.vmbridge"
  local active="$bundle/disks/root.qcow2"
  local metadata="$bundle/metadata/last-disk-create.json"

  assert_contains "$output" "Disk create executed: true" "$label"
  assert_contains "$output" "Disk create command: qemu-img create -f qcow2 $active 80GiB" "$label"
  assert_contains "$output" "Disk create status:" "$label"
  assert_contains "$output" "Disk create stdout: created $active" "$label"
  assert_contains "$output" "Disk: $active" "$label"
  assert_contains "$output" "Disk ready: true" "$label"
  assert_contains "$output" "Disk created: false" "$label"
  assert_file_exists "$active" "$label"
  assert_file_exists "$metadata" "$label"
  grep -q '"executed": true' "$metadata" || fail "$label metadata did not record execution"
  grep -q '"stdout": "created ' "$metadata" || fail "$label metadata omitted qemu-img stdout"
}

assert_inspect_contract() {
  local label="$1"
  local vm="$2"
  local output="$3"
  local bundle="$STORE/vms/$vm.vmbridge"
  local active="$bundle/disks/root.qcow2"
  local metadata="$bundle/metadata/last-disk-inspect.json"
  local bytes
  bytes="$(wc -c <"$active" | tr -d ' ')"

  assert_contains "$output" "Disk inspect command: qemu-img info --output=json $active" "$label"
  assert_contains "$output" "Disk inspect status:" "$label"
  assert_contains "$output" "Disk inspect duration:" "$label"
  assert_contains "$output" "Disk: $active" "$label"
  assert_contains "$output" "Disk ready: true" "$label"
  assert_contains "$output" '"format": "qcow2"' "$label"
  assert_contains "$output" '"virtual-size": 85899345920' "$label"
  assert_contains "$output" "\"actual-size\": $bytes" "$label"
  assert_file_exists "$metadata" "$label"
  grep -q '"command"' "$metadata" || fail "$label metadata omitted command"
  grep -q '"virtual-size": 85899345920' "$metadata" \
    || fail "$label metadata omitted virtual size"
  grep -q "\"actual-size\": $bytes" "$metadata" \
    || fail "$label metadata omitted actual size"
}

trap stop_daemon EXIT

bridgevm create "$VM_LOCAL" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
prepare_output="$(bridgevm disk prepare "$VM_LOCAL")"
assert_prepare_contract "local prepare" "$VM_LOCAL" "$prepare_output"
create_output="$(bridgevm disk create "$VM_LOCAL")"
assert_create_contract "local create" "$VM_LOCAL" "$create_output"
inspect_output="$(bridgevm disk inspect "$VM_LOCAL")"
assert_inspect_contract "local inspect" "$VM_LOCAL" "$inspect_output"

bridgevm create "$VM_RAW" --os ubuntu --arch x86_64 --mode compatibility --disk 1MiB >/dev/null
raw_bundle="$STORE/vms/$VM_RAW.vmbridge"
raw_disk="$raw_bundle/disks/root.raw"
perl -0pi -e 's#path: disks/root\.qcow2#path: disks/root.raw#; s#format: qcow2#format: raw#' \
  "$raw_bundle/manifest.yaml"
raw_prepare="$(bridgevm disk prepare "$VM_RAW")"
assert_contains "$raw_prepare" "Disk: $raw_disk" "raw prepare"
assert_contains "$raw_prepare" "Disk format: raw" "raw prepare"
assert_contains "$raw_prepare" "Disk ready: true" "raw prepare"
assert_contains "$raw_prepare" "Disk created: true" "raw prepare"
assert_not_contains "$raw_prepare" "Disk create command:" "raw prepare"
[[ "$(wc -c <"$raw_disk" | tr -d ' ')" == "1048576" ]] \
  || fail "raw prepare created the wrong disk size"
raw_create="$(bridgevm disk create "$VM_RAW")"
assert_contains "$raw_create" "Disk create executed: false" "raw create"
assert_not_contains "$raw_create" "qemu-img" "raw create"
assert_file_exists "$raw_bundle/metadata/last-disk-create.json" "raw create"

bridgevm create "$VM_MISSING" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
missing_disk="$STORE/vms/$VM_MISSING.vmbridge/disks/root.qcow2"
assert_fails_contains \
  "missing-inspect-rejected" \
  "primary disk is missing: $missing_disk" \
  bridgevm disk inspect "$VM_MISSING"
missing_output="$ASSERT_OUTPUT"
assert_not_contains "$missing_output" "Disk inspect command:" "missing inspect rejection"
assert_file_not_exists \
  "$STORE/vms/$VM_MISSING.vmbridge/metadata/last-disk-inspect.json" \
  "missing inspect rejection"

bridgevm create "$VM_CREATE_FAIL" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
create_fail_disk="$STORE/vms/$VM_CREATE_FAIL.vmbridge/disks/root.qcow2"
assert_fails_contains \
  "qemu-img-create-failure" \
  "disk creation command failed" \
  bridgevm_create_fail disk create "$VM_CREATE_FAIL"
create_fail_output="$ASSERT_OUTPUT"
assert_contains "$create_fail_output" "forced qemu-img create failure" "qemu-img create failure"
assert_file_not_exists "$create_fail_disk" "qemu-img create failure"
assert_file_not_exists \
  "$STORE/vms/$VM_CREATE_FAIL.vmbridge/metadata/last-disk-create.json" \
  "qemu-img create failure"

bridgevm create "$VM_INSPECT_FAIL" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
bridgevm disk create "$VM_INSPECT_FAIL" >/dev/null
assert_fails_contains \
  "qemu-img-info-failure" \
  "disk inspection command failed" \
  bridgevm_info_fail disk inspect "$VM_INSPECT_FAIL"
info_fail_output="$ASSERT_OUTPUT"
assert_contains "$info_fail_output" "forced qemu-img info failure" "qemu-img info failure"
assert_file_not_exists \
  "$STORE/vms/$VM_INSPECT_FAIL.vmbridge/metadata/last-disk-inspect.json" \
  "qemu-img info failure"

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
socket_prepare="$(bridgevm_socket disk prepare "$VM_SOCKET")"
assert_prepare_contract "socket prepare" "$VM_SOCKET" "$socket_prepare"
socket_create="$(bridgevm_socket disk create "$VM_SOCKET")"
assert_create_contract "socket create" "$VM_SOCKET" "$socket_create"
socket_inspect="$(bridgevm_socket disk inspect "$VM_SOCKET")"
assert_inspect_contract "socket inspect" "$VM_SOCKET" "$socket_inspect"

echo "PASS: disk create/inspect CLI/socket integration smoke ($STORE)"
