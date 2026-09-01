#!/usr/bin/env bash
# Reconcile a stale job's receipt without publishing an unauthenticated T17 object.
set -euo pipefail
[[ $# -eq 6 ]] || exit 2
REPO="$1"; WORK_ROOT="$2"; DIR="$3"; JOB_ID="$4"; TIER="$5"; COMMIT="$6"
WORKTREE="$WORK_ROOT/$JOB_ID"; SEALED=false
if [[ -d "$WORKTREE" ]]; then
  SEALED=true
  "$WORKTREE/scripts/live-gates/write-missing-receipt.sh" "$TIER" "$DIR" "$WORKTREE" "$JOB_ID" "$COMMIT" || true
  if [[ -f "$DIR/receipt.json" && ! -e "$DIR/receipt.public.json" ]]; then
    "$WORKTREE/scripts/live-gates/publish-receipt.sh" "$TIER" "$DIR" "$WORKTREE" "$COMMIT" || printf 'receipt=withheld\n' >> "$DIR/result.env"
  fi
  git -C "$WORKTREE" worktree remove --force "$WORKTREE" || true
fi
if [[ "$SEALED" == false && "$TIER" != t17-windows-hvf-product-e2e && -f "$DIR/receipt.json" && ! -e "$DIR/receipt.public.json" ]]; then
  python3 "$REPO/scripts/live-gates/redact-receipt.py" --in "$DIR/receipt.json" --out "$DIR/receipt.public.json" || true
elif [[ "$SEALED" == false && "$TIER" == t17-windows-hvf-product-e2e ]]; then
  printf 'receipt=withheld-no-sealed-worktree\n' >> "$DIR/result.env"
fi
