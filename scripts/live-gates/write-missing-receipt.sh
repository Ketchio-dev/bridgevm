#!/usr/bin/env bash
# Write the tier-specific fail-closed receipt when a runner died before doing so.
set -euo pipefail
TIER="$1"; DIR="$2"; WORKTREE="$3"; JOB_ID="$4"; COMMIT="$5"
[[ -f "$DIR/receipt.json" ]] && exit 0
reason=failed-before-receipt; [[ -f "$DIR/cancel.requested" ]] && reason=canceled
case "$TIER" in
  t6-a3-title)
    python3 "$WORKTREE/scripts/live-gates/write-a3-title-receipt.py" \
      --out "$DIR" --job-id "$JOB_ID" --commit "$COMMIT" --reason "$reason"
    ;;
  t7-windows-closure)
    manifest_hash="$(awk -F= '$1=="input_manifest_sha256"{print $2}' "$DIR/job.env")"
    python3 "$WORKTREE/scripts/live-gates/write-windows-closure-receipt.py" \
      --out "$DIR" --job-id "$JOB_ID" --commit "$COMMIT" \
      --input-manifest-hash "$manifest_hash" --reason "$reason" || true
    ;;
  t11-glyph-scene-pilot) printf '{"tier":"t11-glyph-scene-pilot","criterion":"glyph-diagnostic-only","job_id":"%s","commit":"%s","sample_count":0,"outcome":"%s","pass":false}\n' "$JOB_ID" "$COMMIT" "$reason" >"$DIR/receipt.json" ;;
esac
