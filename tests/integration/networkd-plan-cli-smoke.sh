#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

WORKDIR="$(mktemp -d "/tmp/bridgevm-networkd-plan.XXXXXX")"
PRESERVE_WORKDIR=1

networkd() {
  cargo run --quiet -p networkd -- "$@"
}

fail() {
  echo "FAIL: $*" >&2
  echo "Workdir preserved at $WORKDIR" >&2
  exit 1
}

cleanup() {
  if [[ "$PRESERVE_WORKDIR" == "0" ]]; then
    rm -rf "$WORKDIR"
  fi
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
  local needle="$2"
  shift 2
  local output
  set +e
  output="$("$@" 2>&1)"
  local status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    fail "$label unexpectedly succeeded: $output"
  fi
  assert_contains "$output" "$needle" "$label"
}

assert_json_plan() {
  local label="$1"
  local path="$2"
  local backend="$3"
  local mode="$4"
  local hostname="$5"
  local expected_forwards="$6"
  local guest_outbound="$7"
  local host_to_guest="$8"
  local supports_port_forwarding="$9"
  local guest_to_host="${10}"
  local host_visible_hostname="${11}"
  local requires_privileged_helper="${12}"
  local expected_requirements="${13}"
  local expected_notes="${14}"
  python3 - \
    "$path" \
    "$backend" \
    "$mode" \
    "$hostname" \
    "$expected_forwards" \
    "$guest_outbound" \
    "$host_to_guest" \
    "$supports_port_forwarding" \
    "$guest_to_host" \
    "$host_visible_hostname" \
    "$requires_privileged_helper" \
    "$expected_requirements" \
    "$expected_notes" <<'PY' || fail "$label JSON plan mismatch"
import json
import sys

(
    path,
    expected_backend,
    expected_mode,
    expected_hostname,
    expected_forwards,
    expected_guest_outbound,
    expected_host_to_guest,
    expected_supports_port_forwarding,
    expected_guest_to_host,
    expected_host_visible_hostname,
    expected_requires_privileged_helper,
    expected_requirements,
    expected_notes,
) = sys.argv[1:14]

with open(path, "r", encoding="utf-8") as handle:
    plan = json.load(handle)

capabilities = plan["capabilities"]
requirements = plan["requirements"]
notes = plan["notes"]
checks = [
    plan["backend"] == expected_backend,
    plan["mode"] == expected_mode,
    plan["hostname"] == expected_hostname,
    len(plan["port_forwards"]) == int(expected_forwards),
    capabilities["guest_outbound"] == (expected_guest_outbound == "true"),
    capabilities["host_to_guest"] == (expected_host_to_guest == "true"),
    capabilities["supports_port_forwarding"] == (
        expected_supports_port_forwarding == "true"
    ),
    capabilities["guest_to_host"] == (expected_guest_to_host == "true"),
    capabilities["host_visible_hostname"] == (expected_host_visible_hostname == "true"),
    capabilities["requires_privileged_helper"] == (
        expected_requires_privileged_helper == "true"
    ),
    len(requirements) == int(expected_requirements),
    len(notes) == int(expected_notes),
]

if not all(checks):
    raise SystemExit(plan)
PY
}

trap cleanup EXIT

command -v python3 >/dev/null || fail "python3 is required for JSON validation"

qemu_nat="$WORKDIR/qemu-nat.json"
networkd \
  --print-plan \
  --backend qemu \
  --mode nat \
  --hostname dev.bridgevm.local \
  --forward 2222:22 \
  --forward 8080:80 >"$qemu_nat"
assert_json_plan \
  "qemu nat plan" \
  "$qemu_nat" \
  "qemu" \
  "nat" \
  "dev.bridgevm.local" \
  2 \
  true \
  true \
  true \
  true \
  true \
  false \
  0 \
  2
python3 - "$qemu_nat" <<'PY' || fail "qemu nat forward rules mismatch"
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    plan = json.load(handle)

forwards = plan["port_forwards"]
assert forwards == [{"host": 2222, "guest": 22}, {"host": 8080, "guest": 80}], forwards
assert plan["notes"] == [
    "default NAT networking with automatic DNS intent",
    "port forwards are planning-time rules consumed by backend launchers",
], plan["notes"]
PY

