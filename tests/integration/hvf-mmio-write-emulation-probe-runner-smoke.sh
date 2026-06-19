#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-hvf-mmio-write-emulation-probe-runner.XXXXXX")"
FAKE_BIN="$STORE/bin"
BACKEND_LOG="$STORE/backend-launch.log"

mkdir -p "$FAKE_BIN"

for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
  cat >"$FAKE_BIN/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in HVF MMIO write emulation probe runner smoke: $(basename "$0")" >&2
exit 99
SH
  chmod +x "$FAKE_BIN/$backend"
done

export PATH="$FAKE_BIN:$PATH"
export BRIDGEVM_FAKE_BACKEND_LOG="$BACKEND_LOG"
export BRIDGEVM_APPLE_VZ_RUNNER="$FAKE_BIN/AppleVzRunner"
unset BRIDGEVM_HVF_ALLOW_VM_CREATE
unset BRIDGEVM_HVF_ALLOW_VCPU_RUN
unset BRIDGEVM_HVF_ALLOW_MEMORY_MAP
unset BRIDGEVM_HVF_ALLOW_GUEST_ENTRY
unset BRIDGEVM_HVF_ALLOW_EXIT_LOOP
unset BRIDGEVM_HVF_ALLOW_MMIO_READ
unset BRIDGEVM_HVF_ALLOW_MMIO_EMULATION
unset BRIDGEVM_HVF_ALLOW_MMIO_WRITE_EMULATION

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

output="$(cargo run -q -p hvf-runner -- --mmio-write-emulation-probe 2>&1)" \
  || fail "hvf-runner --mmio-write-emulation-probe command failed: $output"

assert_contains "$output" "HVF MMIO write emulation probe" "HVF MMIO write emulation runner output"
assert_contains "$output" "QEMU: not used" "HVF MMIO write emulation runner output"
assert_contains "$output" "Apple VZ: not used" "HVF MMIO write emulation runner output"
assert_contains "$output" "Guest execution: unmapped STR, captured write value, then HVC" "HVF MMIO write emulation runner output"
assert_contains "$output" "Allowed: false" "HVF MMIO write emulation runner output"
assert_contains "$output" "Attempted: false" "HVF MMIO write emulation runner output"
assert_contains "$output" "MMIO exit observed: false" "HVF MMIO write emulation runner output"
assert_contains "$output" "Write value captured: false" "HVF MMIO write emulation runner output"
assert_contains "$output" "Continuation exit observed: false" "HVF MMIO write emulation runner output"
assert_contains "$output" "Write value preserved: false" "HVF MMIO write emulation runner output"
assert_contains "$output" "Instructions: STR X0, [X1]; HVC #0" "HVF MMIO write emulation runner output"
assert_contains "$output" "Write value: 0xfedcba987654321" "HVF MMIO write emulation runner output"
assert_not_contains "$output" "qemu-system" "HVF MMIO write emulation runner output"
assert_not_matches "$output" '[0-9]+([.][0-9]+)?%' "HVF MMIO write emulation runner output"
assert_no_backend_launch

echo "PASS: HVF MMIO write emulation probe runner opt-in metadata smoke ($STORE)"
