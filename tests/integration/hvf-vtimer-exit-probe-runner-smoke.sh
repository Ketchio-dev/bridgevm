#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-hvf-vtimer-exit-probe-runner.XXXXXX")"
FAKE_BIN="$STORE/bin"
BACKEND_LOG="$STORE/backend-launch.log"

mkdir -p "$FAKE_BIN"

for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
  cat >"$FAKE_BIN/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in HVF VTimer exit runner smoke: $(basename "$0")" >&2
exit 99
SH
  chmod +x "$FAKE_BIN/$backend"
done

export PATH="$FAKE_BIN:$PATH"
export BRIDGEVM_FAKE_BACKEND_LOG="$BACKEND_LOG"
export BRIDGEVM_APPLE_VZ_RUNNER="$FAKE_BIN/AppleVzRunner"
unset BRIDGEVM_HVF_ALLOW_VTIMER_EXIT

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

output="$(cargo run -q -p hvf-runner -- --vtimer-exit-probe 2>&1)" \
  || fail "hvf-runner --vtimer-exit-probe command failed: $output"

assert_contains "$output" "HVF VTimer exit probe" "HVF VTimer exit runner output"
assert_contains "$output" "QEMU: not used" "HVF VTimer exit runner output"
assert_contains "$output" "Apple VZ: not used" "HVF VTimer exit runner output"
assert_contains "$output" "Guest execution: WFI wait loop with host-programmed virtual timer" "HVF VTimer exit runner output"
assert_contains "$output" "Allowed: false" "HVF VTimer exit runner output"
assert_contains "$output" "Attempted: false" "HVF VTimer exit runner output"
assert_contains "$output" "VM created: false" "HVF VTimer exit runner output"
assert_contains "$output" "Memory mapped: false" "HVF VTimer exit runner output"
assert_contains "$output" "vCPU created: false" "HVF VTimer exit runner output"
assert_contains "$output" "VTimer offset set: false" "HVF VTimer exit runner output"
assert_contains "$output" "CNTV_CVAL_EL0 set: false" "HVF VTimer exit runner output"
assert_contains "$output" "CNTV_CTL_EL0 set: false" "HVF VTimer exit runner output"
assert_contains "$output" "VTimer unmasked: false" "HVF VTimer exit runner output"
assert_contains "$output" "Run attempted: false" "HVF VTimer exit runner output"
assert_contains "$output" "VTimer exit observed: false" "HVF VTimer exit runner output"
assert_contains "$output" "Pending IRQ injected: false" "HVF VTimer exit runner output"
assert_contains "$output" "VTimer mask observed after exit: not observed" "HVF VTimer exit runner output"
assert_contains "$output" "Watchdog cancel fired: false" "HVF VTimer exit runner output"
assert_contains "$output" "Instructions: WFI; HVC #0" "HVF VTimer exit runner output"
assert_contains "$output" "CNTV_CTL_EL0 requested: 0x1" "HVF VTimer exit runner output"
assert_contains "$output" "Run status name: not attempted" "HVF VTimer exit runner output"
assert_contains "$output" "Exit reason name: not observed" "HVF VTimer exit runner output"
assert_contains "$output" "set BRIDGEVM_HVF_ALLOW_VTIMER_EXIT=1 or pass --allow-vtimer-exit" "HVF VTimer exit runner output"
assert_not_contains "$output" "qemu-system" "HVF VTimer exit runner output"
assert_not_matches "$output" '[0-9]+([.][0-9]+)?%' "HVF VTimer exit runner output"
assert_no_backend_launch

echo "PASS: HVF VTimer exit probe runner opt-in metadata smoke ($STORE)"
