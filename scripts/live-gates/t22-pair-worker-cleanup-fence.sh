#!/usr/bin/env bash
# D10 retains immutable owned media; measured writer absence is mandatory.
bridgevm_d10_guard_or_fence() {
    local tier="$1" dir="$2" worktree="$3" commit="$4" job_id="$5" queue_root="$6"
    [[ "$tier" == d10-t22-owned-pair-preparation ]] || return 0
    local verifier="$worktree/scripts/live-gates/t22_pair_queue.py"
    if [[ -d "$worktree" && ! -L "$worktree" && -f "$verifier" && ! -L "$verifier" \
        && -f "$dir/receipt.json" && ! -L "$dir/receipt.json" ]] \
        && /bin/bash --noprofile --norc -p "${BASH_SOURCE[0]%/*}/t22-pair-source-admission.sh" "$worktree" "$commit" >/dev/null 2>&1 \
        && /bin/bash --noprofile --norc -p "$worktree/scripts/live-gates/t22-pair-queue-dispatch.sh" guard "$dir" "$worktree" "$commit" "$job_id" >/dev/null 2>&1; then
        return 0
    fi
    printf 'D10 job %s has unverified owned preparation, evidence or process cleanup\n' \
        "$job_id" > "$queue_root/worker-cleanup-required"
    return 126
}
