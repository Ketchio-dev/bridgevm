#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-hvf-host-capabilities-runner.XXXXXX")"
FAKE_BIN="$STORE/bin"
BACKEND_LOG="$STORE/backend-launch.log"

mkdir -p "$FAKE_BIN"

for backend in qemu-system qemu-system-x86_64 qemu-system-aarch64 qemu-system-arm AppleVzRunner open osascript; do
  cat >"$FAKE_BIN/$backend" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FAKE_BACKEND_LOG:?}"
echo "backend or GUI launch is forbidden in HVF host capabilities runner smoke: $(basename "$0")" >&2
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

assert_matches() {
  local haystack="$1"
  local regex="$2"
  local label="$3"
  printf '%s\n' "$haystack" | grep -Eq "$regex" \
    || fail "$label did not match /$regex/; got: $haystack"
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

output="$(cargo run -q -p hvf-runner -- --host-capabilities 2>&1)" \
  || fail "hvf-runner --host-capabilities command failed: $output"

assert_contains "$output" "HVF host capabilities" "HVF host capabilities runner output"
assert_contains "$output" "Available:" "HVF host capabilities runner output"
assert_contains "$output" "Host:" "HVF host capabilities runner output"
assert_contains "$output" "Default IPA bits:" "HVF host capabilities runner output"
assert_contains "$output" "Max IPA bits:" "HVF host capabilities runner output"
assert_contains "$output" "EL2 supported:" "HVF host capabilities runner output"
assert_matches "$output" '^Available: (true|false)$' "HVF host capabilities runner availability"
assert_matches "$output" '^Host: (macos-aarch64|unsupported)$' "HVF host capabilities runner host"
assert_matches "$output" '^Default IPA bits: ([0-9]+|unknown)$' "HVF host capabilities runner default IPA"
assert_matches "$output" '^Max IPA bits: ([0-9]+|unknown)$' "HVF host capabilities runner max IPA"
assert_matches "$output" '^EL2 supported: (true|false|unknown)$' "HVF host capabilities runner EL2 support"
assert_not_matches "$output" '[0-9]+([.][0-9]+)?%' "HVF host capabilities runner output"
assert_no_backend_launch

echo "PASS: HVF host capabilities runner metadata smoke ($STORE)"
