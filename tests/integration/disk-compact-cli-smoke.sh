#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-disk-compact.XXXXXX")"
FAKE_BIN="$STORE/bin"
VM_LOCAL="legacy-disk-local"
VM_SOCKET="legacy-disk-socket"
VM_MISSING="legacy-disk-missing"
VM_CONVERT_FAIL="legacy-disk-convert-fail"

mkdir -p "$FAKE_BIN"

cat >"$FAKE_BIN/qemu-img" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

case "${1:-}" in
  create)
    path="${@: -2:1}"
    mkdir -p "$(dirname "$path")"
    head -c 4096 /dev/zero >"$path"
    echo "created $path"
    ;;
  info)
    path="${@: -1}"
    bytes="$(wc -c <"$path" | tr -d ' ')"
    printf '{"filename":"%s","format":"qcow2","virtual-size":8589934592,"actual-size":%s}\n' "$path" "$bytes"
    ;;
  convert)
    if [[ "${BRIDGEVM_FAKE_QEMU_IMG_CONVERT_FAIL:-}" == "1" ]]; then
      echo "forced qemu-img convert failure for ${4:-<missing-source>}" >&2
      exit 7
    fi
    if [[ "${2:-}" != "-O" ]]; then
      echo "expected convert -O <format> <source> <output>" >&2
      exit 2
    fi
    source="${4:-}"
    output="${5:-}"
    [[ -f "$source" ]] || {
      echo "missing source $source" >&2
      exit 3
    }
    mkdir -p "$(dirname "$output")"
    printf 'compacted qcow2\n' >"$output"
    echo "compacted $source -> $output"
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

line_value() {
  local name="$1"
  local output="$2"
  printf '%s\n' "$output" | sed -n "s/^$name: //p" | head -n 1
}

stop_daemon() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
}

assert_compaction_contract() {
  local label="$1"
  local vm="$2"
  local output="$3"
  local bundle="$STORE/vms/$vm.vmbridge"
  local metadata="$bundle/metadata/last-disk-compact.json"
  local active="$bundle/disks/root.qcow2"

  assert_contains "$output" "Disk compact command: qemu-img convert -O qcow2" "$label"
  assert_contains "$output" "Disk compact status:" "$label"
  assert_contains "$output" "Disk compact backup:" "$label"
  assert_contains "$output" "Disk compact original bytes: 4096" "$label"
  assert_contains "$output" "Disk compact compacted bytes: 16" "$label"
  assert_contains "$output" "Disk compact stdout: compacted" "$label"
  assert_contains "$output" "Active disk: $active" "$label"

  [[ -f "$metadata" ]] || fail "$label metadata missing: $metadata"
  grep -q '"command"' "$metadata" || fail "$label metadata omitted command"
  grep -q '"backup_path"' "$metadata" || fail "$label metadata omitted backup_path"
  grep -q '"compacted_size_bytes": 16' "$metadata" \
    || fail "$label metadata omitted compacted size"
  grep -q '"original_size_bytes": 4096' "$metadata" \
    || fail "$label metadata omitted original size"

  local backup
  backup="$(line_value "Disk compact backup" "$output")"
  [[ -f "$backup" ]] || fail "$label backup missing: $backup"
  [[ "$(wc -c <"$backup" | tr -d ' ')" == "4096" ]] \
    || fail "$label backup size changed"
  [[ "$(cat "$active")" == "compacted qcow2" ]] \
    || fail "$label active disk was not replaced by compacted output"
}

trap stop_daemon EXIT

bridgevm create "$VM_LOCAL" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
bridgevm disk prepare "$VM_LOCAL" >/dev/null
bridgevm disk create "$VM_LOCAL" >/dev/null
bridgevm disk inspect "$VM_LOCAL" >/dev/null
local_output="$(bridgevm disk compact "$VM_LOCAL")"
assert_compaction_contract "local CLI compact" "$VM_LOCAL" "$local_output"

bridgevm create "$VM_MISSING" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
missing_disk="$STORE/vms/$VM_MISSING.vmbridge/disks/root.qcow2"
assert_fails_contains \
  "missing-active-disk-compact-rejected" \
  "primary disk is missing: $missing_disk" \
  bridgevm disk compact "$VM_MISSING"
assert_file_not_exists \
  "$STORE/vms/$VM_MISSING.vmbridge/metadata/last-disk-compact.json" \
  "missing active disk compact rejection"

bridgevm create "$VM_CONVERT_FAIL" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
bridgevm disk create "$VM_CONVERT_FAIL" >/dev/null
convert_fail_disk="$STORE/vms/$VM_CONVERT_FAIL.vmbridge/disks/root.qcow2"
convert_fail_before="$(wc -c <"$convert_fail_disk" | tr -d ' ')"
assert_fails_contains \
  "qemu-img-convert-failure" \
  "disk compaction command failed" \
  env BRIDGEVM_FAKE_QEMU_IMG_CONVERT_FAIL=1 cargo run --quiet -p bridgevm-cli -- --store "$STORE" disk compact "$VM_CONVERT_FAIL"
convert_fail_output="$ASSERT_OUTPUT"
assert_contains "$convert_fail_output" "qemu-img" "qemu-img convert failure"
assert_contains "$convert_fail_output" "convert" "qemu-img convert failure"
assert_contains "$convert_fail_output" "forced qemu-img convert failure" "qemu-img convert failure"
[[ "$(wc -c <"$convert_fail_disk" | tr -d ' ')" == "$convert_fail_before" ]] \
  || fail "qemu-img convert failure changed active disk"
assert_file_not_exists \
  "$STORE/vms/$VM_CONVERT_FAIL.vmbridge/metadata/last-disk-compact.json" \
  "qemu-img convert failure"

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
bridgevm_socket disk prepare "$VM_SOCKET" >/dev/null
bridgevm_socket disk create "$VM_SOCKET" >/dev/null
bridgevm_socket disk inspect "$VM_SOCKET" >/dev/null
socket_output="$(bridgevm_socket disk compact "$VM_SOCKET")"
assert_compaction_contract "socket compact" "$VM_SOCKET" "$socket_output"

echo "PASS: disk compaction CLI/socket integration smoke ($STORE)"
