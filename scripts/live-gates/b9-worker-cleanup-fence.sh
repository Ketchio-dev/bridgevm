#!/usr/bin/env bash
# A B9 diagnostic may leave private guest media and a writable clone on failure.
bridgevm_b9_guard_or_fence() {
    local tier="$1" dir="$2" worktree="$3" commit="$4" job_id="$5" queue_root="$6"
    [[ "$tier" == d9-b9-real-workload ]] || return 0
    local verifier="$worktree/scripts/live-gates/b9_real_workload_receipt.py"
    if [[ -d "$worktree" && ! -L "$worktree" && -f "$verifier" && ! -L "$verifier" \
        && -f "$dir/receipt.json" && ! -L "$dir/receipt.json" ]] \
        && [[ "$(git -C "$worktree" rev-parse HEAD 2>/dev/null)" == "$commit" ]] \
        && python3 "$verifier" guard "$dir" "$commit" "$job_id" >/dev/null 2>&1; then
        return 0
    fi
    printf 'B9 job %s has unverified owned process, clones, share or artifact cleanup\n' \
        "$job_id" > "$queue_root/worker-cleanup-required"
    return 126
}
