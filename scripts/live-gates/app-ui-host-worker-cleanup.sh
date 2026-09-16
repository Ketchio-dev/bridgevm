#!/usr/bin/env bash
# Keep the owned bundle path stable until the versioned host's exit is proven.
bridgevm_wait_for_app_ui_host_group() {
    local tier_pid="$1" cancel_path="$2" job_id="$3" tier="$4" output="$5" worktree="$6" commit="$7"
    if [[ "$tier" != d6-app-ui-host-v1 && "$tier" != d6-app-ui-host-v2 ]]; then
        bridgevm_wait_for_tier_group "$tier_pid" "$cancel_path" "$job_id"
        return $?
    fi
    local auditor=app_ui_host_cleanup.py; [[ "$tier" != d6-app-ui-host-v2 ]] || auditor=app_ui_host_v2_cleanup.py
    # The adapter allows12s; retain this group for15s before the existing KILL.
    bridgevm_wait_for_tier_group "$tier_pid" "$cancel_path" "$job_id" 150 || return 1
    if ! python3 "$worktree/scripts/live-gates/$auditor" "$output" "$commit"; then
        BRIDGEVM_TIER_STATUS=126
        return 1
    fi
}
