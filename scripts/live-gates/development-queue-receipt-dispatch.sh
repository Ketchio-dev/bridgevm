#!/usr/bin/env bash
# Exact development schemas stay out of the generic public redactor.
bridgevm_development_receipt_dispatch() {
    local mode="$1" tier="$2" dir="$3" root="$4" commit="$5"
    case "$tier" in
        d10-t22-owned-pair-preparation|d11-native-fixture-preparation) local helper=t22-pair; [[ "$tier" != d11-native-fixture-preparation ]] || helper=d11-fixture; exec /bin/bash --noprofile --norc -p "$root/scripts/live-gates/$helper-queue-dispatch.sh" "$mode" "$dir" "$root" "$commit" ;;
        d9-b9-real-workload) exec python3 "$root/scripts/live-gates/b9_real_workload_queue.py" "$mode" "$dir" "$root" "$commit" ;;
        d5-guest-input) exec python3 "$root/scripts/live-gates/guest_input_queue.py" "$mode" "$dir" "$commit" ;;
        d4-winpe-companions) exec python3 "$root/scripts/live-gates/winpe_companion_receipt.py" "$mode" "$dir" "$commit" ;;
        d1-windows-media-comparison) exec python3 "$root/scripts/live-gates/windows-media-comparison-queue.py" "$mode" "$dir" "$root" "$commit" ;;
    esac
}
