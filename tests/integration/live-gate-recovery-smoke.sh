#!/usr/bin/env bash
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"; WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
root="$WORK/queue"; work_root="$WORK/work"
mkdir -p "$root/running/orphan" "$root/done" "$work_root"
printf 'job_id=orphan\ntier=t10-qmp-stress\ncommit=deadbeef\n' >"$root/running/orphan/job.env"
BRIDGEVM_LIVE_ROOT="$root" BRIDGEVM_LIVE_WORK="$work_root" BRIDGEVM_REPO="$REPO" "$REPO/scripts/live-gates/recover-live-jobs.sh"
test -f "$root/done/orphan/receipt.public.json"
grep -q '^result=fail$' "$root/done/orphan/result.env"
grep -q '"pass": false' "$root/done/orphan/receipt.public.json"
echo "PASS: abandoned live job recovery fails closed"
