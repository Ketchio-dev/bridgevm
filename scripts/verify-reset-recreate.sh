#!/usr/bin/env bash
# A15 harness: N consecutive guest SYSTEM_RESETs, each producing a NEW helper
# process (fresh PID), an increasing reset generation, and a fresh guest boot
# (agent READY line from that boot). The probe runs with
# BRIDGEVM_EXIT_ON_RESET=1 so a guest reset exits the process with code 42;
# this script is the supervisor: flush, receipt, then a fresh helper.
#
# The full A15 gate is 100 cycles with 4-online-CPU checks; CYCLES=100 runs
# that. Smaller CYCLES prove the machinery without the wall-clock cost.
set -euo pipefail

REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$REPO"
fail() { echo "FAIL: $*" >&2; exit 1; }

TARGET=${TARGET:-$HOME/BridgeVM/work/canonical-attach-resident-20260731.raw}
VARS=${VARS:-$HOME/BridgeVM/work/canonical-attach-resident-20260731-vars.fd}
OUT=${OUT:-$HOME/BridgeVM/runs/reset-recreate-$(date +%Y%m%d-%H%M%S)}
CYCLES=${CYCLES:-3}
BOOT_TIMEOUT=${BOOT_TIMEOUT:-900}

[[ -f "$TARGET" ]] || fail "target image missing: $TARGET"
[[ -f "$VARS" ]] || fail "vars missing: $VARS"

WORK=$HOME/BridgeVM/work/reset-recreate-verify
rm -rf "$WORK"; mkdir -p "$WORK" "$OUT"
cp -c "$TARGET" "$WORK/disk.raw"
cp "$VARS" "$WORK/vars.fd"

declare -a PIDS=()
for ((cycle = 0; cycle < CYCLES; cycle++)); do
  CTL="$OUT/cycle-$cycle.ctl"; : > "$CTL"
  LOG="$OUT/cycle-$cycle.log"
  echo "=== cycle $cycle (generation $cycle) ==="
  scripts/run-hvf-windows-installed-boot.sh --exit-on-reset \
    --target "$WORK/disk.raw" --vars "$WORK/vars.fd" \
    --evidence-dir "$OUT/cycle-$cycle" --watchdog-ms $((BOOT_TIMEOUT * 1000)) \
    --ram-mib 6144 --smp-cpus 4 --release \
    --agent-service-control "$CTL" \
    > "$LOG" 2>&1 &
  LAUNCHER=$!
  RUN_LOG="$OUT/cycle-$cycle/run.log"

  # Fresh guest boot marker: this cycle's own agent READY.
  deadline=$((SECONDS + BOOT_TIMEOUT))
  until grep -aq '^BVAGENT READY' "$RUN_LOG" 2>/dev/null; do
    (( SECONDS < deadline )) || fail "cycle $cycle: no agent READY in ${BOOT_TIMEOUT}s"
    kill -0 "$LAUNCHER" 2>/dev/null || fail "cycle $cycle: helper died before READY"
    sleep 2
  done
  HELPER_PID=$(pgrep -f 'hvf_gic_boot_probe' | head -1)
  [[ -n "$HELPER_PID" ]] || fail "cycle $cycle: no helper process found"
  for seen in "${PIDS[@]:-}"; do
    [[ "$seen" != "$HELPER_PID" ]] || fail "cycle $cycle: helper PID $HELPER_PID reused"
  done
  PIDS+=("$HELPER_PID")
  echo "cycle $cycle: helper_pid=$HELPER_PID agent READY"

  # 4 online CPUs, asked of the guest itself.
  printf 'powershell -NoProfile -Command "(Get-CimInstance Win32_ComputerSystem).NumberOfLogicalProcessors"\n' >> "$CTL"
  cpu_deadline=$((SECONDS + 120))
  until tr -d '\r' < "$RUN_LOG" | grep -aq '^4$'; do
    (( SECONDS < cpu_deadline )) || fail "cycle $cycle: no 4-CPU answer in 120s"
    sleep 2
  done
  echo "cycle $cycle: online_cpus=4"

  # Ask the guest for a SYSTEM_RESET; the helper must EXIT (code 42).
  printf 'shutdown /r /t 2\n' >> "$CTL"
  wait "$LAUNCHER" && RC=0 || RC=$?
  [[ "$RC" -eq 42 ]] || fail "cycle $cycle: helper exit $RC, wanted 42 (reset)"
  echo "cycle $cycle: helper exited 42 (guest reset)"

  # Supervisor order: flush, then the generation-tagged receipt.
  sync
  printf 'generation: %s\nflushed: %s\nflushed: %s\n' \
    "$cycle" "$WORK/disk.raw" "$WORK/vars.fd" > "$OUT/reset.receipt"
done

echo "PASS: $CYCLES reset cycles, each with a fresh helper PID, increasing generation, fresh agent READY, 4 CPUs"
