#!/usr/bin/env bash
# A19 acceptance item 5: a restored snapshot boots Windows with its marker.
#
# The pair gate proves bytes round-trip. Bytes are not the claim a user cares
# about: they care that rolling back gives them the machine they snapshotted.
# So this writes a marker onto the guest's own C: drive, snapshots, destroys
# that state by writing a *different* marker, restores, boots again, and
# requires the first marker to be what comes back.
#
# The second marker is what makes this a real test. Without it a restore that
# did nothing at all would pass, because the original marker would still be
# sitting there.
#
# The marker lives on C: rather than in the shared folder on purpose: the share
# is host-side storage and is not part of the snapshot, so a marker there would
# survive a restore that did nothing.
set -uo pipefail

REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$REPO"

DISK=${DISK:-$HOME/BridgeVM/work/canonical-fresh-12041-agent-20260730.raw}
VARS=${VARS:-$HOME/BridgeVM/work/canonical-fresh-12041-agent-20260730-vars.fd}
OUT=${OUT:-$HOME/BridgeVM/runs/snapshot-restore-boot-$(date +%Y%m%d-%H%M%S)}
QUOTA=${QUOTA:-$((80 * 1024 * 1024 * 1024))}
BOOT_TIMEOUT=${BOOT_TIMEOUT:-1500}
STEP_TIMEOUT=${STEP_TIMEOUT:-240}
GUEST_MARKER='C:\bv-snapshot-marker.txt'

mkdir -p "$OUT"
fail() { echo "FAIL: $*" >&2; exit 1; }

CLI=target/release/examples/snapshot_pair_cli
cargo +1.97.0 build --release -p bridgevm-hvf --example snapshot_pair_cli --locked \
  > "$OUT/build.log" 2>&1 || fail "snapshot_pair_cli build failed; see $OUT/build.log"

WORK=$OUT/live
mkdir -p "$WORK"
trap 'pkill -f hvf_gic_boot_probe 2>/dev/null || true; rm -rf "$WORK"' EXIT INT TERM
cp -c "$DISK" "$WORK/disk.raw" || fail "clone disk"
cp "$VARS" "$WORK/vars.fd" || fail "copy vars"

send_wait() { # $1 = ctl, $2 = log, $3 = command
  local ctl=$1 log=$2 cmd=$3 before
  before=$(grep -cE '^BVAGENT END ' "$log" 2>/dev/null || true)
  printf '%s\n' "$cmd" >> "$ctl"
  local deadline=$((SECONDS + STEP_TIMEOUT)) n
  while (( SECONDS < deadline )); do
    n=$(grep -cE '^BVAGENT END ' "$log" 2>/dev/null || true)
    (( n > before )) && return 0
    sleep 0.5
  done
  return 1
}

