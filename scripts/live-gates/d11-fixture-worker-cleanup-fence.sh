#!/bin/bash
bridgevm_d11_guard_or_fence() {
    local tier="$1" dir="$2" root="$3" commit="$4" job="$5" queue="$6"
    [[ "$tier" == d11-native-fixture-preparation ]] || return 0
    if [[ -d "$root" && ! -L "$root" ]] && /bin/bash --noprofile --norc -p \
        "$root/scripts/live-gates/d11-fixture-queue-dispatch.sh" guard "$dir" "$root" "$commit" "$job" >/dev/null 2>&1; then
        return 0
    fi
    printf 'D11 job %s has unverified process, mount or evidence cleanup\n' "$job" > "$queue/worker-cleanup-required"
    return 126
}
