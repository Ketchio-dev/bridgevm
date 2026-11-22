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
esac
exit 2
