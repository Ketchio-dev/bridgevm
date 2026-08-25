#!/usr/bin/env bash
# Finalize running/ jobs left behind by a dead worker without inventing a pass.
set -euo pipefail

REPO="${BRIDGEVM_REPO:-$(cd "$(dirname "$0")/../.." && pwd)}"
QUEUE_ROOT="${BRIDGEVM_LIVE_ROOT:-$HOME/BridgeVM/live-queue}"
WORK_ROOT="${BRIDGEVM_LIVE_WORK:-$HOME/BridgeVM/live-work}"
FINALIZE="$REPO/scripts/live-gates/finalize-live-job.sh"
shopt -s nullglob

for dir in "$QUEUE_ROOT/running"/*; do
    [[ -d "$dir" ]] || continue
    job_id="$(awk -F= '$1=="job_id"{print $2}' "$dir/job.env" 2>/dev/null || true)"
    tier="$(awk -F= '$1=="tier"{print $2}' "$dir/job.env" 2>/dev/null || true)"
    commit="$(awk -F= '$1=="commit"{print $2}' "$dir/job.env" 2>/dev/null || true)"
    [[ -n "$job_id" ]] || job_id="$(basename "$dir")"
    [[ -n "$tier" ]] || tier=unknown
    [[ -n "$commit" ]] || commit=unknown
    # A dead worker can leave a live tier process group behind; never clean its
    # worktree or evidence while any command still names this exact job.
    pgrep -f "$WORK_ROOT/$job_id|$dir" >/dev/null 2>&1 && continue
    printf 'finished_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >>"$dir/job.env"
    "$FINALIZE" "$tier" "$dir" "$WORK_ROOT/$job_id" "$job_id" "$commit" 1 worker-interrupted || true
    git -C "$REPO" worktree remove --force "$WORK_ROOT/$job_id" >>"$dir/run.log" 2>&1 || true
    rm -rf "${WORK_ROOT:?}/${job_id:?}"
    mv "$dir" "$QUEUE_ROOT/done/$job_id"
done
