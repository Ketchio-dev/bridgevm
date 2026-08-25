#!/usr/bin/env bash
set -euo pipefail
TIER="$1"; DIR="$2"; WORKTREE="$3"; JOB_ID="$4"; COMMIT="$5"; reason="${6:-failed-before-receipt}"
[[ -f "$DIR/receipt.json" ]] && exit 0
[[ -f "$DIR/cancel.requested" ]] && reason=canceled
# shellcheck source=scripts/live-gates/missing-receipt-shapes.sh
source "$(dirname "${BASH_SOURCE[0]}")/missing-receipt-shapes.sh"
python_receipt "$TIER" "$DIR" "$WORKTREE" "$JOB_ID" "$COMMIT" "$reason" && exit 0
gate=""; criterion=""; extra=""
flat_receipt_fields "$TIER" || exit 0
printf '{"tier":"%s","gate_id":"%s","criterion":"%s","job_id":"%s","commit":"%s",%s"outcome":"%s","pass":false}\n' \
  "$TIER" "$gate" "$criterion" "$JOB_ID" "$COMMIT" "$extra" "$reason" > "$DIR/receipt.json"
