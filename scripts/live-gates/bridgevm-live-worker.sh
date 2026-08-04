#!/usr/bin/env bash
# Drain the Studio live-gate queue: claim one job, run its tier at the exact
# sealed commit, publish a redacted receipt.
#
# Runs as a user LaunchAgent. No sudo, no inbound socket, no GitHub
# registration; the queue is a directory this user owns.
set -euo pipefail

REPO="${BRIDGEVM_REPO:-$(cd "$(dirname "$0")/../.." && pwd)}"
QUEUE_ROOT="${BRIDGEVM_LIVE_ROOT:-$HOME/BridgeVM/live-queue}"
WORK_ROOT="${BRIDGEVM_LIVE_WORK:-$HOME/BridgeVM/live-work}"
CLI="$REPO/scripts/live-gates/bridgevm-live"
REDACT="$REPO/scripts/live-gates/redact-receipt.py"

# Canonical Windows media lives on the external SSD and must never be deleted
# to make room. Stop the job instead and say so.
MIN_FREE_GIB="${BRIDGEVM_LIVE_MIN_FREE_GIB:-100}"

log() { printf '%s %s\n' "$(date -u +%H:%M:%SZ)" "$*"; }

free_gib() {
    df -g "$HOME" | awk 'NR==2 {print $4}'
}

# One live gate at a time: they contend for the GPU, the vCPU budget and the
# same guest media. A second concurrent run would not just be slow, it would
# corrupt the evidence.
acquire_lock() {
    local lock="$QUEUE_ROOT/worker.lock"
    mkdir -p "$QUEUE_ROOT"
    if ! mkdir "$lock" 2>/dev/null; then
        local owner
        owner="$(cat "$lock/pid" 2>/dev/null || echo unknown)"
        if [ "$owner" != unknown ] && ! kill -0 "$owner" 2>/dev/null; then
            log "removing lock from dead worker $owner"
            rm -rf "$lock"
            mkdir "$lock" 2>/dev/null || return 1
        else
            return 1
        fi
    fi
    printf '%s\n' "$$" > "$lock/pid"
    # shellcheck disable=SC2064
    trap "rm -rf '$lock'" EXIT
}

run_job() {
    local dir="$1"
    local job_id tier commit
    job_id="$(awk -F= '$1=="job_id"{print $2}' "$dir/job.env")"
    tier="$(awk -F= '$1=="tier"{print $2}' "$dir/job.env")"
    commit="$(awk -F= '$1=="commit"{print $2}' "$dir/job.env")"

    log "job $job_id tier=$tier commit=$commit"
    printf 'started_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$dir/job.env"

    local available
    available="$(free_gib)"
    if [ "$available" -lt "$MIN_FREE_GIB" ]; then
        # Refuse rather than free space: canonical images are immutable inputs.
        log "only ${available}GiB free, need ${MIN_FREE_GIB}GiB"
        printf 'result=refused-free-space\navailable_gib=%s\n' "$available" > "$dir/result.env"
        return 1
    fi

    # A detached worktree at the sealed SHA. The development checkout keeps
    # moving; a receipt must describe what actually ran.
    local worktree="$WORK_ROOT/$job_id"
    mkdir -p "$WORK_ROOT"
    git -C "$REPO" worktree add --detach "$worktree" "$commit" >>"$dir/run.log" 2>&1

    # Per-job target dir so concurrent cargo invocations cannot race, and
    # `caffeinate` because a long gate must not be cut short by sleep.
    local status=0
    (
        cd "$worktree"
        export CARGO_TARGET_DIR="$WORK_ROOT/$job_id/target"
        caffeinate -dimsu "$worktree/scripts/live-gates/run-tier.sh" \
            "$tier" --out "$dir" --job-id "$job_id"
    ) >>"$dir/run.log" 2>&1 || status=$?

    if [ -f "$dir/cancel.requested" ]; then
        log "job $job_id was canceled"
        printf 'result=canceled\n' > "$dir/result.env"
    else
        printf 'result=%s\nexit_code=%s\n' \
            "$([ "$status" -eq 0 ] && echo pass || echo fail)" "$status" > "$dir/result.env"
    fi
    printf 'finished_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$dir/job.env"

    # Publish only the redacted receipt. If redaction refuses, publish nothing
    # and say why: a receipt that names private media must not leak because a
    # gate happened to pass.
    if [ -f "$dir/receipt.json" ]; then
        if ! python3 "$REDACT" --in "$dir/receipt.json" --out "$dir/receipt.public.json"; then
            log "receipt for $job_id was refused by redaction; not publishing"
            printf 'receipt=withheld\n' >> "$dir/result.env"
        fi
    fi

    git -C "$REPO" worktree remove --force "$worktree" >>"$dir/run.log" 2>&1 || true
    return "$status"
}

main() {
    if ! acquire_lock; then
        log "another worker holds the lock; exiting"
        exit 0
    fi

    local claimed
    while claimed="$("$CLI" next 2>/dev/null)"; do
        [ -n "$claimed" ] || break
        run_job "$claimed" || log "job in $claimed did not pass"
        mv "$claimed" "$QUEUE_ROOT/done/$(basename "$claimed")"
    done
    log "queue drained"
}

main "$@"
