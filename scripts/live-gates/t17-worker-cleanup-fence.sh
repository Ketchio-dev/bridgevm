#!/usr/bin/env bash
# Do not release a T17 worktree while its ownership cleanup is unproven.
bridgevm_t17_cleanup_proved() {
    local tier="$1" dir="$2" worktree="$3" commit="$4" job_id="$5" verifier
    [[ "$tier" == t17-windows-hvf-product-e2e ]] || return 0
    verifier="$worktree/scripts/verify-windows-product-e2e-receipt.py"
    [[ -d "$worktree" && ! -L "$worktree" && -f "$verifier" && ! -L "$verifier" \
        && -f "$dir/receipt.json" && ! -L "$dir/receipt.json" ]] || return 1
    [[ "$(git -C "$worktree" rev-parse HEAD 2>/dev/null)" == "$commit" ]] || return 1
    python3 - "$verifier" "$dir/receipt.json" "$commit" "$job_id" >/dev/null 2>&1 <<'PY'
import importlib.util
from pathlib import Path
import sys

verifier, receipt, commit, job_id = sys.argv[1:]
spec = importlib.util.spec_from_file_location("t17_receipt", verifier)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
value = module.validate(module.load_receipt(Path(receipt).read_text(encoding="utf-8")),
                        expected_commit=commit)
raise SystemExit(0 if value["job_id"] == job_id and value["worker_cleanup_verified"] is True else 1)
PY
}
bridgevm_t17_guard_or_fence() {
    local tier="$1" dir="$2" worktree="$3" commit="$4" job_id="$5" queue_root="$6"
    if [[ "$tier" == t21-a19-quota-refusal || "$tier" == t22-a19-interrupted-restore ]]; then
        if [[ "$tier" == t21* ]]; then local guard=a19-quota-worker-cleanup-fence.sh verify=bridgevm_t21_guard_or_fence; else local guard=a19-interrupted-restore-cleanup-fence.sh verify=bridgevm_t22_guard_or_fence; fi; source "$(dirname "${BASH_SOURCE[0]}")/$guard" || { printf '%s cleanup verifier unavailable\n' "$tier" > "$queue_root/worker-cleanup-required"; return 126; }
        "$verify" "$@"; return $?
    fi
    bridgevm_t17_cleanup_proved "$tier" "$dir" "$worktree" "$commit" "$job_id" && return 0
    printf 'T17 job %s has unverified cleanup\n' "$job_id" > "$queue_root/worker-cleanup-required"
    return 126
}
bridgevm_worker_publish_receipt() {
    local tier="$1" dir="$2" worktree="$3" commit="$4" job_id="$5" queue_root="$6" status="$7"
    "$worktree/scripts/live-gates/write-missing-receipt.sh" \
        "$tier" "$dir" "$worktree" "$job_id" "$commit"
    if [[ ! -f "$dir/receipt.json" ]] || \
        ! "$worktree/scripts/live-gates/publish-receipt.sh" "$tier" "$dir" "$worktree" "$commit"; then
        log "receipt for $job_id was refused by validation/redaction; failing job"
        status=1
        printf 'result=fail\nexit_code=1\nreceipt=withheld\n' > "$dir/result.env"
    fi
    bridgevm_t17_guard_or_fence "$tier" "$dir" "$worktree" "$commit" "$job_id" "$queue_root" || return 126
    BRIDGEVM_RECEIPT_STATUS="$status"
}
bridgevm_recover_stale_receipt() {
    local repo="$1" queue_root="$2" work_root="$3" dir="$4" job_id="$5" tier="$6" commit="$7"
    bridgevm_t17_guard_or_fence "$tier" "$dir" "$work_root/$job_id" "$commit" "$job_id" "$queue_root" || return 126
    if [[ ! -f "$dir/result.env" ]]; then
        printf 'result=interrupted-worker-exit\nexit_code=unknown\n' > "$dir/result.env"
    fi
    grep -q '^finished_at=' "$dir/job.env" || \
        printf 'finished_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$dir/job.env"
    "$repo/scripts/live-gates/recover-stale-receipt.sh" \
        "$repo" "$work_root" "$dir" "$job_id" "$tier" "$commit"
}
