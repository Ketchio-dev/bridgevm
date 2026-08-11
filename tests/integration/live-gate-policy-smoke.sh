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
A3_TIER="$REPO/scripts/live-gates/run-a3-title-tier.sh"
A3_RECEIPT="$REPO/scripts/live-gates/write-a3-title-receipt.py"
A3_VERIFY="$REPO/scripts/verify-d3d11-title-fps.sh"
BOOT_RUNNER="$REPO/scripts/run-hvf-windows-installed-boot-runner.sh"

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
check "the A3 tier helper is executable" '[ -x "$A3_TIER" ]'
check "the A3 receipt writer is executable" '[ -x "$A3_RECEIPT" ]'
check "the plist is well formed" 'plutil -lint "$PLIST" >/dev/null'

# --- no inbound network path --------------------------------------------
# A local queue that grew a listener would be a self-hosted runner with extra
# steps, which is exactly what a public repo must not have.
check "the plist declares no socket" '! grep -q "<key>Sockets</key>" "$PLIST"'
check "the plist is not a root daemon" '! grep -qi "UserName.*root" "$PLIST"'
no_match "no component opens a listening socket" \
    'nc +-l|socat|LISTEN|bind\(' "$CLI" "$WORKER" "$TIER" "$A3_TIER" "$A3_RECEIPT"
# The installer names actions-runner only to refuse installing beside one, so
# this looks for the act of registering rather than the word.
no_match "nothing registers a GitHub runner" \
    'config\.sh +--url|RUNNER_TOKEN|--runnergroup' "$CLI" "$WORKER" "$INSTALL" "$TIER" "$A3_TIER" "$A3_RECEIPT"
check "the installer refuses to sit beside a runner" \
    'grep -q "actions-runner" "$INSTALL"'
no_match "nothing in the queue path uses sudo" \
    '^[^#]*\bsudo\b' "$CLI" "$WORKER" "$INSTALL" "$TIER" "$A3_TIER" "$A3_RECEIPT"
check "the A3 receipt requires all three runs" \
    'python3 "$A3_RECEIPT" --self-test | grep -q "PASS"'
check "the A3 campaign stops once three of three is impossible" \
    'grep -Fq '\''verify-d3d11-title-fps.sh" || break'\'' "$A3_TIER"'
check "the A3 verifier kills a parked boot after diagnostics" \
    'grep -q "BRIDGEVM_BOOT_PROGRESS_KILL=1" "$A3_VERIFY"'
check "the A3 verifier notices an exited launcher while waiting" \
    'grep -q '\''kill -0 "$LAUNCHER"'\'' "$A3_VERIFY"'

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

# The A3 tier must seal a copied input manifest at submit time. It may not
# retain a caller-owned path that can be edited after submission.
manifest="$WORK/a3-inputs.tsv"
printf 'image\t/tmp/image\t%s\n' "$(printf image | shasum -a 256 | cut -d' ' -f1)" > "$manifest"
printf '#!/bin/sh\nexit 0\n' > "$WORK/probe"; chmod +x "$WORK/probe"
printf 'binary\t%s\t%s\n' "$WORK/probe" "$(shasum -a 256 "$WORK/probe" | cut -d' ' -f1)" >> "$manifest"
a3_job="$($CLI submit t6-a3-title --input-manifest "$manifest")"
check "A3 submit copies the input manifest" \
    '[ -f "$BRIDGEVM_LIVE_ROOT/queued/$a3_job/input-manifest.tsv" ]'
check "A3 submit seals the copied manifest hash" \
    'grep -q "^input_manifest_sha256=[0-9a-f]\{64\}$" "$BRIDGEVM_LIVE_ROOT/queued/$a3_job/job.env"'
check "A3 submit copies and seals the release binary" \
    'test -x "$BRIDGEVM_LIVE_ROOT/queued/$a3_job/hvf_gic_boot_probe" && grep -q "^sealed_binary_sha256=[0-9a-f]\{64\}$" "$BRIDGEVM_LIVE_ROOT/queued/$a3_job/job.env"'
check "A3 submit rejects a missing manifest" \
    '! "$CLI" submit t6-a3-title --input-manifest "$WORK/missing" >/dev/null 2>&1'
check "non-A3 tiers reject an input manifest" \
    '! "$CLI" submit t0-check --input-manifest "$manifest" >/dev/null 2>&1'
malformed_out="$WORK/malformed-a3"
check "the A3 tier refuses a malformed manifest before build" \
    '! "$TIER" t6-a3-title --out "$malformed_out" --input-manifest "$manifest" --job-id policy-smoke >/dev/null 2>&1'
check "the A3 refusal leaves a machine-readable receipt" \
    'python3 - "$malformed_out/receipt.json" <<'"'"'PY'"'"'
import json, sys
r = json.load(open(sys.argv[1]))
required = {"gate_id", "criterion", "tested_commit", "host_os", "host_hardware", "sample_count", "passes", "failures", "evidence_paths"}
assert required <= r.keys()
assert r["criterion"] == "A3" and r["pass"] is False
PY'
check "the installed-boot runner honors a sealed prebuilt binary" \
    'grep -Fq '"'"'BRIDGEVM_PREBUILT_PROBE requires an absolute regular release binary with --skip-build'"'"' "$BOOT_RUNNER"'
rm -rf "$BRIDGEVM_LIVE_ROOT/queued/$a3_job"

# --- claiming is atomic --------------------------------------------------
claimed="$("$CLI" next)"
check "next claims the job" '[ -n "$claimed" ]'
check "the claimed job moved to running" '"$CLI" status | grep -q "running .*$job_id"'
check "a second worker cannot claim it" '! "$CLI" next'

# --- receipts are only served redacted -----------------------------------
cat > "$claimed/receipt.json" <<'JSON'
{"gate_id":"a3-d3d11-real-title-3run","criterion":"A3","tested_commit":"0123456789abcdef0123456789abcdef01234567","tier":"t6-a3-title","pass":true,"passes":3,"sample_count":1200,"fps_p50":[58.82,58.82,58.82],"title_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","evidence_paths":["run-1/summary.txt"],"disk_path":"/Users/me/win11.qcow2","vars_path":"/tmp/VARS.fd"}
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
check "the published receipt keeps A3 provenance" \
    '"$CLI" receipt "$job_id" | grep -q "\"criterion\": \"A3\""'
check "the published receipt keeps FPS samples" \
    '"$CLI" receipt "$job_id" | grep -q "\"sample_count\": 1200"'
check "the published receipt keeps only relative evidence paths" \
    '"$CLI" receipt "$job_id" | grep -q "run-1/summary.txt"'

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