# Boot once: report the marker already on C:, write a new one, power off.
# Writes "<phase>/marker-before.txt" with whatever the guest had on entry.
boot_and_mark() { # $1 = new marker text, $2 = phase name
  local marker=$1 phase=$2
  local pdir=$OUT/$phase
  mkdir -p "$pdir"
  local ctl=$pdir/agent.ctl log=$pdir/run.log
  : > "$ctl"

  scripts/run-hvf-windows-installed-boot.sh \
    --target "$WORK/disk.raw" --vars "$WORK/vars.fd" \
    --evidence-dir "$pdir" --watchdog-ms $((BOOT_TIMEOUT * 1000)) \
    --ram-mib 6144 --smp-cpus 4 \
    --agent-service-control "$ctl" \
    > "$pdir/launcher.out" 2>&1 &
  local launcher=$!

  local deadline=$((SECONDS + BOOT_TIMEOUT))
  while (( SECONDS < deadline )); do
    grep -qE '^BVAGENT SERVICE start' "$log" 2>/dev/null && break
    kill -0 $launcher 2>/dev/null || break
    sleep 1
  done
  if ! grep -qE '^BVAGENT SERVICE start' "$log" 2>/dev/null; then
    kill $launcher 2>/dev/null || true
    pkill -f hvf_gic_boot_probe 2>/dev/null || true
    wait $launcher 2>/dev/null || true
    return 1
  fi

  # Read what is there before writing. This readback is the observation the
  # whole gate turns on, so it happens first and is kept verbatim.
  send_wait "$ctl" "$log" \
    "powershell -NoProfile -Command \"if (Test-Path '$GUEST_MARKER') { Get-Content '$GUEST_MARKER' } else { 'BV-NO-MARKER' }\"" \
    || { kill $launcher 2>/dev/null || true; return 1; }
  awk '/^BVAGENT CMD .*Test-Path/{f=1; next} /^BVAGENT END /{f=0} f' "$log" \
    | tr -d '\r' > "$pdir/marker-before.txt"

  send_wait "$ctl" "$log" \
    "powershell -NoProfile -Command \"Set-Content -NoNewline -Path '$GUEST_MARKER' -Value '$marker'\"" \
    || { kill $launcher 2>/dev/null || true; return 1; }

  printf 'POWEROFF\n' >> "$ctl"
  local off=$((SECONDS + STEP_TIMEOUT))
  while (( SECONDS < off )); do
    kill -0 $launcher 2>/dev/null || break
    sleep 1
  done
  kill $launcher 2>/dev/null || true
  pkill -f hvf_gic_boot_probe 2>/dev/null || true
  wait $launcher 2>/dev/null || true
  return 0
}

echo "=== phase 1: write the marker that must survive ==="
ORIGINAL="BV-ORIGINAL-$(date +%s)"
boot_and_mark "$ORIGINAL" phase1-original \
  || fail "guest never reached agent service state in phase 1"
echo "original marker: $ORIGINAL"

echo "=== phase 2: snapshot the powered-off pair ==="
SNAP=$OUT/snapshot
"$CLI" create --disk "$WORK/disk.raw" --vars "$WORK/vars.fd" \
  --out "$SNAP" --quota "$QUOTA" > "$OUT/create.txt" 2>&1 \
  || fail "snapshot create failed; see $OUT/create.txt"

echo "=== phase 3: overwrite it, so a no-op restore cannot pass ==="
CLOBBER="BV-CLOBBERED-$(date +%s)"
boot_and_mark "$CLOBBER" phase3-clobber \
  || fail "guest never reached agent service state in phase 3"
grep -q "$ORIGINAL" "$OUT/phase3-clobber/marker-before.txt" 2>/dev/null \
  || fail "phase 3 could not read back the marker phase 1 wrote (got: $(head -c 120 "$OUT/phase3-clobber/marker-before.txt" 2>/dev/null)); the marker does not persist across a power cycle, so this gate cannot measure a restore"
echo "clobber marker: $CLOBBER"

echo "=== phase 4: restore ==="
"$CLI" restore --snapshot "$SNAP" --disk "$WORK/disk.raw" --vars "$WORK/vars.fd" \
  > "$OUT/restore.txt" 2>&1 || fail "restore failed; see $OUT/restore.txt"

echo "=== phase 5: boot the restored pair and read the marker ==="
boot_and_mark "BV-FINAL-$(date +%s)" phase5-restored \
  || fail "restored pair never reached agent service state -- the snapshot does not boot"

READBACK=$OUT/phase5-restored/marker-before.txt
if grep -q "$CLOBBER" "$READBACK" 2>/dev/null; then
  fail "restored guest still has the clobbered marker: the restore did not take effect"
fi
grep -q "$ORIGINAL" "$READBACK" 2>/dev/null \
  || fail "restored guest has neither marker; read back: $(head -c 200 "$READBACK" 2>/dev/null)"

echo
echo "PASS: A19 acceptance item 5"
echo "  the restored pair booted Windows to agent service state,"
echo "  the marker written before the snapshot ($ORIGINAL) came back,"
echo "  and the marker written after it ($CLOBBER) did not."
