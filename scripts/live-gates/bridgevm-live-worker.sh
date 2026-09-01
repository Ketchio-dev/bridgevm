#!/usr/bin/env bash
# Drain the physical-Mac live-gate queue at the exact sealed commit as a user
# LaunchAgent, with no sudo, inbound socket or GitHub registration.
set -euo pipefail
# Put each tier in its own process group so cancellation kills its whole tree.
set -m

REPO="${BRIDGEVM_REPO:-$(cd "$(dirname "$0")/../.." && pwd)}"
QUEUE_ROOT="${BRIDGEVM_LIVE_ROOT:-$HOME/BridgeVM/live-queue}"
WORK_ROOT="${BRIDGEVM_LIVE_WORK:-$HOME/BridgeVM/live-work}"
CLI="$REPO/scripts/live-gates/bridgevm-live"
RECOVER="$REPO/scripts/live-gates/recover-stale-jobs.sh"
source "$REPO/scripts/live-gates/live-process-cleanup.sh"

# Refuse low space rather than delete canonical Windows media.
MIN_FREE_GIB="${BRIDGEVM_LIVE_MIN_FREE_GIB:-100}"

log() { printf '%s %s\n' "$(date -u +%H:%M:%SZ)" "$*"; }

free_gib() {
    df -g "$HOME" | awk 'NR==2 {print $4}'
}

