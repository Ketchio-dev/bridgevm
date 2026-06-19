#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-hvf-guest-entry-probe-runner.XXXXXX")"
FAKE_BIN="$STORE/bin"
BACKEND_LOG="$STORE/backend-launch.log"

mkdir -p "$FAKE_BIN"

for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
  cat >"$FAKE_BIN/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in HVF guest entry probe runner smoke: $(basename "$0")" >&2
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

output="$(cargo run -q -p hvf-runner -- --guest-entry-probe 2>&1)" \
  || fail "hvf-runner --guest-entry-probe command failed: $output"

assert_contains "$output" "HVF guest entry probe" "HVF guest entry probe runner output"
assert_contains "$output" "QEMU: not used" "HVF guest entry probe runner output"
assert_contains "$output" "Apple VZ: not used" "HVF guest entry probe runner output"
assert_contains "$output" "Guest execution: one HVC instruction with watchdog" "HVF guest entry probe runner output"
assert_contains "$output" "Allowed: false" "HVF guest entry probe runner output"
assert_contains "$output" "Attempted: false" "HVF guest entry probe runner output"
assert_contains "$output" "VM created: false" "HVF guest entry probe runner output"
assert_contains "$output" "Memory mapped: false" "HVF guest entry probe runner output"
assert_contains "$output" "vCPU created: false" "HVF guest entry probe runner output"
assert_contains "$output" "PC set: false" "HVF guest entry probe runner output"
assert_contains "$output" "CPSR set: false" "HVF guest entry probe runner output"
assert_contains "$output" "Run attempted: false" "HVF guest entry probe runner output"
assert_contains "$output" "Entry boundary observed: false" "HVF guest entry probe runner output"
assert_contains "$output" "Instruction: HVC #0" "HVF guest entry probe runner output"
assert_contains "$output" "Run status name: not attempted" "HVF guest entry probe runner output"
assert_contains "$output" "Exit reason name: not observed" "HVF guest entry probe runner output"
assert_not_contains "$output" "qemu-system" "HVF guest entry probe runner output"
assert_not_matches "$output" '[0-9]+([.][0-9]+)?%' "HVF guest entry probe runner output"
assert_no_backend_launch

echo "PASS: HVF guest entry probe runner opt-in metadata smoke ($STORE)"
