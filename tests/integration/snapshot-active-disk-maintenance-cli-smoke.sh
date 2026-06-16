#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-snapshot-active-maintenance.XXXXXX")"
FAKE_BIN="$STORE/bin"
VM_LOCAL="snapshot-active-maintenance-local"
VM_SOCKET="snapshot-active-maintenance-socket"
SNAPSHOT_NAME="maintenance-overlay"
DAEMON_PID=""
PRESERVE_STORE=1

mkdir -p "$FAKE_BIN"

cat >"$FAKE_BIN/qemu-img" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

case "${1:-}" in
  create)
    backing=""
    for ((i = 1; i <= $#; i++)); do
      if [[ "${!i}" == "-b" ]]; then
        next=$((i + 1))
        backing="${!next}"
      fi
    done

    if [[ -n "$backing" ]]; then
      path="${@: -1}"
      [[ -f "$backing" ]] || {
        echo "missing backing $backing" >&2
        exit 3
      }
      mkdir -p "$(dirname "$path")"
      printf 'fake active snapshot overlay backed by %s\n' "$backing" >"$path"
    else
      path="${@: -2:1}"
      mkdir -p "$(dirname "$path")"
      head -c 4096 /dev/zero >"$path"
    fi

    echo "created $path"
    ;;
  check)
    path="${@: -1}"
    [[ -f "$path" ]] || {
      echo "missing disk $path" >&2
      exit 4
    }
    bytes="$(wc -c <"$path" | tr -d ' ')"
    printf '{"filename":"%s","format":"qcow2","check-errors":0,"image-end-offset":%s}\n' "$path" "$bytes"
    ;;
  info)
    path="${@: -1}"
    [[ -f "$path" ]] || {
      echo "missing disk $path" >&2
      exit 5
    }
    bytes="$(wc -c <"$path" | tr -d ' ')"
    printf '{"filename":"%s","format":"qcow2","virtual-size":8589934592,"actual-size":%s}\n' "$path" "$bytes"
    ;;
  convert)
    if [[ "${2:-}" != "-O" ]]; then
      echo "expected convert -O <format> <source> <output>" >&2
      exit 2
    fi
    source="${4:-}"
    output="${5:-}"
    [[ -f "$source" ]] || {
      echo "missing source $source" >&2
      exit 6
    }
    mkdir -p "$(dirname "$output")"
    printf 'compacted active snapshot overlay\n' >"$output"
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

line_value() {
  local name="$1"
  local output="$2"
  printf '%s\n' "$output" | sed -n "s/^$name: //p" | head -n 1
}

assert_json_active_disk() {
  local label="$1"
  local metadata="$2"
  local expected_path="$3"

  python3 - "$metadata" "$expected_path" <<'PY' \
    || fail "$label metadata did not record the snapshot overlay active disk"
import json
import sys

metadata_path, expected_path = sys.argv[1:3]
with open(metadata_path, "r", encoding="utf-8") as handle:
    metadata = json.load(handle)
active = metadata.get("active_disk") or {}
checks = [
    active.get("source") == "snapshot-overlay",
    active.get("snapshot") == "maintenance-overlay",
    active.get("path") == expected_path,
    active.get("exists") is True,
]
sys.exit(0 if all(checks) else 1)
PY
}

assert_active_maintenance_contract() {
  local label="$1"
  local vm="$2"
  local runner="$3"
  local bundle="$STORE/vms/$vm.vmbridge"
  local primary="$bundle/disks/root.qcow2"
  local overlay="$bundle/disks/snapshots/$SNAPSHOT_NAME.qcow2"
  local active_metadata="$bundle/metadata/active-disk.json"
  local verify_metadata="$bundle/metadata/last-disk-verify.json"
  local compact_metadata="$bundle/metadata/last-disk-compact.json"

  "$runner" create "$vm" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
  "$runner" disk create "$vm" >/dev/null
  "$runner" snapshot create "$vm" "$SNAPSHOT_NAME" --kind disk >/dev/null
  "$runner" snapshot disk-create "$vm" "$SNAPSHOT_NAME" >/dev/null

  [[ -f "$primary" ]] || fail "$label primary disk missing"
  [[ -f "$overlay" ]] || fail "$label snapshot overlay missing"
  grep -q "\"path\": \"$overlay\"" "$active_metadata" \
    || fail "$label active disk metadata did not select overlay"

  verify_output="$("$runner" disk verify "$vm")"
  overlay_bytes_before="$(wc -c <"$overlay" | tr -d ' ')"
  assert_contains "$verify_output" "Disk verify command: qemu-img check --output=json $overlay" "$label verify"
  assert_contains "$verify_output" "Active disk: $overlay" "$label verify"
  assert_contains "$verify_output" "Active disk source: snapshot-overlay" "$label verify"
  assert_contains "$verify_output" "Active disk snapshot: $SNAPSHOT_NAME" "$label verify"
  assert_contains "$verify_output" '"check-errors": 0' "$label verify"
  assert_contains "$verify_output" "\"image-end-offset\": $overlay_bytes_before" "$label verify"

  [[ -f "$verify_metadata" ]] || fail "$label verify metadata missing"
  grep -q "$overlay" "$verify_metadata" || fail "$label verify metadata omitted overlay path"
  assert_json_active_disk "$label verify" "$verify_metadata" "$overlay"

  compact_output="$("$runner" disk compact "$vm")"
  compacted_bytes="$(wc -c <"$overlay" | tr -d ' ')"
  assert_contains "$compact_output" "Disk compact command: qemu-img convert -O qcow2 $overlay" "$label compact"
  assert_contains "$compact_output" "Active disk: $overlay" "$label compact"
  assert_contains "$compact_output" "Disk compact original bytes: $overlay_bytes_before" "$label compact"
  assert_contains "$compact_output" "Disk compact compacted bytes: $compacted_bytes" "$label compact"
  assert_contains "$compact_output" "Disk compact stdout: compacted $overlay ->" "$label compact"
  [[ "$(cat "$overlay")" == "compacted active snapshot overlay" ]] \
    || fail "$label compact did not replace the active overlay"

  backup="$(line_value "Disk compact backup" "$compact_output")"
  [[ -f "$backup" ]] || fail "$label compact backup missing: $backup"
  grep -q "fake active snapshot overlay backed by $primary" "$backup" \
    || fail "$label compact backup did not preserve previous overlay bytes"

  [[ -f "$compact_metadata" ]] || fail "$label compact metadata missing"
  grep -q "$overlay" "$compact_metadata" || fail "$label compact metadata omitted overlay path"
  grep -q "$backup" "$compact_metadata" || fail "$label compact metadata omitted backup path"
  assert_json_active_disk "$label compact" "$compact_metadata" "$overlay"

  chain_output="$("$runner" snapshot chain "$vm")"
  assert_contains "$chain_output" "Active disk source: snapshot-overlay" "$label chain"
  assert_contains "$chain_output" "Active disk snapshot: $SNAPSHOT_NAME" "$label chain"
  assert_contains "$chain_output" "Active disk: $overlay" "$label chain"
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

assert_active_maintenance_contract \
  "local active snapshot disk maintenance" \
  "$VM_LOCAL" \
  bridgevm

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

assert_active_maintenance_contract \
  "socket active snapshot disk maintenance" \
  "$VM_SOCKET" \
  bridgevm_socket

PRESERVE_STORE=0
echo "PASS: snapshot active-disk maintenance CLI/socket smoke ($STORE)"
