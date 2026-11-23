#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 4 ]] || exit 2
TIER="$1"; RECEIPT="$2"; WORKTREE="$3"; COMMIT="$4"
case "$TIER" in
  t17-windows-hvf-product-e2e)
    exec python3 "$WORKTREE/scripts/verify-windows-product-e2e-receipt.py" "$RECEIPT" --expected-commit "$COMMIT" ;;
  t19-windows-hvf-import-product-e2e)
    exec python3 "$WORKTREE/scripts/verify-windows-import-product-e2e-receipt.py" "$RECEIPT" --expected-commit "$COMMIT" ;;
esac
