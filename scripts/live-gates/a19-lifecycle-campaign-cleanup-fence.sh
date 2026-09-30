#!/usr/bin/env bash
# Keep a T23 campaign job in running unless every lane's media cleanup is proven.

bridgevm_t23_guard_or_fence() {
    local tier="$1" dir="$2" worktree="$3" commit="$4" job_id="$5" queue_root="$6" verifier
    [[ "$tier" == t23-a19-lifecycle-campaign ]] || return 0
    verifier="$worktree/scripts/live-gates/a19_lifecycle_campaign_read.py"
    if [[ -d "$worktree" && ! -L "$worktree" && -d "$dir" && ! -L "$dir" \
        && -f "$verifier" && ! -L "$verifier" \
        && -f "$dir/receipt.json" && ! -L "$dir/receipt.json" \
        && "$(git -C "$worktree" rev-parse HEAD 2>/dev/null)" == "$commit" ]] && \
        python3 "$verifier" fence "$dir" --expected-commit "$commit" --job-id "$job_id" >/dev/null 2>&1
    then
        return 0
    fi
    printf 'T23 job %s has unverified private-media cleanup\n' "$job_id" > "$queue_root/worker-cleanup-required"
    return 126
}