# Live gates contend for GPU, vCPUs and guest media; run exactly one at a time.
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
    if [[ ! "$commit" =~ ^[0-9a-f]{40}$ ]]; then
        printf 'result=refused-unknown-commit\n' > "$dir/result.env"; return 1
    fi

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

    local tier_args=()
    if [ "$tier" = t6-a3-title ] || [ "$tier" = t7-windows-closure ] || [ "$tier" = t8-pointer-reliability ] || [ "$tier" = t14-bridgevm-pc-windows-start ] || [ "$tier" = t15-hvf-boot-performance ] || [ "$tier" = t16-hvf-nvme-performance ] || [ "$tier" = t17-windows-hvf-product-e2e ]; then
        local manifest="$dir/input-manifest.tsv" sealed_binary="$dir/hvf_gic_boot_probe"
        local expected_manifest actual_manifest expected_binary actual_binary
        expected_manifest="$(awk -F= '$1=="input_manifest_sha256"{print $2}' "$dir/job.env")"
        actual_manifest="$(shasum -a 256 "$manifest" 2>/dev/null | cut -d' ' -f1 || true)"
        if [ -z "$expected_manifest" ] || [ "$actual_manifest" != "$expected_manifest" ]; then
            log "job $job_id manifest is missing or changed after submission"
            printf 'result=refused-sealed-input\n' > "$dir/result.env"
            return 1
        fi
        tier_args=(--input-manifest "$manifest")
        if [ "$tier" != t8-pointer-reliability ] && [ "$tier" != t14-bridgevm-pc-windows-start ] && [ "$tier" != t17-windows-hvf-product-e2e ]; then
            expected_binary="$(awk -F= '$1=="sealed_binary_sha256"{print $2}' "$dir/job.env")"
            actual_binary="$(shasum -a 256 "$sealed_binary" 2>/dev/null | cut -d' ' -f1 || true)"
            if [ -z "$expected_binary" ] || [ "$actual_binary" != "$expected_binary" ]; then
                log "job $job_id binary is missing or changed after submission"
                printf 'result=refused-sealed-input\n' > "$dir/result.env"
                return 1
            fi
            tier_args+=(--sealed-binary "$sealed_binary")
        fi
    fi

    local worktree="$WORK_ROOT/$job_id"
    mkdir -p "$WORK_ROOT"
    if ! git -C "$REPO" cat-file -e "$commit^{commit}" 2>/dev/null; then
        git -C "$REPO" fetch --no-tags origin "$commit" >>"$dir/run.log" 2>&1 || true
    fi
    if ! git -C "$REPO" cat-file -e "$commit^{commit}" 2>/dev/null; then
        log "job $job_id exact commit is unavailable after origin fetch"
        printf 'result=refused-unknown-commit\n' > "$dir/result.env"
        return 1
    fi
    if ! git -C "$REPO" worktree add --detach "$worktree" "$commit" >>"$dir/run.log" 2>&1; then
        log "job $job_id could not create its sealed worktree"
        printf 'result=refused-worktree\n' > "$dir/result.env"
        return 1
    fi
    # Per-job target avoids cargo races; run only an explicit non-secret environment.
    local status=0 clean_user clean_path
    clean_user="$(id -un)"; clean_path="$HOME/.cargo/bin:/opt/homebrew/opt/rustup/bin:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin"
    (
        cd "$worktree"
        /usr/bin/env -i HOME="$HOME" USER="$clean_user" LOGNAME="$clean_user" SHELL=/bin/bash \
            PATH="$clean_path" CARGO_TARGET_DIR="$WORK_ROOT/$job_id/target" \
            /usr/sbin/taskpolicy -a /usr/bin/caffeinate -dimsu "$worktree/scripts/live-gates/run-tier.sh" \
            "$tier" --out "$dir" --job-id "$job_id" \
            ${tier_args[@]+"${tier_args[@]}"}
    ) >>"$dir/run.log" 2>&1 &
    local tier_pid=$!

    if bridgevm_wait_for_tier_group "$tier_pid" "$dir/cancel.requested" "$job_id"; then
        status="$BRIDGEVM_TIER_STATUS"
    else
        printf 'cleanup for tier process group %s was not confirmed\n' "$tier_pid" > "$QUEUE_ROOT/worker-cleanup-required"
        return 126
    fi

    if [ -f "$dir/cancel.requested" ]; then
        log "job $job_id was canceled"
        printf 'result=canceled\n' > "$dir/result.env"
    else
        printf 'result=%s\nexit_code=%s\n' \
            "$([ "$status" -eq 0 ] && echo pass || echo fail)" "$status" > "$dir/result.env"
    fi
    printf 'finished_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$dir/job.env"
    "$worktree/scripts/live-gates/write-missing-receipt.sh" \
        "$tier" "$dir" "$worktree" "$job_id" "$commit"

    # Publish only through the tier-aware redaction/schema boundary.
    if [ ! -f "$dir/receipt.json" ] || ! "$worktree/scripts/live-gates/publish-receipt.sh" "$tier" "$dir" "$worktree" "$commit"; then
        log "receipt for $job_id was refused by validation/redaction; failing job"
        status=1
        printf 'result=fail\nexit_code=1\nreceipt=withheld\n' > "$dir/result.env"
    fi

    git -C "$REPO" worktree remove --force "$worktree" >>"$dir/run.log" 2>&1 || true
    # Per-job target output is reproducible and can be discarded after receipt.
    rm -rf "${WORK_ROOT:?}/${job_id:?}"
    return "$status"
}

main() {
    if ! acquire_lock; then
        log "another worker holds the lock; exiting"
        exit 0
    fi
    [[ ! -f "$QUEUE_ROOT/worker-cleanup-required" ]] || { log "worker is fenced: process-group cleanup needs operator review"; exit 126; }
    "$RECOVER" "$REPO" "$QUEUE_ROOT" "$WORK_ROOT"

    local claimed
    while claimed="$("$CLI" next 2>/dev/null)"; do
        [ -n "$claimed" ] || break
        local run_status=0; run_job "$claimed" || run_status=$?
        [[ "$run_status" != 126 ]] || { log "job in $claimed could not confirm cleanup; leaving it in running and fencing the queue"; exit 126; }
        [[ "$run_status" == 0 ]] || log "job in $claimed did not pass"
        mv "$claimed" "$QUEUE_ROOT/done/$(basename "$claimed")"
    done
    log "queue drained"
}

main "$@"