apple_host_only="$WORKDIR/apple-host-only.json"
networkd \
  --print-plan \
  --backend apple-vz \
  --mode host-only \
  --hostname hostonly.bridgevm.local >"$apple_host_only"
assert_json_plan \
  "apple-vz host-only plan" \
  "$apple_host_only" \
  "apple-vz" \
  "host-only" \
  "hostonly.bridgevm.local" \
  0 \
  false \
  true \
  false \
  true \
  true \
  false \
  0 \
  1

qemu_host_only="$WORKDIR/qemu-host-only.json"
networkd \
  --print-plan \
  --backend qemu \
  --mode host-only \
  --hostname qemu-hostonly.bridgevm.local >"$qemu_host_only"
assert_json_plan \
  "qemu host-only privilege plan" \
  "$qemu_host_only" \
  "qemu" \
  "host-only" \
  "qemu-hostonly.bridgevm.local" \
  0 \
  false \
  true \
  false \
  true \
  true \
  true \
  1 \
  1
python3 - "$qemu_host_only" <<'PY' || fail "qemu host-only blocker metadata mismatch"
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    plan = json.load(handle)

assert plan["requirements"] == [
    {
        "blocker": "qemu-host-only-requires-privilege",
        "requirement": "Compatibility Mode QEMU host-only networking uses vmnet-host, which requires the qemu process to run as root or carry the com.apple.vm.networking entitlement",
    }
], plan["requirements"]
assert plan["notes"] == [
    "host-only network intent; guest outbound internet is disabled",
], plan["notes"]
PY

qemu_isolated="$WORKDIR/qemu-isolated.json"
networkd \
  --print-plan \
  --backend qemu \
  --mode isolated \
  --hostname isolated.bridgevm.local >"$qemu_isolated"
assert_json_plan \
  "qemu isolated plan" \
  "$qemu_isolated" \
  "qemu" \
  "isolated" \
  "isolated.bridgevm.local" \
  0 \
  false \
  false \
  false \
  false \
  false \
  false \
  0 \
  1

qemu_bridged="$WORKDIR/qemu-bridged.json"
networkd \
  --print-plan \
  --backend qemu \
  --mode bridged \
  --hostname bridged.bridgevm.local >"$qemu_bridged"
assert_json_plan \
  "qemu bridged vmnet privilege plan" \
  "$qemu_bridged" \
  "qemu" \
  "bridged" \
  "bridged.bridgevm.local" \
  0 \
  true \
  true \
  false \
  true \
  true \
  true \
  1 \
  1
python3 - "$qemu_bridged" <<'PY' || fail "qemu bridged vmnet privilege metadata mismatch"
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    plan = json.load(handle)

assert plan["requirements"] == [
    {
        "blocker": "qemu-bridged-requires-privilege",
        "requirement": "Compatibility Mode QEMU bridged networking uses vmnet-bridged, which requires the qemu process to run as root or carry the com.apple.vm.networking entitlement",
    }
], plan["requirements"]
assert plan["notes"] == [
    "bridged network intent; privileged helper may be required",
], plan["notes"]
PY

summary="$(networkd --backend qemu --mode nat --forward 2222:22)"
assert_contains "$summary" "networkd ready: qemu backend, nat mode, 1 forward rule(s)" "summary output"

blocked_summary="$(networkd --backend qemu --mode host-only)"
assert_contains "$blocked_summary" "networkd blocked: qemu backend, host-only mode, 0 forward rule(s), 1 requirement(s)" "blocked summary output"

assert_fails_contains \
  "malformed forward" \
  "forward must use HOST:GUEST" \
  networkd --print-plan --forward 2222

assert_fails_contains \
  "zero port forward" \
  "port must be between 1 and 65535" \
  networkd --print-plan --forward 0:22

assert_fails_contains \
  "duplicate host port forward" \
  "host port 2222 is already forwarded" \
  networkd --print-plan --forward 2222:22 --forward 2222:2222

assert_fails_contains \
  "isolated forwarding rejection" \
  "does not support port forwarding" \
  networkd --print-plan --backend qemu --mode isolated --forward 2222:22

assert_fails_contains \
  "apple-vz bridged rejection" \
  "AppleVz does not support bridged networking yet" \
  networkd --print-plan --backend apple-vz --mode bridged

PRESERVE_WORKDIR=0
echo "PASS: networkd public CLI plan smoke ($WORKDIR)"
