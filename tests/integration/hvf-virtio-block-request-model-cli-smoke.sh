#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-hvf-virtio-block-request-model-cli.XXXXXX")"
FAKE_BIN="$STORE/bin"
BACKEND_LOG="$STORE/backend-launch.log"

mkdir -p "$FAKE_BIN"

for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
  cat >"$FAKE_BIN/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in VirtIO block request model smoke: $(basename "$0")" >&2
exit 99
SH
  chmod +x "$FAKE_BIN/$backend"
done

export PATH="$FAKE_BIN:$PATH"
export BRIDGEVM_FAKE_BACKEND_LOG="$BACKEND_LOG"
export BRIDGEVM_APPLE_VZ_RUNNER="$FAKE_BIN/AppleVzRunner"

fail() {
  echo "FAIL: $*" >&2
  echo "Store preserved at $STORE" >&2
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

assert_not_matches() {
  local haystack="$1"
  local regex="$2"
  local label="$3"
  if printf '%s\n' "$haystack" | grep -Eq "$regex"; then
    fail "$label unexpectedly matched /$regex/; got: $haystack"
  fi
}

assert_no_backend_launch() {
  [[ ! -s "$BACKEND_LOG" ]] || fail "backend or GUI launch attempted: $(cat "$BACKEND_LOG")"
}

output="$(cargo run -q -p bridgevm-cli -- hvf virtio-block-request-model-probe 2>&1)" \
  || fail "bridgevm hvf virtio-block-request-model-probe failed: $output"

assert_contains "$output" "VirtIO block request model probe" "VirtIO block request model CLI output"
assert_contains "$output" "QEMU: not used" "VirtIO block request model CLI output"
assert_contains "$output" "Apple VZ: not used" "VirtIO block request model CLI output"
assert_contains "$output" "HVF: not entered" "VirtIO block request model CLI output"
assert_contains "$output" "Guest execution: not entered; in-memory VirtIO block descriptor chain" "VirtIO block request model CLI output"
assert_contains "$output" "Configured via MMIO: true" "VirtIO block request model CLI output"
assert_contains "$output" "Configured via MMIO bus: true" "VirtIO block request model CLI output"
assert_contains "$output" "Queue notified: true" "VirtIO block request model CLI output"
assert_contains "$output" "Queue notify value: 0x0" "VirtIO block request model CLI output"
assert_contains "$output" "Completed via device bus: true" "VirtIO block request model CLI output"
assert_contains "$output" "Completed: true" "VirtIO block request model CLI output"
assert_contains "$output" "Descriptor index: 0x0" "VirtIO block request model CLI output"
assert_contains "$output" "Request type: 0x0" "VirtIO block request model CLI output"
assert_contains "$output" "Sector: 0x7" "VirtIO block request model CLI output"
assert_contains "$output" "Data bytes: 0x200" "VirtIO block request model CLI output"
assert_contains "$output" "Data prefix: 0x0708090a0b0c0d0e" "VirtIO block request model CLI output"
assert_contains "$output" "Status byte: 0x0" "VirtIO block request model CLI output"
assert_contains "$output" "Used index: 0x1" "VirtIO block request model CLI output"
assert_contains "$output" "Used length: 0x201" "VirtIO block request model CLI output"
assert_contains "$output" "Interrupt status: 0x1" "VirtIO block request model CLI output"
assert_contains "$output" "Blockers: none" "VirtIO block request model CLI output"
assert_not_contains "$output" "qemu-system" "VirtIO block request model CLI output"
assert_not_matches "$output" '[0-9]+([.][0-9]+)?%' "VirtIO block request model CLI output"
assert_no_backend_launch

echo "PASS: VirtIO block request model CLI metadata smoke ($STORE)"
