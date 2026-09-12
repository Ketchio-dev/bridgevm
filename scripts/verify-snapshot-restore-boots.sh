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
cd "$REPO" || exit 1

# The pair gate uses canonical-fresh-12041-agent, which despite its name has
# no ARM64 virtio-serial driver -- the agent channel does not exist there, so
# the guest boots to a working desktop and never answers. Measured: a full
# Windows desktop in ramfb and zero BVAGENT lines. This pair has vioser
# injected, which is what a marker test needs.
DISK=${DISK:-$HOME/BridgeVM/work/rethink-fresh-12041-agent-vioserial.raw}
VARS=${VARS:-$HOME/BridgeVM/work/rethink-fresh-12041-agent-vioserial-vars.fd}
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
mkdir "$WORK" || fail "work directory must be new"
source "$REPO/scripts/snapshot-restore-lifecycle.sh"
trap snapshot_cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
cp -c "$DISK" "$WORK/disk.raw" || fail "clone disk"
cp "$VARS" "$WORK/vars.fd" || fail "copy vars"

send_wait() { # ctl, log, command, isolated response path
  python3 "$REPO/scripts/snapshot-restore-channel.py" "$1" "$2" "$3" "$STEP_TIMEOUT" "$4"
}

# Boot once: report the marker already on C:, write a new one, power off.
# Writes "<phase>/marker-before.txt" with whatever the guest had on entry.
boot_and_mark() { # $1 = new marker text, $2 = phase name
  local marker=$1 phase=$2
  local pdir=$OUT/$phase
  mkdir "$pdir" || return 1
  local ctl=$pdir/agent.ctl log=$pdir/run.log
  local probe_args=(); [[ -z "${BRIDGEVM_PREBUILT_PROBE:-}" ]] || probe_args+=(--release --skip-build)
  : > "$ctl"

  scripts/run-hvf-windows-installed-boot.sh \
    --target "$WORK/disk.raw" --vars "$WORK/vars.fd" \
    --evidence-dir "$pdir" --watchdog-ms $((BOOT_TIMEOUT * 1000)) \
    --ram-mib 6144 --smp-cpus 4 \
    --agent-service-control "$ctl" ${probe_args[@]+"${probe_args[@]}"} \
    > "$pdir/launcher.out" 2>&1 &
  local launcher=$!
  SNAPSHOT_LAUNCHER=$launcher

  local deadline=$((SECONDS + BOOT_TIMEOUT))
  while (( SECONDS < deadline )); do
    grep -qE '^BVAGENT SERVICE start' "$log" 2>/dev/null && break
    kill -0 $launcher 2>/dev/null || break
    sleep 1
  done
  if ! grep -qE '^BVAGENT SERVICE start' "$log" 2>/dev/null; then
    # Distinguish "the guest did not boot" from "the guest booted but has no
    # agent". They look identical from the control file and need different
    # fixes; the second one is an image problem, not a VMM problem.
    if grep -qE 'ramfb checkpoint' "$log" 2>/dev/null; then
      echo "  the guest produced framebuffer output but no BVAGENT line:" >&2
      echo "  this image most likely has no virtio-serial driver or no agent" >&2
    fi
    snapshot_stop_launcher
    return 1
  fi

  # Read what is there before writing. This readback is the observation the
  # whole gate turns on, so it happens first and is kept verbatim.
  send_wait "$ctl" "$log" \
    "powershell -NoProfile -Command \"\$ErrorActionPreference='Stop'; if (Test-Path '$GUEST_MARKER') { Get-Content '$GUEST_MARKER' } else { 'BV-NO-MARKER' }\"" \
    "$pdir/marker-before.txt" || { snapshot_stop_launcher; return 1; }

  send_wait "$ctl" "$log" \
    "powershell -NoProfile -Command \"\$ErrorActionPreference='Stop'; Set-Content -NoNewline -Encoding ascii -Path '$GUEST_MARKER' -Value '$marker'; if ((Get-Content -Raw '$GUEST_MARKER') -cne '$marker') { exit 1 }; Write-Output '$marker'\"" \
    "$pdir/marker-after.txt" || { snapshot_stop_launcher; return 1; }
  [[ $(cat "$pdir/marker-after.txt") == "$marker" ]] || { snapshot_stop_launcher; return 1; }
  snapshot_shutdown "$ctl" "$log"
}

echo "=== phase 1: write the marker that must survive ==="
ORIGINAL="BV-ORIGINAL-$(date +%s)"
boot_and_mark "$ORIGINAL" phase1-original \
  || fail "guest never reached agent service state in phase 1"
echo "original marker: $ORIGINAL"

echo "=== phase 2: snapshot the powered-off pair ==="
SNAP=$OUT/snapshot
"$CLI" create "$WORK/disk.raw" "$WORK/vars.fd" "$SNAP" a19-restore-boot "$QUOTA" \
  > "$OUT/create.txt" 2>&1 || fail "snapshot create failed; see $OUT/create.txt"
"$CLI" verify "$SNAP" > "$OUT/verify.txt" 2>&1 \
  || fail "snapshot verify failed; see $OUT/verify.txt"

echo "=== phase 3: overwrite it, so a no-op restore cannot pass ==="
CLOBBER="BV-CLOBBERED-$(date +%s)"
boot_and_mark "$CLOBBER" phase3-clobber \
  || fail "guest never reached agent service state in phase 3"
[[ $(cat "$OUT/phase3-clobber/marker-before.txt") == "$ORIGINAL" ]] \
  || fail "phase 3 could not read back the marker phase 1 wrote (got: $(head -c 120 "$OUT/phase3-clobber/marker-before.txt" 2>/dev/null)); the marker does not persist across a power cycle, so this gate cannot measure a restore"
echo "clobber marker: $CLOBBER"

echo "=== phase 4: restore ==="
"$CLI" restore "$SNAP" "$WORK/disk.raw" "$WORK/vars.fd" \
  > "$OUT/restore.txt" 2>&1 || fail "restore failed; see $OUT/restore.txt"

echo "=== phase 5: boot the restored pair and read the marker ==="
boot_and_mark "BV-FINAL-$(date +%s)" phase5-restored \
  || fail "restored pair never reached agent service state -- the snapshot does not boot"

READBACK=$OUT/phase5-restored/marker-before.txt
if grep -q "$CLOBBER" "$READBACK" 2>/dev/null; then
  fail "restored guest still has the clobbered marker: the restore did not take effect"
fi
[[ $(cat "$READBACK") == "$ORIGINAL" ]] \
  || fail "restored guest has neither marker; read back: $(head -c 200 "$READBACK" 2>/dev/null)"

echo
echo "PASS: A19 acceptance item 5"
echo "  the restored pair booted Windows to agent service state,"
echo "  the marker written before the snapshot ($ORIGINAL) came back,"
echo "  and the marker written after it ($CLOBBER) did not."
