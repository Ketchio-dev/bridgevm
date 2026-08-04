#!/usr/bin/env bash
# T1 gate: build, sign and run the bare-metal vtimer/cancellation microprobe.
#
# This reproduces the host-side condition behind the A1 boot stall -- a guest
# parked in WFI whose virtual-timer fire was swallowed by an hv_vcpus_exit
# cancellation -- in seconds rather than the ~20 minutes a Windows boot costs.
#
# A pass is a precondition for spending an A1 boot campaign on a candidate fix.
# It is NOT evidence that A1 is closed: this exercises the timer/cancellation
# path only, against a ~40 instruction guest.
set -euo pipefail

cd "$(dirname "$0")/.."

ITERATIONS=10000
CANCEL_INTERVAL_US=0
STALL_TIMEOUT_MS=5000
ARM_TICKS=1000
OUT=""
SKIP_BUILD=0
EXTRA=()

usage() {
    cat <<'USAGE'
usage: scripts/run-hvf-vtimer-cancel-gate.sh [options]

  --iterations N           timer wakes the guest must complete (default 10000)
  --cancel-interval-us N   microseconds between cancellations (default 0, max pressure)
  --arm-ticks N            counter ticks from arm to deadline (default 1000)
  --stall-timeout-ms N     no-progress window before failing (default 5000)
  --out DIR                write receipt and log to DIR
  --skip-build             use the already-built, already-signed binary
  --quiesce-probe          on the first swallowed fire, halt cancellation and
                           report whether the wake still arrives
  --no-recover             disable the recovery under test (expected to fail)
  -h, --help               print this message
USAGE
}

while [ $# -gt 0 ]; do
    case "$1" in
        --iterations) ITERATIONS="$2"; shift 2 ;;
        --cancel-interval-us) CANCEL_INTERVAL_US="$2"; shift 2 ;;
        --arm-ticks) ARM_TICKS="$2"; shift 2 ;;
        --stall-timeout-ms) STALL_TIMEOUT_MS="$2"; shift 2 ;;
        --out) OUT="$2"; shift 2 ;;
        --skip-build) SKIP_BUILD=1; shift ;;
        --quiesce-probe|--no-recover) EXTRA+=("$1"); shift ;;
        -h|--help) usage; exit 0 ;;
        *) echo "unknown argument $1" >&2; usage >&2; exit 2 ;;
    esac
done

BIN=target/debug/examples/hvf_vtimer_cancel_probe

if [ "$SKIP_BUILD" -eq 0 ]; then
    cargo build -p bridgevm-hvf --example hvf_vtimer_cancel_probe
    # The probe needs com.apple.security.hypervisor to create a VM at all.
    codesign --sign - --entitlements apps/macos/HvfRunner.entitlements --force "$BIN"
fi

if [ ! -x "$BIN" ]; then
    echo "missing $BIN; run without --skip-build" >&2
    exit 1
fi

# A later `cargo build` of any target rewrites this binary and drops the
# signature, so --skip-build cannot assume the entitlement survived. Without it
# hv_vm_create fails with -85377017 and the probe panics before doing anything.
if ! codesign -d --entitlements - "$BIN" 2>&1 | grep -q 'com.apple.security.hypervisor'; then
    echo "$BIN is not signed with com.apple.security.hypervisor; re-signing" >&2
    codesign --sign - --entitlements apps/macos/HvfRunner.entitlements --force "$BIN"
fi

ARGS=(
    --iterations "$ITERATIONS"
    --cancel-interval-us "$CANCEL_INTERVAL_US"
    --arm-ticks "$ARM_TICKS"
    --stall-timeout-ms "$STALL_TIMEOUT_MS"
)
[ ${#EXTRA[@]} -gt 0 ] && ARGS+=("${EXTRA[@]}")

if [ -n "$OUT" ]; then
    mkdir -p "$OUT"
    ARGS+=(--receipt "$OUT/vtimer-cancel-receipt.json")
    set +e
    "$BIN" "${ARGS[@]}" 2>&1 | tee "$OUT/run.log"
    status=${PIPESTATUS[0]}
    set -e
    echo "receipt: $OUT/vtimer-cancel-receipt.json"
    exit "$status"
fi

exec "$BIN" "${ARGS[@]}"
