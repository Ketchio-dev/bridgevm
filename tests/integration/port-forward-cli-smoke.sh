#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-pf.XXXXXX")"
VM_NAME="legacy-linux"
MANIFEST="$STORE/vms/$VM_NAME.vmbridge/manifest.yaml"
FAKE_BIN="$STORE/bin"
OPEN_MARKER="$STORE/open-invoked"

mkdir -p "$FAKE_BIN"
cat >"$FAKE_BIN/open" <<'SH'
#!/usr/bin/env bash
echo "$*" >"${BRIDGEVM_OPEN_MARKER:?}"
exit 42
SH
chmod +x "$FAKE_BIN/open"
export PATH="$FAKE_BIN:$PATH"
export BRIDGEVM_OPEN_MARKER="$OPEN_MARKER"

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
  local output="$1"
  local expected="$2"
  local label="$3"
  case "$output" in
    *"$expected"*) ;;
    *) fail "$label missing '$expected', got: $output" ;;
  esac
}

assert_fails_contains() {
  local label="$1"
  local needle="$2"
  shift 2

  local output
  if output="$("$@" 2>&1)"; then
    fail "$label unexpectedly succeeded: $output"
  fi
  assert_contains "$output" "$needle" "$label"
}

assert_open_not_invoked() {
  local label="$1"
  if [[ -e "$OPEN_MARKER" ]]; then
    fail "$label invoked host open command: $(cat "$OPEN_MARKER")"
  fi
}

stop_daemon() {
  if [[ -n "${DAEMON_PID:-}" ]]; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
}

trap stop_daemon EXIT

bridgevm create "$VM_NAME" --os ubuntu --arch x86_64 --mode compatibility >/dev/null

initial_ports="$(bridgevm port list "$VM_NAME")"
case "$initial_ports" in
  *"No port forwards configured"*) ;;
  *) fail "new VM should not have port forwards, got: $initial_ports" ;;
esac
assert_fails_contains \
  "local open missing forward" \
  "no host port is forwarded to guest port 80" \
  bridgevm open "$VM_NAME" 80 --scheme http
assert_open_not_invoked "local open missing forward"
assert_fails_contains \
  "local open invalid guest port" \
  "guest port must be between 1 and 65535" \
  bridgevm open "$VM_NAME" 0 --scheme http
assert_open_not_invoked "local open invalid guest port"

bridgevm port add "$VM_NAME" 2222:22 >/dev/null
bridgevm port add "$VM_NAME" 10080:80 >/dev/null
bridgevm port add "$VM_NAME" 18080:80 >/dev/null

grep -q "host: 2222" "$MANIFEST" || fail "manifest did not record host port 2222"
grep -q "guest: 22" "$MANIFEST" || fail "manifest did not record guest port 22"
grep -q "host: 10080" "$MANIFEST" || fail "manifest did not record host port 10080"
grep -q "host: 18080" "$MANIFEST" || fail "manifest did not record host port 18080"
grep -q "guest: 80" "$MANIFEST" || fail "manifest did not record guest port 80"

ports_after_add="$(bridgevm port list "$VM_NAME")"
case "$ports_after_add" in
  *"2222:22"*) ;;
  *) fail "port list did not show added forward, got: $ports_after_add" ;;
esac
assert_contains "$ports_after_add" "10080:80" "port list after add"
assert_contains "$ports_after_add" "18080:80" "port list after add"

open_plan="$(bridgevm open "$VM_NAME" 80 --scheme HTTPS)"
assert_contains "$open_plan" "Open target for $VM_NAME" "local open plan"
assert_contains "$open_plan" "Scheme: https" "local open plan"
assert_contains "$open_plan" "Host: 127.0.0.1" "local open plan"
assert_contains "$open_plan" "URL: https://127.0.0.1:10080" "local open plan"
assert_contains "$open_plan" "Guest port: 80" "local open plan"
assert_contains "$open_plan" "Host port: 10080" "local open plan"
assert_contains "$open_plan" "Command: open https://127.0.0.1:10080" "local open plan"
assert_open_not_invoked "local open plan"
assert_fails_contains \
  "local open invalid scheme" \
  "URL scheme must start with an ASCII letter" \
  bridgevm open "$VM_NAME" 80 --scheme 1http
assert_open_not_invoked "local open invalid scheme"

qemu_args_after_add="$(bridgevm qemu-args "$VM_NAME")"
case "$qemu_args_after_add" in
  *"hostfwd=tcp::2222-:22"*) ;;
  *) fail "qemu-args did not render hostfwd after add" ;;
esac
assert_contains "$qemu_args_after_add" "hostfwd=tcp::10080-:80" "qemu-args after add"
assert_contains "$qemu_args_after_add" "hostfwd=tcp::18080-:80" "qemu-args after add"

