#!/usr/bin/env bash
# Route A19 live jobs to their tier-specific private-media cleanup fences.

bridgevm_a19_guard_or_fence() {
    local tier="$1" queue_root="$6" guard verify
    case "$tier" in
        t20-a19-native-snapshot-restore)
            guard=native-snapshot-restore-cleanup-fence.sh
            verify=bridgevm_t20_guard_or_fence ;;
        t21-a19-quota-refusal)
            guard=a19-quota-worker-cleanup-fence.sh
            verify=bridgevm_t21_guard_or_fence ;;
        t22-a19-interrupted-restore)
            guard=a19-interrupted-restore-cleanup-fence.sh
            verify=bridgevm_t22_guard_or_fence ;;
        t23-a19-lifecycle-campaign) guard=a19-lifecycle-campaign-cleanup-fence.sh; verify=bridgevm_t23_guard_or_fence ;; *) return 2 ;;
    esac
    source "$(dirname "${BASH_SOURCE[0]}")/$guard" || {
        printf '%s cleanup verifier unavailable\n' "$tier" > "$queue_root/worker-cleanup-required"
        return 126
    }
    "$verify" "$@"
}
