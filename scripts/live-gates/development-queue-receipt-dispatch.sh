#!/usr/bin/env bash
# Exact development schemas stay out of the generic public redactor.
bridgevm_development_receipt_dispatch() {
    local mode="$1" tier="$2" dir="$3" root="$4" commit="$5"
    case "$tier" in
        d5-guest-input) exec python3 "$root/scripts/live-gates/guest_input_queue.py" "$mode" "$dir" "$commit" ;;
    esac
}
