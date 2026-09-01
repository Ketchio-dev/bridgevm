#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 4 ]] || exit 2
DIR="$1"; WORKTREE="$2"; JOB_ID="$3"; COMMIT="$4"
PRIVATE="$DIR/private"; mkdir -p "$PRIVATE"
STARTED="$(awk -F= '$1=="submitted_at"{print $2}' "$DIR/job.env")"
MODE=pilot
if [[ -f "$PRIVATE/verified-inputs.json" ]]; then
  MODE="$(python3 -c 'import json,sys; value=json.load(open(sys.argv[1])).get("campaign_mode","pilot"); print(value if value in ("pilot","release") else "pilot")' "$PRIVATE/verified-inputs.json")"
fi
OUTCOME=missing-receipt; FAILURE=missing-tier-receipt
[[ -f "$DIR/cancel.requested" ]] && { OUTCOME=canceled; FAILURE=canceled; }
[[ -f "$PRIVATE/cleanup-failed" ]] && { OUTCOME=cleanup-failed; FAILURE=cleanup-failed; }
python3 "$WORKTREE/scripts/live-gates/write-windows-product-e2e-receipt.py" \
  --out "$DIR/receipt.json" --private "$PRIVATE" --input-manifest "$DIR/input-manifest.tsv" \
  --verified "$PRIVATE/verified-inputs.json" --job-id "$JOB_ID" --commit "$COMMIT" \
  --mode "$MODE" --attempts 0 --started-at "$STARTED" --outcome "$OUTCOME" --failure-code "$FAILURE"
