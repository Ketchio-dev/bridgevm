# Source only: clone a separate native library for the existing guest phases.
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
