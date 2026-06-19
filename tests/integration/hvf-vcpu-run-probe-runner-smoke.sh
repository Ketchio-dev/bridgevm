#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-hvf-vcpu-run-probe-runner.XXXXXX")"
FAKE_BIN="$STORE/bin"
BACKEND_LOG="$STORE/backend-launch.log"

mkdir -p "$FAKE_BIN"

for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
  cat >"$FAKE_BIN/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in HVF vCPU run probe runner smoke: $(basename "$0")" >&2
exit 99
SH
  chmod +x "$FAKE_BIN/$backend"
done

export PATH="$FAKE_BIN:$PATH"
export BRIDGEVM_FAKE_BACKEND_LOG="$BACKEND_LOG"
export BRIDGEVM_APPLE_VZ_RUNNER="$FAKE_BIN/AppleVzRunner"
unset BRIDGEVM_HVF_ALLOW_VM_CREATE
unset BRIDGEVM_HVF_ALLOW_VCPU_RUN

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

output="$(cargo run -q -p hvf-runner -- --vcpu-run-probe 2>&1)" \
  || fail "hvf-runner --vcpu-run-probe command failed: $output"

assert_contains "$output" "HVF vCPU run/cancel probe" "HVF vCPU run probe runner output"
assert_contains "$output" "QEMU: not used" "HVF vCPU run probe runner output"
assert_contains "$output" "Apple VZ: not used" "HVF vCPU run probe runner output"
assert_contains "$output" "Guest execution: pre-canceled before entry" "HVF vCPU run probe runner output"
assert_contains "$output" "Allowed: false" "HVF vCPU run probe runner output"
assert_contains "$output" "Attempted: false" "HVF vCPU run probe runner output"
assert_contains "$output" "VM created: false" "HVF vCPU run probe runner output"
assert_contains "$output" "vCPU created: false" "HVF vCPU run probe runner output"
assert_contains "$output" "Cancel requested: false" "HVF vCPU run probe runner output"
assert_contains "$output" "Run attempted: false" "HVF vCPU run probe runner output"
assert_contains "$output" "Run boundary observed: false" "HVF vCPU run probe runner output"
assert_contains "$output" "Exit reason name: not observed" "HVF vCPU run probe runner output"
assert_not_contains "$output" "qemu-system" "HVF vCPU run probe runner output"
assert_not_matches "$output" '[0-9]+([.][0-9]+)?%' "HVF vCPU run probe runner output"
assert_no_backend_launch

echo "PASS: HVF vCPU run probe runner opt-in metadata smoke ($STORE)"
