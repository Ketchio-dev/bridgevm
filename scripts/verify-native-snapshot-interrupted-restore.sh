#!/usr/bin/env bash
# A19 T22: one controlled helper death before restore publication on real media.
#
# The private guest is powered off before the helper is interrupted. A fresh
# product CLI selection must still boot the clobbered pair; a normal retry must
# then boot the original snapshot pair. This does not simulate host power loss.
#
# The second marker is what makes this a real test. Without it a restore that
# did nothing at all would pass, because the original marker would still be
# sitting there.
#
# The marker lives on C: rather than in the shared folder on purpose: the share
# is host-side storage and is not part of the snapshot, so a marker there would
# survive a restore that did nothing.
set -euo pipefail
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
BOOT_TIMEOUT=${BOOT_TIMEOUT:-1500}
STEP_TIMEOUT=${STEP_TIMEOUT:-240}
NATIVE_SNAPSHOT_CLI=${NATIVE_SNAPSHOT_CLI:-}
NATIVE_SNAPSHOT_VM_ID=${NATIVE_SNAPSHOT_VM_ID:-a19-native-cli-live}
A19_SNAPSHOT_HELPER=${A19_SNAPSHOT_HELPER:-}
mkdir -p "$OUT"
fail() { echo "FAIL: $*" >&2; exit 1; }
[[ "$NATIVE_SNAPSHOT_CLI" == /* && -x "$NATIVE_SNAPSHOT_CLI" && ! -L "$NATIVE_SNAPSHOT_CLI" ]] \
  || fail "NATIVE_SNAPSHOT_CLI must be an absolute non-symlink executable"
[[ "$NATIVE_SNAPSHOT_VM_ID" =~ ^[a-z0-9]+(-[a-z0-9]+)*$ ]] \
  || fail "NATIVE_SNAPSHOT_VM_ID is not canonical"
[[ "$A19_SNAPSHOT_HELPER" == /* && -x "$A19_SNAPSHOT_HELPER" && ! -L "$A19_SNAPSHOT_HELPER" ]] \
  || fail "A19_SNAPSHOT_HELPER must be an absolute non-symlink executable"
WORK=$OUT/live
mkdir "$WORK" || fail "work directory must be new"
source "$REPO/scripts/snapshot-restore-lifecycle.sh"
source "$REPO/scripts/native-snapshot-export-live.sh"
INTERRUPT_LAUNCHER=""
t22_cleanup() {
  local status=$?
  if [[ -n "$INTERRUPT_LAUNCHER" ]]; then
    bridgevm_terminate_process_group_bounded "$INTERRUPT_LAUNCHER" || return 1
    wait "$INTERRUPT_LAUNCHER" 2>/dev/null || true
    INTERRUPT_LAUNCHER=""
  fi
  snapshot_cleanup || return 1
  return "$status"
}
trap t22_cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
LIBRARY=$WORK/library
BUNDLE=$LIBRARY/$NATIVE_SNAPSHOT_VM_ID/bundle.vmbridge
mkdir -p "$BUNDLE/disks" "$BUNDLE/metadata" "$BUNDLE/logs/hvf" || fail "create isolated native library"
cp -c "$DISK" "$BUNDLE/disks/hvf-target.raw" || fail "clone disk"
cp -c "$VARS" "$BUNDLE/metadata/hvf-vars.fd" || fail "clone vars"
python3 - "$LIBRARY/$NATIVE_SNAPSHOT_VM_ID/vm.json" "$BUNDLE" "$NATIVE_SNAPSHOT_VM_ID" <<'PY'
import json, pathlib, sys
path, bundle, vm_id = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), sys.argv[3]
value = {
    "id": vm_id, "name": vm_id, "displayName": "A19 native CLI live (3D off)",
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
WORK_DISK=$BUNDLE/disks/hvf-target.raw
WORK_VARS=$BUNDLE/metadata/hvf-vars.fd
chmod u+w "$WORK_DISK" "$WORK_VARS" || fail "make private clones writable"
source "$REPO/scripts/a19-t22-marker-share.sh"
echo "=== phase 1: write the marker that must survive ==="
ORIGINAL=$(t22_random_marker ORIGINAL) || fail "make original marker"
boot_and_mark "$ORIGINAL" phase1-original \
  || fail "guest never reached agent service state in phase 1"
echo "original marker: $ORIGINAL"

echo "=== phase 2: snapshot the powered-off pair ==="
"$NATIVE_SNAPSHOT_CLI" app snapshot-create "$NATIVE_SNAPSHOT_VM_ID" \
  --library "$LIBRARY" --json > "$OUT/create.json" 2> "$OUT/create.stderr" \
  || fail "native snapshot create failed; see $OUT/create.stderr"
SNAP=$BUNDLE/metadata/snapshots/latest.snapshot
python3 - "$OUT/create.json" "$SNAP" "$NATIVE_SNAPSHOT_VM_ID" <<'PY'
import json, pathlib, sys
value=json.load(open(sys.argv[1], encoding="utf-8"))
assert value == {"schema":"bridgevm.app-snapshot.v1","command":"create","vmID":sys.argv[3],
                 "libraryPath":str(pathlib.Path(sys.argv[2]).parents[4]),
                 "snapshotPath":sys.argv[2],"complete":True}
PY
cp "$SNAP/manifest.json" "$OUT/snapshot-created-manifest.json" \
  || fail "retain authenticated snapshot manifest"

echo "=== phase 3: overwrite it, so a no-op restore cannot pass ==="
CLOBBER=$(t22_random_marker CLOBBERED) || fail "make clobber marker"
boot_and_mark "$CLOBBER" phase3-clobber \
  || fail "guest never reached agent service state in phase 3"
[[ $(cat "$OUT/phase3-clobber/marker-before.txt") == "$ORIGINAL" ]] \
  || fail "phase 3 could not read back the marker phase 1 wrote (got: $(head -c 120 "$OUT/phase3-clobber/marker-before.txt" 2>/dev/null)); the marker does not persist across a power cycle, so this gate cannot measure a restore"
echo "clobber marker: $CLOBBER"

echo "=== phase 4: stop and kill helper during staged restore verification ==="
LOGICAL_DISK=$WORK_DISK
LOGICAL_VARS=$WORK_VARS
openssl dgst -sha256 -r "$LOGICAL_DISK" | awk '{print $1}' > "$OUT/pre-interrupt-disk.sha256"
openssl dgst -sha256 -r "$LOGICAL_VARS" | awk '{print $1}' > "$OUT/pre-interrupt-vars.sha256"
python3 "$REPO/scripts/live-gates/a19_interrupt_restore_child.py" \
  "$A19_SNAPSHOT_HELPER" "$SNAP" "$LOGICAL_DISK" "$LOGICAL_VARS" "$OUT" \
  &
INTERRUPT_LAUNCHER=$!
wait "$INTERRUPT_LAUNCHER" || fail "exact helper interruption point was not proven"
INTERRUPT_LAUNCHER=""

echo "=== phase 4b: fresh app selection after helper death ==="
mkdir "$WORK/postkill-export" "$OUT/postkill-export" || fail "postkill export directories"
native_snapshot_export_and_select "$NATIVE_SNAPSHOT_CLI" "$NATIVE_SNAPSHOT_VM_ID" \
  "$LIBRARY" "$WORK/postkill-export" "$OUT/postkill-export" \
  || fail "postkill selected-pair export failed"
openssl dgst -sha256 -r "$WORK_DISK" | awk '{print $1}' > "$OUT/postkill-disk.sha256"
openssl dgst -sha256 -r "$WORK_VARS" | awk '{print $1}' > "$OUT/postkill-vars.sha256"
cmp "$OUT/pre-interrupt-disk.sha256" "$OUT/postkill-disk.sha256" \
  || fail "postkill selected disk differs from the old pair"
cmp "$OUT/pre-interrupt-vars.sha256" "$OUT/postkill-vars.sha256" \
  || fail "postkill selected vars differ from the old pair"

echo "=== phase 5: boot the still-selected clobbered pair ==="
POSTKILL=$(t22_random_marker POSTKILL) || fail "make postkill marker"
boot_and_mark "$POSTKILL" phase5-postkill \
  || fail "postkill selected pair did not boot"
[[ $(cat "$OUT/phase5-postkill/marker-before.txt") == "$CLOBBER" ]] \
  || fail "postkill boot did not preserve the exact clobber marker"
echo "=== phase 6: normal product CLI restore retry ==="
"$NATIVE_SNAPSHOT_CLI" app snapshot-restore "$NATIVE_SNAPSHOT_VM_ID" \
  --library "$LIBRARY" --json > "$OUT/restore-retry.json" 2> "$OUT/restore-retry.stderr" \
  || fail "native snapshot restore retry failed"
python3 - "$OUT/restore-retry.json" "$SNAP" "$NATIVE_SNAPSHOT_VM_ID" <<'PY'
import json, pathlib, sys
value=json.load(open(sys.argv[1], encoding="utf-8"))
assert value == {"schema":"bridgevm.app-snapshot.v1","command":"restore","vmID":sys.argv[3],
                 "libraryPath":str(pathlib.Path(sys.argv[2]).parents[4]),
                 "snapshotPath":sys.argv[2],"complete":True}
PY

echo "=== phase 6b: export the selected restored pair ==="
mkdir "$WORK/postretry-export" "$OUT/postretry-export" || fail "postretry export directories"
native_snapshot_export_and_select "$NATIVE_SNAPSHOT_CLI" "$NATIVE_SNAPSHOT_VM_ID" \
  "$LIBRARY" "$WORK/postretry-export" "$OUT/postretry-export" \
  || fail "postretry selected-generation export failed"

echo "=== phase 7: boot the restored original pair and read the marker ==="
FINAL=$(t22_random_marker FINAL) || fail "make final marker"
boot_and_mark "$FINAL" phase7-restored \
  || fail "restored pair never reached agent service state -- the snapshot does not boot"

READBACK=$OUT/phase7-restored/marker-before.txt
if grep -q "$CLOBBER" "$READBACK" 2>/dev/null; then
  fail "restored guest still has the clobbered marker: the restore did not take effect"
fi
[[ $(cat "$READBACK") == "$ORIGINAL" ]] \
  || fail "restored guest has neither marker; read back: $(head -c 200 "$READBACK" 2>/dev/null)"

openssl dgst -sha256 -r "$WORK_DISK" | awk '{print $1}' > "$OUT/final-disk.sha256"
openssl dgst -sha256 -r "$WORK_VARS" | awk '{print $1}' > "$OUT/final-vars.sha256"

echo
echo "PASS: A19 T22 interrupted restore diagnostic"
echo "  helper died before publication with all staged files synced,"
echo "  a fresh product selection booted the exact clobbered pair,"
echo "  and a normal retry restored the exact original marker."
