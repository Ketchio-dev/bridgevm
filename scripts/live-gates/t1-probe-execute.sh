#!/usr/bin/env bash
# The counter verdict stays in the probe; this stage binds actual evidence.
run_t1_probe() {
    local temporary=0 status recovery=1 quiesce=0 flag
    local pipe_status=()
    if [[ -z "$OUT" ]]; then OUT="$(mktemp -d)"; temporary=1; fi
    mkdir -p "$OUT"
    [[ ! -L "$OUT" && ! -e "$OUT/receipt.json" && ! -e "$OUT/vtimer-cancel-receipt.json" \
        && ! -e "$OUT/t1-run-config.json" && ! -e "$OUT/t1-build-seal.json" && ! -e "$OUT/t1-probe.log" ]] || return 1
    for flag in ${EXTRA[@]+"${EXTRA[@]}"}; do
        [[ "$flag" != --no-recover ]] || recovery=0
        [[ "$flag" != --quiesce-probe ]] || quiesce=1
    done
    ARGS+=(--receipt "$OUT/vtimer-cancel-receipt.json")
    python3 "$PWD/scripts/live-gates/t1_probe_receipt.py" prepare-run "$OUT" "$BIN" "$BUILD_SEAL" \
        "$JOB_ID" "$ITERATIONS" "$ARM_TICKS" "$CANCEL_INTERVAL_US" "$STALL_TIMEOUT_MS" "$recovery" "$quiesce" "${ARGS[@]}"
    set +e
    "$BIN" "${ARGS[@]}" 2>&1 | tee "$OUT/t1-probe.log"
    pipe_status=("${PIPESTATUS[@]}")
    status=${pipe_status[0]}
    set -e
    [[ "${pipe_status[1]}" == 0 ]] || return 1
    python3 "$PWD/scripts/live-gates/t1_probe_receipt.py" finalize "$OUT" "$BIN" "$BUILD_SEAL" \
        "$JOB_ID" "$status" "$ARM_TICKS" "$CANCEL_INTERVAL_US" "$STALL_TIMEOUT_MS" "$recovery" "$quiesce" || return 1
    echo "receipt: $OUT/vtimer-cancel-receipt.json"
    [[ "$temporary" == 0 ]] || rm -rf "$OUT"
    return "$status"
}
