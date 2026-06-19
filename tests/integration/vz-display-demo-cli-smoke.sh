#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

fail() {
  echo "FAIL: $*" >&2
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

assert_fails_contains() {
  local label="$1"
  local expected="$2"
  shift 2

  local output
  if output="$("$@" 2>&1)"; then
    fail "$label unexpectedly succeeded"
  fi
  assert_contains "$output" "$expected" "$label"
}

bash -n scripts/run-vz-display-demo.sh

help_output="$(bash scripts/run-vz-display-demo.sh --help 2>&1)"
assert_contains "$help_output" "--prove-proxy-crop" "help output"
assert_contains "$help_output" "--preflight" "help output"
assert_contains "$help_output" "--width PX --height PX" "help output"
grep -Fq "verify-vz-proxy-crop-evidence.sh" scripts/run-vz-display-demo.sh \
  || fail "run-vz-display-demo.sh does not wire the proxy-crop evidence verifier"

preflight_dir="$(mktemp -d /tmp/bridgevm-vz-preflight-fixture.XXXXXX)"
preflight_output="$(
  BRIDGEVM_LIVE_VZ_FIXTURE_DIR="$preflight_dir" \
    bash scripts/run-vz-display-demo.sh --preflight --width 1440 --height 900
)"
assert_contains "$preflight_output" "BridgeVM Apple VZ display demo preflight" "preflight output"
assert_contains "$preflight_output" "No downloads, signing, Apple VZ launch, GUI capture, or displayd run performed." "preflight output"
assert_contains "$preflight_output" "Fixture dir: $preflight_dir" "preflight output"
assert_contains "$preflight_output" "Kernel: missing" "preflight output"
assert_contains "$preflight_output" "Initrd: missing" "preflight output"
assert_contains "$preflight_output" "Raw disk: missing" "preflight output"
assert_contains "$preflight_output" "AppleVzRunner:" "preflight output"
assert_contains "$preflight_output" "Default display size: 1440x900" "preflight output"
assert_contains "$preflight_output" "Preflight ready without setup: false" "preflight output"
[[ ! -e "$preflight_dir/linux" ]] || fail "preflight unexpectedly created kernel fixture"
[[ ! -e "$preflight_dir/initrd.gz" ]] || fail "preflight unexpectedly created initrd fixture"
[[ ! -e "$preflight_dir/root.raw" ]] || fail "preflight unexpectedly created raw disk fixture"

assert_fails_contains \
  "missing paired height" \
  "--width and --height must be provided together" \
  bash scripts/run-vz-display-demo.sh --check --width 1440

assert_fails_contains \
  "non-proof evidence dir" \
  "--evidence-dir only applies" \
  bash scripts/run-vz-display-demo.sh --check --evidence-dir /tmp/unused

assert_fails_contains \
  "non-proof timing args" \
  "--proof-seconds and --capture-delay only apply" \
  bash scripts/run-vz-display-demo.sh --check --proof-seconds 2

assert_fails_contains \
  "invalid crop proof timing" \
  "--capture-delay must be less than --proof-seconds" \
  bash scripts/run-vz-display-demo.sh --prove-proxy-crop --proof-seconds 5 --capture-delay 5

echo "PASS: VZ display demo CLI parser smoke"
