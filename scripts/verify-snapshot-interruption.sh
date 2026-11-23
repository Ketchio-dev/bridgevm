#!/usr/bin/env bash
# A19 interrupted snapshot operations, on private APFS clones of an installed
# Windows disk and its UEFI vars (deleted afterwards).
#
# Trials cycle through `bridgevm app snapshot-create`, `snapshot-restore` and
# `snapshot-export`. Each SIGKILLs the CLI and its snapshot_pair_cli helper at
# a random moment, then requires:
#   1. the VM still resolves to exactly one current disk+vars pair;
#   2. that pair is byte-exact: unchanged after an interrupted create or
#      export, and the old pair or the newest snapshot after a restore;
#   3. an interrupted export left its destination absent, as the previous
#      export, or complete and equal to the current pair;
#   4. the same operation then succeeds without repair, byte-exact (snapshot
#      == current after create; current == newest snapshot after restore;
#      export == current), and leaves no `.NAME.staging` and no snapshot
#      directory without a manifest behind;
#   5. after a create or restore, Windows boots from the resulting pair and
#      reads back the marker written into exactly that pair, then writes a new
#      one.
# Hashes are sha256 of the files the VM boots from, taken by this script.
#
#   APP=/abs/BridgeVM.app TARGET=disk.raw VARS=vars.fd OUT=new-dir TRIALS=9 \
#     scripts/verify-snapshot-interruption.sh
set -euo pipefail
fail() { echo "FAIL: $*" >&2; exit 1; }

REPO=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
APP=${APP:?packaged BridgeVM.app}
TARGET=${TARGET:?installed Windows disk with the BridgeVM agent}
VARS=${VARS:?its UEFI vars}
OUT=${OUT:?new output directory}
TRIALS=${TRIALS:-9}
BOOT_TIMEOUT=${BOOT_TIMEOUT:-900}
STEP_TIMEOUT=${STEP_TIMEOUT:-240}
ID=${ID:-a19-interrupt}
CLI=$APP/Contents/Resources/target/release/bridgevm
export BRIDGEVM_PREBUILT_PROBE=$APP/Contents/Resources/target/release/examples/hvf_gic_boot_probe
GUEST_MARKER='C:\bv-interrupt-marker.txt'
cd "$REPO"
source scripts/live-gates/live-process-cleanup.sh

mkdir "$OUT"
OUT=$(cd "$OUT" && pwd)
LIB=$OUT/library
B=$LIB/$ID/bundle.vmbridge
mkdir -p "$B/disks" "$B/metadata" "$B/logs/hvf"
cp -c "$TARGET" "$B/disks/hvf-target.raw"
cp -c "$VARS" "$B/metadata/hvf-vars.fd"
chmod 600 "$B/disks/hvf-target.raw" "$B/metadata/hvf-vars.fd"
python3 - "$LIB/$ID/vm.json" "$B" "$ID" <<'PY'
import json, pathlib, sys
path, bundle, vm_id = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), sys.argv[3]
value = {
    "id": vm_id, "name": vm_id, "displayName": "A19 interruption campaign (3D off)",
    "backendKind": "hvf-engine", "bootMode": "windows-hvf",
    "bundlePath": str(bundle), "runnerPath": "", "launchSpecPath": "",
    "handoffPath": "", "sshKeyPath": "", "sshUser": "", "leasesPath": "",
    "guestName": vm_id, "displayWidth": 1280, "displayHeight": 800,
    "installPending": False, "diskPath": str(bundle / "disks/hvf-target.raw"),
    "memMiB": 6144, "cpuCount": 4, "networkEnabled": False,
    "experimental3DAllowed": False,
}
with path.open("x", encoding="utf-8") as output:
    json.dump(value, output, sort_keys=True, separators=(",", ":")); output.write("\n")
PY

set -m
LAUNCHER=""
cleanup() {
  local status=$?
  if [[ -n "$LAUNCHER" ]]; then bridgevm_terminate_process_group_bounded "$LAUNCHER" || true; fi
  # The library and the exports hold guest disk copies.
  [[ "${KEEP_LIBRARY:-0}" == 1 ]] || rm -rf "$LIB" "$OUT/exports"
  return "$status"
}
trap cleanup EXIT

