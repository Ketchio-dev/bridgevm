#!/usr/bin/env bash
# A15: N consecutive guest SYSTEM_RESETs, each in a new helper process.
#
# The probe runs with --exit-on-reset (the product shape): a guest reset ends
# the helper with exit 42 and this script, acting as the supervisor, launches
# a fresh one. Each cycle checks a new helper PID (the previous one gone), a
# guest boot time that differs from the last cycle's, and 4 online CPUs. The
# last cycle shuts the guest down cleanly. The disk and vars are private APFS
# clones that are deleted afterwards.
#
#   TARGET=disk.raw VARS=vars.fd OUT=new-dir CYCLES=100 scripts/verify-reset-recreate.sh
#
# BRIDGEVM_PREBUILT_PROBE=/abs/hvf_gic_boot_probe uses that release probe
# instead of building one.
set -euo pipefail
fail() { echo "FAIL: $*" >&2; exit 1; }

REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$REPO"
TARGET=${TARGET:?installed Windows disk with the BridgeVM agent}
VARS=${VARS:?its UEFI vars}
OUT=${OUT:?new output directory}
CYCLES=${CYCLES:-3}
BOOT_TIMEOUT=${BOOT_TIMEOUT:-900}
BUILD_ARGS=(--release)
[[ -z "${BRIDGEVM_PREBUILT_PROBE:-}" ]] || BUILD_ARGS+=(--skip-build)

mkdir "$OUT"
WORK=$OUT/work
mkdir "$WORK"
cp -c "$TARGET" "$WORK/disk.raw"
cp -c "$VARS" "$WORK/vars.fd"
chmod 600 "$WORK/disk.raw" "$WORK/vars.fd"
trap 'rm -rf "$WORK"' EXIT

GENERATION=0
PREV_PID=""
PREV_BOOT=""
LAUNCHES=0
launch_helper() { # cycle
  RUN_DIR="$OUT/c$1-g$GENERATION"
  mkdir "$RUN_DIR"
  CTL="$RUN_DIR.ctl"
  : > "$CTL"
  RUN_LOG="$RUN_DIR/run.log"
  LAUNCHED_AT=$SECONDS
  LAUNCHES=$((LAUNCHES + 1))
  scripts/run-hvf-windows-installed-boot.sh --exit-on-reset "${BUILD_ARGS[@]}" \
    --target "$WORK/disk.raw" --vars "$WORK/vars.fd" \
    --evidence-dir "$RUN_DIR" --watchdog-ms $((BOOT_TIMEOUT * 1000)) \
    --ram-mib 6144 --smp-cpus 4 \
    --agent-service-control "$CTL" \
    > "$RUN_DIR.launcher.log" 2>&1 &
  LAUNCHER=$!
}

descendants() { local p; for p in $(pgrep -P "$1"); do echo "$p"; descendants "$p"; done; }

ask() { # command -> its output, from an exact agent receipt
  python3 scripts/snapshot-restore-channel.py "$CTL" "$RUN_LOG" "$1" 120 "$RUN_DIR.resp" \
    || fail "cycle $cycle: no agent receipt for: $1"
  tr -d '\r' < "$RUN_DIR.resp" | sed '/^$/d'
}

for ((cycle = 0; cycle < CYCLES; cycle++)); do
  launch_helper "$cycle"
  deadline=$((SECONDS + BOOT_TIMEOUT))
  # A Windows boot may reset on its own before the agent starts (updates,
  # first boot); each such reset is a normal recreation, not a failure.
  until grep -aq '^BVAGENT SERVICE start' "$RUN_LOG" 2>/dev/null; do
    ((SECONDS < deadline)) || fail "cycle $cycle: no agent service start in ${BOOT_TIMEOUT}s (generation $GENERATION)"
    if ! kill -0 "$LAUNCHER" 2>/dev/null; then
      wait "$LAUNCHER" && RC=0 || RC=$?
      [[ "$RC" -eq 42 ]] || fail "cycle $cycle: helper died before the agent started (exit $RC, generation $GENERATION)"
      GENERATION=$((GENERATION + 1))
      echo "cycle $cycle: intermediate reset -> generation $GENERATION"
      launch_helper "$cycle"
    fi
    sleep 1
  done
  ready_s=$((SECONDS - LAUNCHED_AT))
  HELPER_PID=$(for p in $(descendants "$LAUNCHER"); do
    ps -o comm= -p "$p" | grep -q hvf_gic_boot_probe && echo "$p"
  done | head -1)
  [[ -n "$HELPER_PID" ]] || fail "cycle $cycle: no helper process below the launcher"
  if [[ -n "$PREV_PID" ]]; then
    # macOS recycles PIDs, so compare with the predecessor only.
    [[ "$PREV_PID" != "$HELPER_PID" ]] || fail "cycle $cycle: helper PID $HELPER_PID reused"
    ! kill -0 "$PREV_PID" 2>/dev/null || fail "cycle $cycle: previous helper $PREV_PID still alive"
  fi
  PREV_PID=$HELPER_PID

  cpus=$(ask 'powershell -NoProfile -Command "(Get-CimInstance Win32_ComputerSystem).NumberOfLogicalProcessors"')
  [[ "$cpus" == "4" ]] || fail "cycle $cycle: online CPUs '$cpus', wanted 4"
  boot=$(ask 'powershell -NoProfile -Command "(Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToUniversalTime().ToString(\"o\")"')
  [[ -n "$boot" && "$boot" != "$PREV_BOOT" ]] || fail "cycle $cycle: guest boot time '$boot' is not new (previous '$PREV_BOOT')"
  PREV_BOOT=$boot
  echo "cycle $cycle: pid=$HELPER_PID generation=$GENERATION cpus=$cpus boot=$boot ready_s=$ready_s"

  if ((cycle == CYCLES - 1)); then
    printf 'shutdown /s /t 2\n' >> "$CTL"
    wait "$LAUNCHER" && RC=0 || RC=$?
    [[ "$RC" -eq 0 ]] || fail "final shutdown: helper exit $RC, wanted 0"
    break
  fi
  printf 'shutdown /r /t 2\n' >> "$CTL"
  wait "$LAUNCHER" && RC=0 || RC=$?
  [[ "$RC" -eq 42 ]] || fail "cycle $cycle: helper exit $RC, wanted 42 (guest reset)"
  GENERATION=$((GENERATION + 1))
done

echo "PASS: $CYCLES cycles in $LAUNCHES helper processes (generation $GENERATION), each with a new PID, a new guest boot time and 4 CPUs; clean final shutdown"
