# Supervise one running tier and hand its evidence to the fail-closed finalizer.
# Sourced by bridgevm-live-worker.sh; uses its job locals and log().
supervise_and_finalize() {
    while kill -0 "$tier_pid" 2>/dev/null; do
        if [[ -f "$dir/cancel.requested" ]]; then
            log "job $job_id canceled; stopping its process group"
            kill -TERM -"$tier_pid" 2>/dev/null || kill -TERM "$tier_pid" 2>/dev/null
            sleep 5
            kill -KILL -"$tier_pid" 2>/dev/null || true
            break
        fi
        sleep 2
    done
    wait "$tier_pid" 2>/dev/null || status=$?
    printf 'finished_at=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >>"$dir/job.env"
    local reason=failed-before-receipt
    [[ -f "$dir/cancel.requested" ]] && reason=canceled
    "$REPO/scripts/live-gates/finalize-live-job.sh" \
        "$tier" "$dir" "$worktree" "$job_id" "$commit" "$status" "$reason" || status=1
}
