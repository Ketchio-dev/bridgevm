#!/usr/bin/env bash
bridgevm_development_guard_or_fence() {
    local tier="$1" queue_root="$6" guard verify
    case "$tier" in
        d9-b9-real-workload) guard=b9-worker-cleanup-fence.sh; verify=bridgevm_b9_guard_or_fence ;;
        d10-t22-owned-pair-preparation|d11-native-fixture-preparation) guard=t22-pair-worker-cleanup-fence.sh; verify=bridgevm_d10_guard_or_fence; [[ "$tier" != d11-native-fixture-preparation ]] || { guard=d11-fixture-worker-cleanup-fence.sh; verify=bridgevm_d11_guard_or_fence; } ;;
        *) return 2 ;;
    esac
    source "$(dirname "${BASH_SOURCE[0]}")/$guard" || {
        printf '%s cleanup verifier unavailable\n' "$tier" > "$queue_root/worker-cleanup-required"
        return 126
    }
    "$verify" "$@"
}
