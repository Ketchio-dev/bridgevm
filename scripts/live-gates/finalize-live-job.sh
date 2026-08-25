#!/usr/bin/env bash
# A queue result can say pass only when both the tier and public receipt do.
set -euo pipefail

TIER="$1"; DIR="$2"; WORKTREE="$3"; JOB_ID="$4"; COMMIT="$5"
TIER_STATUS="$6"; REASON="${7:-failed-before-receipt}"
REPO="${BRIDGEVM_REPO:-$(cd "$(dirname "$0")/../.." && pwd)}"
REDACT="$REPO/scripts/live-gates/redact-receipt.py"
MISSING="$REPO/scripts/live-gates/write-missing-receipt.sh"

receipt_status=1
withheld=0
rm -f "$DIR/receipt.public.json"
"$MISSING" "$TIER" "$DIR" "$WORKTREE" "$JOB_ID" "$COMMIT" "$REASON" || true
if [[ -f "$DIR/receipt.json" ]]; then
    if python3 "$REDACT" --in "$DIR/receipt.json" --out "$DIR/receipt.public.json"; then
        if python3 -c 'import json,sys;sys.exit(json.load(open(sys.argv[1])).get("pass") is not True)' "$DIR/receipt.public.json"; then
            receipt_status=0
        fi
    else
        withheld=1
    fi
fi

result=fail
status=1
if [[ -f "$DIR/cancel.requested" || "$REASON" == canceled ]]; then
    result=canceled
elif [[ "$TIER_STATUS" -eq 0 && "$receipt_status" -eq 0 ]]; then
    result=pass
    status=0
fi
printf 'result=%s\nexit_code=%s\n' "$result" "$status" >"$DIR/result.env"
(( withheld == 0 )) || printf 'receipt=withheld\n' >>"$DIR/result.env"
exit "$status"
