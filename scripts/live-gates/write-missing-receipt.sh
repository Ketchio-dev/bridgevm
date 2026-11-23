#!/usr/bin/env bash
set -euo pipefail
TIER="$1"; DIR="$2"; WORKTREE="$3"; JOB_ID="$4"; COMMIT="$5"; source "$WORKTREE/scripts/live-gates/development-queue-receipt-dispatch.sh"; bridgevm_development_receipt_dispatch finalize "$TIER" "$DIR" "$WORKTREE" "$COMMIT"
[[ -f "$DIR/receipt.json" ]] && exit 0
case "$TIER" in
  t17-windows-hvf-product-e2e|t18-audio-teardown|t19-windows-hvf-import-product-e2e)
    "$WORKTREE/scripts/live-gates/write-live-missing-receipt.sh" "$TIER" "$DIR" "$WORKTREE" "$JOB_ID" "$COMMIT" || true ;;
esac
