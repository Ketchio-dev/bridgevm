#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-hvf-guest-exit-loop-probe-cli.XXXXXX")"
FAKE_BIN="$STORE/bin"
BACKEND_LOG="$STORE/backend-launch.log"

mkdir -p "$FAKE_BIN"

for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
  cat >"$FAKE_BIN/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in HVF guest exit loop probe smoke: $(basename "$0")" >&2
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

output="$(cargo run -q -p bridgevm-cli -- hvf guest-exit-loop-probe 2>&1)" \
  || fail "bridgevm hvf guest-exit-loop-probe command failed: $output"

assert_contains "$output" "HVF guest exit loop probe" "HVF guest exit loop probe CLI output"
assert_contains "$output" "QEMU: not used" "HVF guest exit loop probe CLI output"
assert_contains "$output" "Apple VZ: not used" "HVF guest exit loop probe CLI output"
assert_contains "$output" "Guest execution: two HVC instructions with PC advance watchdog" "HVF guest exit loop probe CLI output"
assert_contains "$output" "Allowed: false" "HVF guest exit loop probe CLI output"
assert_contains "$output" "Attempted: false" "HVF guest exit loop probe CLI output"
assert_contains "$output" "First run attempted: false" "HVF guest exit loop probe CLI output"
assert_contains "$output" "First exit observed: false" "HVF guest exit loop probe CLI output"
assert_contains "$output" "PC advanced: false" "HVF guest exit loop probe CLI output"
assert_contains "$output" "Second run attempted: false" "HVF guest exit loop probe CLI output"
assert_contains "$output" "Second exit observed: false" "HVF guest exit loop probe CLI output"
assert_contains "$output" "Exit loop observed: false" "HVF guest exit loop probe CLI output"
assert_contains "$output" "Instructions: HVC #0; HVC #1" "HVF guest exit loop probe CLI output"
assert_contains "$output" "First run status name: not attempted" "HVF guest exit loop probe CLI output"
assert_contains "$output" "Second run status name: not attempted" "HVF guest exit loop probe CLI output"
assert_not_contains "$output" "qemu-system" "HVF guest exit loop probe CLI output"
assert_not_matches "$output" '[0-9]+([.][0-9]+)?%' "HVF guest exit loop probe CLI output"
assert_no_backend_launch

echo "PASS: HVF guest exit loop probe CLI opt-in metadata smoke ($STORE)"
