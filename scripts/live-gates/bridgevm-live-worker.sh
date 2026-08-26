#!/usr/bin/env bash
# Drain the Studio live-gate queue: claim one job, run its tier at the exact
# sealed commit, publish a redacted receipt.
#
# Runs as a user LaunchAgent. No sudo, no inbound socket, no GitHub
# registration; the queue is a directory this user owns.
set -euo pipefail
# Job control, so each tier runs in its own process group and a cancellation
# can kill the whole tree. Without it `kill -TERM -$pid` fails and the gate's
# children (cargo, the probe) survive the cancellation.
set -m

REPO="${BRIDGEVM_REPO:-$(cd "$(dirname "$0")/../.." && pwd)}"
QUEUE_ROOT="${BRIDGEVM_LIVE_ROOT:-$HOME/BridgeVM/live-queue}"
# shellcheck source=scripts/live-gates/worker-job-finalize.sh
source "$(dirname "${BASH_SOURCE[0]}")/worker-job-finalize.sh"
WORK_ROOT="${BRIDGEVM_LIVE_WORK:-$HOME/BridgeVM/live-work}"
CLI="$REPO/scripts/live-gates/bridgevm-live"

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

    # awk exits 0 when the key is simply absent, so an empty job_id survives
    # set -e and later reaches `rm -rf "$WORK_ROOT/$job_id"`, which would
    # delete the entire work root rather than this job's directory.
    if [[ -z "$job_id" ]]; then
        log "refusing job in $dir: job.env has no job_id"
        return 1
    fi

    log "job $job_id tier=$tier commit=$commit"
    printf 'started_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$dir/job.env"
    # A crash or fork failure must never leave a plausible success behind.
    printf 'result=fail\nexit_code=125\n' > "$dir/result.env"

    local available
    available="$(free_gib)"
    if [ "$available" -lt "$MIN_FREE_GIB" ]; then
        # Refuse rather than free space: canonical images are immutable inputs.
        log "only ${available}GiB free, need ${MIN_FREE_GIB}GiB"
        printf 'result=refused-free-space\navailable_gib=%s\n' "$available" > "$dir/result.env"
        return 1
    fi

    local tier_args=() manifest="$dir/input-manifest.tsv"
    case "$tier" in
        t6-a3-title|t7-windows-closure) tier_args=(--input-manifest "$manifest" --sealed-binary "$dir/hvf_gic_boot_probe") ;;
        t8-pointer-reliability|t12-b4-umd-diagnostic) tier_args=(--input-manifest "$manifest" --sealed-package "$dir/sealed-package") ;; t13-compatibility-observation) tier_args=(--input-manifest "$manifest" --sealed-inputs "$dir/sealed-compatibility" --sealed-package "$dir/sealed-package") ;;
    esac
    if (( ${#tier_args[@]} )) && ! "$REPO/scripts/live-gates/verify-live-sealed-input.sh" "$tier" "$dir" "$REPO"; then
        log "job $job_id sealed input is missing or changed after submission"
        printf 'result=refused-sealed-input\n' > "$dir/result.env"
        return 1
    fi

    # A detached worktree at the sealed SHA. The development checkout keeps
    # moving; a receipt must describe what actually ran.
    local worktree="$WORK_ROOT/$job_id"
    mkdir -p "$WORK_ROOT"
    local status=0
    if git -C "$REPO" worktree add --detach "$worktree" "$commit" >>"$dir/run.log" 2>&1; then
        # Per-job target dir prevents cargo races; caffeinate prevents sleep.
        (
            cd "$worktree"
            export CARGO_TARGET_DIR="$WORK_ROOT/$job_id/target"
            caffeinate -dimsu "$worktree/scripts/live-gates/run-tier.sh" \
                "$tier" --out "$dir" --job-id "$job_id" \
                ${tier_args[@]+"${tier_args[@]}"}
        ) >>"$dir/run.log" 2>&1 &
        local tier_pid=$!
        supervise_and_finalize
    else
        status=$?
        printf 'finished_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$dir/job.env"
        "$REPO/scripts/live-gates/finalize-live-job.sh" \
            "$tier" "$dir" "$worktree" "$job_id" "$commit" "$status" failed-before-receipt || status=1
    fi

    git -C "$REPO" worktree remove --force "$worktree" >>"$dir/run.log" 2>&1 || true
    # The per-job target dir is build output, not evidence, and a few of them
    # are tens of gigabytes. Keeping them is what tripped the free-space guard.
    rm -rf "${WORK_ROOT:?}/${job_id:?}"
    return "$status"
}

main() {
    if ! acquire_lock; then
        log "another worker holds the lock; exiting"
        exit 0
    fi

    "$REPO/scripts/live-gates/recover-live-jobs.sh" || log "abandoned job recovery failed"

    local claimed
    while claimed="$("$CLI" next 2>/dev/null)"; do
        [ -n "$claimed" ] || break
        run_job "$claimed" || log "job in $claimed did not pass"
        mv "$claimed" "$QUEUE_ROOT/done/$(basename "$claimed")"
    done
    log "queue drained"
}

main "$@"
