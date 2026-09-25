#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 5 ]] || exit 2
TIER="$1"; DIR="$2"; WORKTREE="$3"; JOB_ID="$4"; COMMIT="$5"
case "$TIER" in
  t17-windows-hvf-product-e2e)
    exec "$WORKTREE/scripts/live-gates/write-windows-product-e2e-missing-receipt.sh" "$DIR" "$WORKTREE" "$JOB_ID" "$COMMIT" ;;
  t18-audio-teardown)
    exec "$WORKTREE/scripts/live-gates/write-audio-teardown-missing-receipt.sh" "$DIR" "$WORKTREE" "$JOB_ID" "$COMMIT" ;;
  t19-windows-hvf-import-product-e2e)
    exec "$WORKTREE/scripts/live-gates/write-windows-import-product-e2e-missing-receipt.sh" "$DIR" "$WORKTREE" "$JOB_ID" "$COMMIT" ;;
  t20-a19-native-snapshot-restore)
    exec python3 "$WORKTREE/scripts/live-gates/native_snapshot_restore_receipt.py" missing "$DIR/receipt.json" --job-id "$JOB_ID" --expected-commit "$COMMIT" ;;
  t21-a19-quota-refusal)
    exec python3 "$WORKTREE/scripts/live-gates/a19_quota_refusal_receipt.py" missing "$DIR/receipt.json" --job-id "$JOB_ID" --expected-commit "$COMMIT" ;;
esac
exit 2
