#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

STORE="$(mktemp -d "/tmp/bridgevm-sleep-wake-baseline.XXXXXX")"
FAKE_BIN="$STORE/bin"
VM_NAME="sleep-wake-baseline"
BACKEND_LOG="$STORE/forbidden-launch.log"
HOST_SLEEP_LOG="$STORE/forbidden-host-sleep.log"
ARTIFACT_DIR="$STORE/artifacts"
BASELINE="$ARTIFACT_DIR/sleep-wake-baseline.json"

mkdir -p "$FAKE_BIN" "$ARTIFACT_DIR"

for forbidden in qemu-system-x86_64 qemu-system-aarch64 AppleVzRunner; do
  cat >"$FAKE_BIN/$forbidden" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf '%s %s\n' "$(basename "$0")" "$*" >>"${BRIDGEVM_FORBIDDEN_BACKEND_LOG:?}"
echo "backend launch is forbidden in sleep/wake metadata baseline: $(basename "$0")" >&2
exit 99
SH
  chmod +x "$FAKE_BIN/$forbidden"
done

cat >"$FAKE_BIN/pmset" <<'SH'
#!/usr/bin/env bash
set -euo pipefail

printf 'pmset %s\n' "$*" >>"${BRIDGEVM_FORBIDDEN_HOST_SLEEP_LOG:?}"
echo "host sleep control is forbidden in sleep/wake metadata baseline" >&2
exit 99
SH
chmod +x "$FAKE_BIN/pmset"

export PATH="$FAKE_BIN:$PATH"
export BRIDGEVM_FORBIDDEN_BACKEND_LOG="$BACKEND_LOG"
export BRIDGEVM_FORBIDDEN_HOST_SLEEP_LOG="$HOST_SLEEP_LOG"

bridgevm() {
  cargo run --quiet -p bridgevm-cli -- --store "$STORE" "$@"
}

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

assert_no_forbidden_actions() {
  [[ ! -s "$BACKEND_LOG" ]] || fail "backend launch attempted: $(cat "$BACKEND_LOG")"
  [[ ! -s "$HOST_SLEEP_LOG" ]] || fail "host sleep command attempted: $(cat "$HOST_SLEEP_LOG")"
}

bridgevm create "$VM_NAME" --os ubuntu --arch arm64 --mode fast >/dev/null

initial_status="$(bridgevm status "$VM_NAME")"
assert_contains "$initial_status" "State: stopped" "initial status"

suspend_plan="$(bridgevm lifecycle-plan "$VM_NAME" --action suspend)"
assert_contains "$suspend_plan" "Lifecycle plan for $VM_NAME" "suspend plan"
assert_contains "$suspend_plan" "Action: suspend" "suspend plan"
assert_contains "$suspend_plan" "Metadata only: true" "suspend plan"
assert_contains "$suspend_plan" "Executable: false" "suspend plan"
assert_contains "$suspend_plan" "Blocker: invalid-lifecycle-transition:" "suspend plan"
assert_contains "$suspend_plan" "Fast Mode suspend/resume is wired through the runner via Apple VZ" "suspend plan"

resume_plan="$(bridgevm lifecycle-plan "$VM_NAME" --action resume)"
assert_contains "$resume_plan" "Lifecycle plan for $VM_NAME" "resume plan"
assert_contains "$resume_plan" "Action: resume" "resume plan"
assert_contains "$resume_plan" "Metadata only: true" "resume plan"
assert_contains "$resume_plan" "Executable: false" "resume plan"
assert_contains "$resume_plan" "Blocker: invalid-lifecycle-transition:stopped->running" "resume plan"
assert_contains "$resume_plan" "Fast Mode suspend/resume is wired through the runner via Apple VZ" "resume plan"

final_status="$(bridgevm status "$VM_NAME")"
assert_contains "$final_status" "State: stopped" "final status"

cat >"$BASELINE" <<EOF
{
  "lane": "sleep-wake",
  "mode": "metadata-only",
  "vm": "$VM_NAME",
  "store": "$STORE",
  "live_host_sleep_exercised": false,
  "vm_start_exercised": false,
  "initial_state": "stopped",
  "final_state": "stopped",
  "checked_commands": [
    "bridgevm create",
    "bridgevm status",
    "bridgevm lifecycle-plan --action suspend",
    "bridgevm lifecycle-plan --action resume"
  ],
  "future_live_scope": "host sleep/wake recovery with a running VM, guest networking, display, and resume validation"
}
EOF

grep -q '"live_host_sleep_exercised": false' "$BASELINE" \
  || fail "baseline artifact did not mark host sleep as non-live"
grep -q '"vm_start_exercised": false' "$BASELINE" \
  || fail "baseline artifact did not mark VM start as non-live"

assert_no_forbidden_actions

echo "PASS: sleep/wake metadata baseline smoke ($BASELINE)"
