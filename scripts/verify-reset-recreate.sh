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
GENERATION=0
# One helper process lifetime: launch, then wait for either READY or exit.
# Windows boot legitimately issues intermediate SYSTEM_RESETs (a passing
# boot shows reboots=3), and in product mode EVERY reset recreates the
# process. An exit-42 before READY is therefore a normal intermediate
# reset: count the generation and relaunch. Only a non-42 exit is a crash.
launch_helper() { # cycle
  CTL="$OUT/cycle-$1.gen-$GENERATION.ctl"; : > "$CTL"
  RUN_DIR="$OUT/cycle-$1-gen-$GENERATION"
  scripts/run-hvf-windows-installed-boot.sh --exit-on-reset \
    --target "$WORK/disk.raw" --vars "$WORK/vars.fd" \
    --evidence-dir "$RUN_DIR" --watchdog-ms $((BOOT_TIMEOUT * 1000)) \
    --ram-mib 6144 --smp-cpus 4 --release \
    --agent-service-control "$CTL" \
    > "$OUT/cycle-$1.gen-$GENERATION.log" 2>&1 &
  LAUNCHER=$!
  RUN_LOG="$RUN_DIR/run.log"
}

record_helper_pid() { # cycle
  HELPER_PID=$(pgrep -f 'hvf_gic_boot_probe' | head -1)
  [[ -n "$HELPER_PID" ]] || fail "cycle $1: no helper process found"
  for seen in "${PIDS[@]:-}"; do
    [[ "$seen" != "$HELPER_PID" ]] || fail "cycle $1: helper PID $HELPER_PID reused"
  done
  PIDS+=("$HELPER_PID")
}

for ((cycle = 0; cycle < CYCLES; cycle++)); do
  echo "=== cycle $cycle ==="
  launch_helper "$cycle"

  # Fresh guest boot marker: this generation's own agent READY. Intermediate
  # resets recreate the process (fresh PID, next generation) and the wait
  # continues against the new generation's log.
  deadline=$((SECONDS + BOOT_TIMEOUT))
  until grep -aq '^BVAGENT READY' "$RUN_LOG" 2>/dev/null; do
    (( SECONDS < deadline )) || fail "cycle $cycle: no agent READY in ${BOOT_TIMEOUT}s"
    if ! kill -0 "$LAUNCHER" 2>/dev/null; then
      wait "$LAUNCHER" && RC=0 || RC=$?
      [[ "$RC" -eq 42 ]] || fail "cycle $cycle: helper died before READY (exit $RC)"
      GENERATION=$((GENERATION + 1))
      echo "cycle $cycle: intermediate reset -> generation $GENERATION (new helper)"
      sync
      printf 'generation: %s\n' "$GENERATION" > "$OUT/reset.receipt"
      launch_helper "$cycle"
    fi
    sleep 2
  done
  record_helper_pid "$cycle"
  echo "cycle $cycle: helper_pid=$HELPER_PID generation=$GENERATION agent READY"

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
  GENERATION=$((GENERATION + 1))
  sync
  printf 'generation: %s\nflushed: %s\nflushed: %s\n' \
    "$GENERATION" "$WORK/disk.raw" "$WORK/vars.fd" > "$OUT/reset.receipt"
done

echo "PASS: $CYCLES reset cycles, each with a fresh helper PID, increasing generation, fresh agent READY, 4 CPUs"
