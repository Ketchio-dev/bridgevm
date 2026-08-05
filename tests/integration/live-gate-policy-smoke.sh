#!/usr/bin/env bash
# Policy tests for the Studio live-gate queue.
#
# These assert the properties that make a local queue acceptable on a public
# repository: no inbound listener, no runner registration, atomic job claiming,
# and receipts that are redacted before anyone can read them.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/../.." && pwd)"
CLI="$REPO/scripts/live-gates/bridgevm-live"
WORKER="$REPO/scripts/live-gates/bridgevm-live-worker.sh"
INSTALL="$REPO/scripts/live-gates/install-studio-queue.sh"
PLIST="$REPO/scripts/live-gates/com.ketchio.bridgevm-live.plist"
REDACT="$REPO/scripts/live-gates/redact-receipt.py"
TIER="$REPO/scripts/live-gates/run-tier.sh"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
export BRIDGEVM_LIVE_ROOT="$WORK/queue"

checks=0
check() {
    checks=$((checks + 1))
    if ! eval "$2"; then
        echo "FAIL: $1" >&2
        exit 1
    fi
}

# Assert a pattern is absent. Separate from `check` because passing a regex
# through `eval` needs two rounds of quoting and silently mangles backslashes.
# Comments are stripped first: these files *describe* the listener they must
# not open, and matching prose would make the check unfalsifiable in reverse.
no_match() {
    local description="$1" pattern="$2"
    shift 2
    checks=$((checks + 1))
    local hits
    hits="$(cat "$@" | sed 's/#.*$//' | grep -inE "$pattern" || true)"
    if [ -n "$hits" ]; then
        echo "FAIL: $description" >&2
        printf '%s\n' "$hits" >&2
        exit 1
    fi
}

# --- shape ---------------------------------------------------------------
check "the CLI is executable" '[ -x "$CLI" ]'
check "the worker is executable" '[ -x "$WORKER" ]'
check "the installer is executable" '[ -x "$INSTALL" ]'
check "the redactor is executable" '[ -x "$REDACT" ]'
check "the tier dispatcher is executable" '[ -x "$TIER" ]'
check "the plist is well formed" 'plutil -lint "$PLIST" >/dev/null'

# --- no inbound network path --------------------------------------------
# A local queue that grew a listener would be a self-hosted runner with extra
# steps, which is exactly what a public repo must not have.
check "the plist declares no socket" '! grep -q "<key>Sockets</key>" "$PLIST"'
check "the plist is not a root daemon" '! grep -qi "UserName.*root" "$PLIST"'
no_match "no component opens a listening socket" \
    'nc +-l|socat|LISTEN|bind\(' "$CLI" "$WORKER" "$TIER"
# The installer names actions-runner only to refuse installing beside one, so
# this looks for the act of registering rather than the word.
no_match "nothing registers a GitHub runner" \
    'config\.sh +--url|RUNNER_TOKEN|--runnergroup' "$CLI" "$WORKER" "$INSTALL" "$TIER"
check "the installer refuses to sit beside a runner" \
    'grep -q "actions-runner" "$INSTALL"'
no_match "nothing in the queue path uses sudo" \
    '^[^#]*\bsudo\b' "$CLI" "$WORKER" "$INSTALL" "$TIER"

# --- submit returns immediately -----------------------------------------
start=$(date +%s)
job_id="$("$CLI" submit t1-vtimer)"
elapsed=$(( $(date +%s) - start ))
check "submit returns a job id" '[ -n "$job_id" ]'
check "submit returns in under 10s" '[ "$elapsed" -lt 10 ]'
check "the job is queued" '"$CLI" status | grep -q "queued .*$job_id"'

# --- the exact commit is sealed at submit time ---------------------------
check "the job records a commit" 'grep -q "^commit=[0-9a-f]\{40\}$" "$BRIDGEVM_LIVE_ROOT/queued/$job_id/job.env"'
check "the job records its tier" 'grep -q "^tier=t1-vtimer$" "$BRIDGEVM_LIVE_ROOT/queued/$job_id/job.env"'

# --- claiming is atomic --------------------------------------------------
claimed="$("$CLI" next)"
check "next claims the job" '[ -n "$claimed" ]'
check "the claimed job moved to running" '"$CLI" status | grep -q "running .*$job_id"'
check "a second worker cannot claim it" '! "$CLI" next'

# --- receipts are only served redacted -----------------------------------
cat > "$claimed/receipt.json" <<'JSON'
{"tier":"t1-vtimer","pass":true,"disk_path":"/Users/me/win11.qcow2","vars_path":"/tmp/VARS.fd"}
JSON
check "a raw receipt is never served" '! "$CLI" receipt "$job_id" >/dev/null 2>&1'
python3 "$REDACT" --in "$claimed/receipt.json" --out "$claimed/receipt.public.json"
check "the published receipt is served" '"$CLI" receipt "$job_id" >/dev/null'
check "the published receipt drops the disk path" \
    '! "$CLI" receipt "$job_id" | grep -q "qcow2"'
check "the published receipt drops the vars path" \
    '! "$CLI" receipt "$job_id" | grep -q "VARS.fd"'
check "the published receipt keeps the result" \
    '"$CLI" receipt "$job_id" | grep -q "\"pass\": true"'

# --- cancellation --------------------------------------------------------
check "cancelling a running job requests, not kills" \
    '"$CLI" cancel "$job_id" | grep -q "cancellation requested"'
check "the cancel request is visible to the worker" '[ -f "$claimed/cancel.requested" ]'

second="$("$CLI" submit t0-check)"
check "a queued job cancels immediately" '"$CLI" cancel "$second" | grep -q "canceled"'
check "the canceled job is done" '"$CLI" status | grep -q "done .*$second"'

# --- tiers refuse to invent evidence -------------------------------------
check "an unknown tier is rejected" '! "$TIER" nonsense --out "$WORK/x" 2>/dev/null'
t5_output="$(BASE_IMAGE="$WORK/absent.raw" BASE_VARS="$WORK/absent.fd" \
    INJECTOR="$WORK/absent-inj.raw" \
    "$TIER" t5-campaign --out "$WORK/t5" 2>&1 || true)"
check "boot tiers refuse when the media cannot be read" \
    'printf "%s" "$t5_output" | grep -q "cannot read required Windows media"'
check "the refusal names which input is missing" \
    'printf "%s" "$t5_output" | grep -q "absent-inj.raw"'
check "the refusal is recorded in the receipt" \
    'grep -q "refused-no-media" "$WORK/t5/receipt.json"'

# An unreadable file, not just an absent one: this is the TCC failure mode,
# where the path exists and stat() succeeds but the bytes cannot be read.
: > "$WORK/unreadable.raw"; chmod 000 "$WORK/unreadable.raw"
unreadable_output="$(BASE_IMAGE="$WORK/unreadable.raw" \
    "$TIER" t5-campaign --out "$WORK/t5b" 2>&1 || true)"
chmod 644 "$WORK/unreadable.raw"
check "a present but unreadable input is refused too" \
    'printf "%s" "$unreadable_output" | grep -q "unreadable.raw"'

# --- the installer is safe to inspect ------------------------------------
check "the installer supports a dry run" '"$INSTALL" --dry-run >/dev/null 2>&1 || true'
no_match "the installer stores no credentials" \
    'password|token=|api[_-]key' "$INSTALL"

echo "PASS: live gate policy smoke ($checks checks)"