# The pair the app boots: the managed generation once one exists, else the imported files.
current_pair() {
  local n=0 d found=""
  for d in "$B"/disks/.bridgevm-pair-v2-*/current; do [[ -d "$d" ]] && { n=$((n + 1)); found=$d; }; done
  case $n in
    0) echo "$B/disks/hvf-target.raw $B/metadata/hvf-vars.fd" ;;
    1) [[ -f "$found/disk.raw" && -f "$found/vars.fd" ]] || { echo "BROKEN $found"; return 0; }
       echo "$found/disk.raw $found/vars.fd" ;;
    *) echo "BROKEN $n pair generations" ;;
  esac
}
pair_hash() { # disk vars
  local disk vars
  disk=$(openssl dgst -sha256 -r "$1") || return
  vars=$(openssl dgst -sha256 -r "$2") || return
  printf '%s:%s\n' "${disk%% *}" "${vars%% *}"
}
current_hash() {
  local p; p=$(current_pair)
  [[ "$p" != BROKEN* ]] || fail "trial $T: current pair does not resolve: $p"
  # shellcheck disable=SC2086
  pair_hash $p
}
newest_snapshot() { find "$B/metadata/snapshots" -maxdepth 1 -name '*.snapshot' | sort | tail -1; }
EXPORT=$OUT/exports/vm.snapshot
export_hash() { # absent (or an empty directory), or the hash of a complete export
  if [[ ! -e "$EXPORT" ]] || [[ -d "$EXPORT" && -z "$(find "$EXPORT" -mindepth 1 | head -1)" ]]; then echo absent
  elif [[ -f "$EXPORT/manifest.json" ]]; then pair_hash "$EXPORT/disk.raw" "$EXPORT/vars.fd"
  else echo "INCOMPLETE $(find "$EXPORT" -mindepth 1 | head -3 | tr '\n' ' ')"
  fi
}
# What an interrupted operation may leave is reclaimed by the next one.
no_debris() { # label
  local left
  left=$(find "$B/metadata/snapshots" "$OUT/exports" -maxdepth 1 -name '.*.staging' 2>/dev/null)
  [[ -z "$left" ]] || fail "$1: staging left behind: $left"
  # An empty reservation is harmless; a partial one would hold copies.
  local d
  for d in "$B"/metadata/snapshots/*.snapshot; do
    [[ ! -d "$d" || -f "$d/manifest.json" || -z "$(find "$d" -mindepth 1 | head -1)" ]] \
      || fail "$1: snapshot directory with content but no manifest: $d"
  done
}
inventory() { # label
  {
    echo "== $1 $(date -u +%H:%M:%SZ)"
    (cd "$B" && find . -path ./logs -prune -o -print | sort)
    du -sk "$B" | awk '{print "allocated_kib " $1}'
  } >> "$OUT/inventory.txt"
}

# pair hash -> guest marker it holds (bash 3.2: no associative arrays)
set_marker() { printf '%s\t%s\n' "$1" "$2" >> "$OUT/markers.tsv"; }
get_marker() { awk -F'\t' -v h="$1" '$1==h{m=$2} END{print (m=="" ? "unknown" : m)}' "$OUT/markers.tsv"; }
ask() { # ctl log command out
  python3 scripts/snapshot-restore-channel.py "$1" "$2" "$3" "$STEP_TIMEOUT" "$4"
}
boot_mark() { # phase-name new-marker -> sets READ_MARKER
  local pdir=$OUT/boots/$1 p
  mkdir -p "$pdir"
  p=$(current_pair); [[ "$p" != BROKEN* ]] || fail "$1: current pair does not resolve: $p"
  local disk=${p% *} vars=${p#* }
  local ctl=$pdir/agent.ctl log=$pdir/run.log; : > "$ctl"
  scripts/run-hvf-windows-installed-boot.sh --release --skip-build \
    --target "$disk" --vars "$vars" --evidence-dir "$pdir" \
    --watchdog-ms $((BOOT_TIMEOUT * 1000)) --ram-mib 6144 --smp-cpus 4 \
    --agent-service-control "$ctl" > "$pdir/launcher.out" 2>&1 &
  LAUNCHER=$!
  local deadline=$((SECONDS + BOOT_TIMEOUT))
  until grep -aq '^BVAGENT SERVICE start' "$log" 2>/dev/null; do
    if ((SECONDS >= deadline)) || ! kill -0 "$LAUNCHER" 2>/dev/null; then fail "$1: guest never reached agent service state"; fi
    sleep 1
  done
  ask "$ctl" "$log" "powershell -NoProfile -Command \"\$ErrorActionPreference='Stop'; if (Test-Path '$GUEST_MARKER') { Get-Content '$GUEST_MARKER' } else { 'BV-NO-MARKER' }\"" "$pdir/marker-before.txt" \
    || fail "$1: marker read"
  READ_MARKER=$(tr -d '\r\n' < "$pdir/marker-before.txt")
  ask "$ctl" "$log" "powershell -NoProfile -Command \"\$ErrorActionPreference='Stop'; Set-Content -NoNewline -Encoding ascii -Path '$GUEST_MARKER' -Value '$2'; if ((Get-Content -Raw '$GUEST_MARKER') -cne '$2') { exit 1 }; Write-Output '$2'\"" "$pdir/marker-after.txt" \
    || fail "$1: marker write"
  printf 'shutdown.exe /s /t 0\n' >> "$ctl"
  local rc=0; wait "$LAUNCHER" || rc=$?
  LAUNCHER=""
  (( rc == 0 )) || fail "$1: helper exit $rc after guest shutdown"
  python3 scripts/live-gates/hvf_terminal_evidence.py --require-system-off "$log" >/dev/null || fail "$1: no SYSTEM_OFF"
  python3 scripts/live-gates/hvf_terminal_evidence.py --require-nvme-write-back "$log" >/dev/null || fail "$1: no NVMe write-back"
  rm -rf "$pdir/ramfb"
  CUR=$(current_hash)
  set_marker "$CUR" "$2"
}

descendants() { local p; for p in $(pgrep -P "$1"); do echo "$p"; descendants "$p"; done; }

run_op() { # op kill-after-seconds(or "") name -> sets OP_RC OP_SECS OP_KILLED OP_HELPER_SIGNALED
  local op=$1 after=$2 name=$3 start=$SECONDS
  local d=$OUT/ops/$name; mkdir -p "$d"
  local args=(app "snapshot-$op" "$ID")
  [[ $op != export ]] || args+=("$EXPORT")
  "$CLI" "${args[@]}" --library "$LIB" --json > "$d/out.json" 2> "$d/err.txt" &
  local pid=$!
  OP_KILLED=0
  OP_HELPER_SIGNALED=0
  if [[ -n "$after" ]]; then
    sleep "$after"
    if kill -0 "$pid" 2>/dev/null; then
      # The app CLI runs snapshot_pair_cli in its own process group (Foundation
      # Process), so kill the CLI and every descendant at once: a crash of both.
      local victims victim
      victims="$pid $(descendants "$pid" | tr '\n' ' ')"
      # shellcheck disable=SC2086
      ps -o pid=,pgid=,command= -p "$(echo $victims | tr ' ' ',')" > "$d/killed.txt" 2>/dev/null || true
      for victim in $victims; do
        if kill -KILL "$victim" 2>/dev/null; then
          if [[ "$victim" == "$pid" ]]; then
            OP_KILLED=1
          elif grep -Eq "^[[:space:]]*${victim}[[:space:]].*snapshot_pair_cli" "$d/killed.txt"; then
            OP_HELPER_SIGNALED=1
          fi
        fi
      done
    fi
  fi
  OP_RC=0; wait "$pid" || OP_RC=$?
  (( OP_RC == 128 + 9 )) || OP_KILLED=0
  OP_SECS=$((SECONDS - start))
  sleep 0.5
  # A helper spawned in the instant of the kill would survive it; kill it too.
  if pgrep -f "$LIB" >/dev/null 2>&1; then
    pgrep -fl "$LIB" > "$d/late-survivors.txt" 2>/dev/null || true
    pkill -KILL -f "$LIB" 2>/dev/null || true
    sleep 0.5
    ! pgrep -f "$LIB" >/dev/null 2>&1 || fail "$name: processes survived the kill"
  fi
}

T=setup
echo "setup $(date -u +%H:%M:%SZ)"
boot_mark setup-0 "BV-M0-$(date +%s)"
run_op create "" setup-create
(( OP_RC == 0 )) || fail "setup create rc=$OP_RC: $(head -c 300 "$OUT/ops/setup-create/err.txt")"
T_CREATE=$OP_SECS
SNAP=$(newest_snapshot)
SNAP_HASH=$(pair_hash "$SNAP/disk.raw" "$SNAP/vars.fd")
[[ "$SNAP_HASH" == "$CUR" ]] || fail "setup: snapshot $SNAP_HASH != current $CUR"
boot_mark setup-1 "BV-M1-$(date +%s)"
T_RESTORE=$((T_CREATE * 2))
T_EXPORT=$T_CREATE
mkdir -p "$OUT/exports"
EXPORT_HASH=absent
inventory setup
echo "setup done: create ${T_CREATE}s current=$CUR snapshot=$SNAP_HASH $(date -u +%H:%M:%SZ)"

interrupted=0; finished=0; helper_signals=0
for ((T = 1; T <= TRIALS; T++)); do
  case $((T % 3)) in
    1) op="create"; est=$T_CREATE ;;
    2) op="restore"; est=$T_RESTORE ;;
    0) op="export"; est=$T_EXPORT ;;
  esac
  # Uniform over the operation's last measured duration, in tenths of a second.
  after=$(python3 -c "import random; print(round(random.uniform(0.2, $est * 1.05), 1))")
  before=$CUR
  run_op "$op" "$after" "t$T-$op-killed"
  if (( OP_KILLED )); then
    interrupted=$((interrupted + 1))
    helper_signals=$((helper_signals + OP_HELPER_SIGNALED))
    state="killed helper-signaled=$OP_HELPER_SIGNALED"
  else
    (( OP_RC == 0 )) || fail "trial $T: $op exited before the planned interruption (rc=$OP_RC)"
    finished=$((finished + 1))
    state="finished-before-kill"
  fi
  CUR=$(current_hash)
  case $op in
    create|export) [[ "$CUR" == "$before" ]] || fail "trial $T: interrupted $op changed the current pair ($before -> $CUR)" ;;
    restore) [[ "$CUR" == "$before" || "$CUR" == "$SNAP_HASH" ]] || fail "trial $T: interrupted restore left a pair that is neither old nor snapshot ($CUR)" ;;
  esac
  outcome=$([[ "$CUR" == "$before" ]] && echo old || echo snapshot)
  if [[ $op == export ]]; then
    seen=$(export_hash)
    [[ "$seen" == "$EXPORT_HASH" || "$seen" == "$CUR" ]] \
      || fail "trial $T: interrupted export left '$seen' (previous export $EXPORT_HASH, current $CUR)"
    outcome="destination $([[ "$seen" == "$EXPORT_HASH" ]] && echo unchanged || echo complete)"
  fi
  inventory "t$T after $op kill@${after}s"

  run_op "$op" "" "t$T-$op-retry"
  (( OP_RC == 0 )) || fail "trial $T: $op after the interruption failed rc=$OP_RC: $(head -c 400 "$OUT/ops/t$T-$op-retry/err.txt")"
  no_debris "trial $T: after the $op retry"
  if [[ $op == export ]]; then
    T_EXPORT=$OP_SECS
    EXPORT_HASH=$(export_hash)
    [[ "$EXPORT_HASH" == "$CUR" ]] || fail "trial $T: export $EXPORT_HASH != current $CUR"
    [[ "$(current_hash)" == "$CUR" ]] || fail "trial $T: export changed the current pair"
    inventory "t$T end"
    echo "trial $T: $op kill@${after}s/${est}s $state -> $outcome; retry ok (${OP_SECS}s); export exact $(date -u +%H:%M:%SZ)"
    continue
  elif [[ $op == create ]]; then
    T_CREATE=$OP_SECS
    SNAP=$(newest_snapshot)
    SNAP_HASH=$(pair_hash "$SNAP/disk.raw" "$SNAP/vars.fd")
    [[ "$SNAP_HASH" == "$CUR" ]] || fail "trial $T: new snapshot $SNAP_HASH != current $CUR"
  else
    T_RESTORE=$OP_SECS
    python3 - "$OUT/ops/t$T-$op-retry/out.json" "$SNAP" <<'PY' || fail "trial $T: restore did not report the newest snapshot"
import json, os, sys
value = json.load(open(sys.argv[1]))
assert value.get("complete") is True
assert os.path.realpath(value["snapshotPath"]) == os.path.realpath(sys.argv[2])
PY
    CUR=$(current_hash)
    [[ "$CUR" == "$SNAP_HASH" ]] || fail "trial $T: restore result $CUR != snapshot $SNAP_HASH"
  fi
  expected=$(get_marker "$CUR")
  boot_mark "t$T" "BV-T$T-$(date +%s)"
  [[ "$READ_MARKER" == "$expected" ]] || fail "trial $T: guest read '$READ_MARKER', the pair it booted holds '$expected'"
  inventory "t$T end"
  echo "trial $T: $op kill@${after}s/${est}s $state -> $outcome; retry ok (${OP_SECS}s); boot read $READ_MARKER $(date -u +%H:%M:%SZ)"
done

echo "PASS: $TRIALS trials ($interrupted CLI SIGKILL exits, $helper_signals helper signals sent, $finished finished before the kill): the current pair and the export were always exact, every retry succeeded and left no staging, every boot read its own marker"
du -sk "$B" | awk '{print "final allocated_kib " $1}'
find "$B/metadata/snapshots" -maxdepth 1 -name '*.snapshot' | wc -l | awk '{print "snapshots kept " $1}'
