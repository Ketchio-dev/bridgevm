#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 4 ]] || exit 2
TIER="$1"; RECEIPT="$2"; WORKTREE="$3"; COMMIT="$4"
case "$TIER" in
  t17-windows-hvf-product-e2e)
    exec python3 "$WORKTREE/scripts/verify-windows-product-e2e-receipt.py" "$RECEIPT" --expected-commit "$COMMIT" ;;
  t19-windows-hvf-import-product-e2e)
    exec python3 "$WORKTREE/scripts/verify-windows-import-product-e2e-receipt.py" "$RECEIPT" --expected-commit "$COMMIT" ;;
  t20-a19-native-snapshot-restore)
    exec python3 "$WORKTREE/scripts/live-gates/native_snapshot_restore_receipt.py" verify "$RECEIPT" --expected-commit "$COMMIT" ;;
  t21-a19-quota-refusal)
    exec python3 "$WORKTREE/scripts/live-gates/a19_quota_refusal_receipt.py" verify "$RECEIPT" --expected-commit "$COMMIT" --job-dir "$(dirname "$RECEIPT")" ;; d9-b9-real-workload) mode=verify-private; [[ "$RECEIPT" != *.public.json ]] || mode=verify-public; exec python3 "$WORKTREE/scripts/live-gates/b9_real_workload_receipt.py" "$mode" "$(dirname "$RECEIPT")" "$COMMIT" ;;
esac
