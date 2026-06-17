#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-hostonly.XXXXXX")"
VM_NAME="legacy-hostonly"
BUNDLE="$STORE/vms/$VM_NAME.vmbridge"
MANIFEST="$BUNDLE/manifest.yaml"
FAKE_BIN="$STORE/bin"

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

assert_not_contains() {
  local haystack="$1"
  local needle="$2"
  local label="$3"
  case "$haystack" in
    *"$needle"*) fail "$label unexpectedly included '$needle'; got: $haystack" ;;
  esac
}

assert_host_only_qemu_args() {
  local output="$1"
  local label="$2"

  assert_contains "$output" "-netdev" "$label"
  assert_contains "$output" "vmnet-host,id=net0" "$label"
  assert_not_contains "$output" "hostfwd=" "$label"
  assert_not_contains "$output" "qemu-host-only-network-unimplemented" "$label"
  assert_not_contains "$output" "host-only networking is not implemented" "$label"
}

assert_port_add_rejected() {
  local label="$1"
  shift

  local stdout="$STORE/$label.stdout"
  local stderr="$STORE/$label.stderr"
  if "$@" >"$stdout" 2>"$stderr"; then
    fail "$label unexpectedly accepted a host-only port forward"
  fi

  local output
  output="$(cat "$stdout" "$stderr")"
  assert_contains "$output" "host-only networking does not support port forwarding" "$label"
}

assert_spawn_rejected() {
  local label="$1"
  shift

  local stdout="$STORE/$label.stdout"
  local stderr="$STORE/$label.stderr"
  if PATH="$FAKE_BIN:$PATH" "$@" >"$stdout" 2>"$stderr"; then
    fail "$label unexpectedly accepted host-only spawn"
  fi

  local output
  output="$(cat "$stdout" "$stderr")"
  assert_contains "$output" "qemu-host-only-requires-privilege" "$label"
  assert_contains "$output" "vmnet-host" "$label"
  assert_contains "$output" "com.apple.vm.networking" "$label"
  assert_not_contains "$output" "unexpected qemu spawn" "$label"
}

assert_manifest_has_no_forwards() {
  if grep -q "host: 2222" "$MANIFEST" || grep -q "guest: 22" "$MANIFEST"; then
    fail "host-only port-forward rejection still recorded a forwarding rule"
  fi
}

stop_daemon() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
}

trap stop_daemon EXIT

mkdir -p "$FAKE_BIN"
cat >"$FAKE_BIN/qemu-system-x86_64" <<'SH'
#!/usr/bin/env bash
echo "unexpected qemu spawn" >&2
exit 77
SH
chmod +x "$FAKE_BIN/qemu-system-x86_64"

bridgevm create "$VM_NAME" --os ubuntu --arch x86_64 --mode compatibility >/dev/null
perl -0pi -e 's/network:\n  mode: nat/network:\n  mode: host-only/' "$MANIFEST"
perl -0pi -e 's/path: disks\/root\.qcow2/path: disks\/root.raw/' "$MANIFEST"
perl -0pi -e 's/size: 64GiB/size: 1MiB/' "$MANIFEST"
perl -0pi -e 's/format: qcow2/format: raw/' "$MANIFEST"

grep -q "mode: host-only" "$MANIFEST" || fail "manifest was not switched to host-only"

local_qemu_args="$(bridgevm qemu-args "$VM_NAME")"
assert_host_only_qemu_args "$local_qemu_args" "local qemu-args"

assert_port_add_rejected "local-port-add" bridgevm port add "$VM_NAME" 2222:22
assert_manifest_has_no_forwards
assert_spawn_rejected "local-run-spawn" bridgevm run "$VM_NAME" --spawn

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

socket_qemu_args="$(bridgevm_socket qemu-args "$VM_NAME")"
assert_host_only_qemu_args "$socket_qemu_args" "socket qemu-args"

assert_port_add_rejected "socket-port-add" bridgevm_socket port add "$VM_NAME" 2222:22
assert_manifest_has_no_forwards
assert_spawn_rejected "socket-run-spawn" bridgevm_socket run "$VM_NAME" --spawn

echo "PASS: QEMU host-only CLI/socket integration smoke ($STORE)"