bridgevm port remove "$VM_NAME" 2222:22 >/dev/null
bridgevm port remove "$VM_NAME" 10080:80 >/dev/null
bridgevm port remove "$VM_NAME" 18080:80 >/dev/null

ports_after_remove="$(bridgevm port list "$VM_NAME")"
case "$ports_after_remove" in
  *"No port forwards configured"*) ;;
  *) fail "port list still showed forwards after remove, got: $ports_after_remove" ;;
esac

qemu_args_after_remove="$(bridgevm qemu-args "$VM_NAME")"
case "$qemu_args_after_remove" in
  *"hostfwd=tcp::2222-:22"*) fail "qemu-args still rendered removed hostfwd" ;;
  *"hostfwd=tcp::10080-:80"*) fail "qemu-args still rendered removed hostfwd" ;;
esac

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

socket_initial_ports="$(bridgevm_socket port list "$VM_NAME")"
case "$socket_initial_ports" in
  *"No port forwards configured"*) ;;
  *) fail "socket port list should start empty, got: $socket_initial_ports" ;;
esac
assert_fails_contains \
  "socket open missing forward" \
  "no host port is forwarded to guest port 443" \
  bridgevm_socket open "$VM_NAME" 443 --scheme https
assert_open_not_invoked "socket open missing forward"
assert_fails_contains \
  "socket open invalid guest port" \
  "guest port must be between 1 and 65535" \
  bridgevm_socket open "$VM_NAME" 0 --scheme https
assert_open_not_invoked "socket open invalid guest port"

bridgevm_socket port add "$VM_NAME" 2222:22 >/dev/null
bridgevm_socket port add "$VM_NAME" 10443:443 >/dev/null
bridgevm_socket port add "$VM_NAME" 18443:443 >/dev/null

socket_ports_after_add="$(bridgevm_socket port list "$VM_NAME")"
case "$socket_ports_after_add" in
  *"2222:22"*) ;;
  *) fail "socket port list did not show added forward, got: $socket_ports_after_add" ;;
esac
assert_contains "$socket_ports_after_add" "10443:443" "socket port list after add"
assert_contains "$socket_ports_after_add" "18443:443" "socket port list after add"

socket_open_plan="$(bridgevm_socket open "$VM_NAME" 443 --scheme bridgevm+https)"
assert_contains "$socket_open_plan" "Open target for $VM_NAME" "socket open plan"
assert_contains "$socket_open_plan" "Scheme: bridgevm+https" "socket open plan"
assert_contains "$socket_open_plan" "Host: 127.0.0.1" "socket open plan"
assert_contains "$socket_open_plan" "URL: bridgevm+https://127.0.0.1:10443" "socket open plan"
assert_contains "$socket_open_plan" "Guest port: 443" "socket open plan"
assert_contains "$socket_open_plan" "Host port: 10443" "socket open plan"
assert_contains "$socket_open_plan" "Command: open bridgevm+https://127.0.0.1:10443" "socket open plan"
assert_open_not_invoked "socket open plan"
assert_fails_contains \
  "socket open invalid scheme" \
  "URL scheme may only contain ASCII letters" \
  bridgevm_socket open "$VM_NAME" 443 --scheme "bridge vm"
assert_open_not_invoked "socket open invalid scheme"

socket_qemu_args_after_add="$(bridgevm_socket qemu-args "$VM_NAME")"
case "$socket_qemu_args_after_add" in
  *"hostfwd=tcp::2222-:22"*) ;;
  *) fail "socket qemu-args did not render hostfwd after add" ;;
esac
assert_contains "$socket_qemu_args_after_add" "hostfwd=tcp::10443-:443" "socket qemu-args after add"
assert_contains "$socket_qemu_args_after_add" "hostfwd=tcp::18443-:443" "socket qemu-args after add"

bridgevm_socket port remove "$VM_NAME" 2222:22 >/dev/null
bridgevm_socket port remove "$VM_NAME" 10443:443 >/dev/null
bridgevm_socket port remove "$VM_NAME" 18443:443 >/dev/null

socket_ports_after_remove="$(bridgevm_socket port list "$VM_NAME")"
case "$socket_ports_after_remove" in
  *"No port forwards configured"*) ;;
  *) fail "socket port list still showed forwards after remove, got: $socket_ports_after_remove" ;;
esac

socket_qemu_args_after_remove="$(bridgevm_socket qemu-args "$VM_NAME")"
case "$socket_qemu_args_after_remove" in
  *"hostfwd=tcp::2222-:22"*) fail "socket qemu-args still rendered removed hostfwd" ;;
  *"hostfwd=tcp::10443-:443"*) fail "socket qemu-args still rendered removed hostfwd" ;;
esac

echo "PASS: port forwarding CLI/socket integration smoke ($STORE)"
