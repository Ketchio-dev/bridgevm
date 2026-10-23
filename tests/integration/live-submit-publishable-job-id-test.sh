#!/usr/bin/env bash
# Submit refuses a job id that receipt redaction would refuse to publish.
# Every receipt carries its job id, and redact-receipt.py withholds a receipt
# whose job id looks like a disk image, a vars file, or a vTPM or key word. A
# job submitted with such an id ran to the end on 2026-10-23 and then had its
# receipt withheld, so submit refuses the id first, before burning it.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/bridgevm-check.XXXXXX")"
trap 'rm -rf "${WORK:?}"' EXIT
export BRIDGEVM_LIVE_ROOT="$WORK/queue"
SHA="$(git -C "$REPO" rev-parse HEAD)"
LIVE="$REPO/scripts/live-gates/bridgevm-live"
checks=0
fail() { echo "FAIL: $*" >&2; exit 1; }
for id in t17-64b2ecbb-integration-tpm-pilot-r92 job-vtpm-1 job-SWTPM-1 t1-secret-run run-token-1 disk.iso.1 vars-copy-1; do
    status=0
    "$LIVE" submit t1-vtimer --sha "$SHA" --job-id "$id" > "$WORK/out" 2>&1 || status=$?
    [[ "$status" -eq 2 ]] || fail "$id: expected refusal status 2, got $status"
    grep -Fq 'job id would be refused by receipt redaction' "$WORK/out" || fail "$id: refusal did not name redaction"
    [[ ! -e "$BRIDGEVM_LIVE_ROOT/job-ledger/$id" ]] || fail "$id: a refused id was burned"
    checks=$((checks + 3))
done
# Ids redaction publishes are still recorded and queued as before.
for id in t17-5de1f8ca-main-pilot-r91 bvm-5de1f8ca-native-ui-v2-20261023-01; do
    "$LIVE" submit t1-vtimer --sha "$SHA" --job-id "$id" > "$WORK/out" 2>&1 || fail "$id: a publishable id was refused"
    [[ -d "$BRIDGEVM_LIVE_ROOT/job-ledger/$id" ]] || fail "$id: a publishable id was not recorded"
    checks=$((checks + 2))
done
echo "PASS: $checks submit job id publication checks"
