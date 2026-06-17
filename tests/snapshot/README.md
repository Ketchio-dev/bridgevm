# Snapshot Tests

Disk, suspend, and application-consistent snapshot tests live here.

Current scaffold coverage:

```bash
bridgevm snapshot create dev before-upgrade
bridgevm snapshot create dev before-upgrade-disk --kind disk
bridgevm snapshot create dev paused --kind suspend
bridgevm snapshot disk-create dev before-upgrade-disk
bridgevm snapshot chain dev
bridgevm snapshot list dev
bridgevm snapshot restore dev before-upgrade
```

The current restore path records metadata, restores the VM runtime state stored with the snapshot, and rewinds disk snapshots to their backing chain member through `metadata/active-disk.json`. Disk snapshot creation also records a qcow2 chain scaffold under `metadata/snapshot-disks/<snapshot>.json`, including the backing disk, backing format, planned overlay path, overlay existence state, and the `qemu-img create -f qcow2 -F <backing-format> -b <backing-file> <overlay>` command that `snapshot disk-create <vm> <snapshot>` can execute. `snapshot chain <vm>` reports the disk snapshot chain metadata plus the active disk source, snapshot name when present, and selected disk path. Application-consistent snapshot creation records preflight metadata under `metadata/application-consistent-snapshots/<snapshot>.json`, including guest-tools connection state, required `fs-freeze`/`fs-thaw` capabilities, missing capabilities, readiness, and planned freeze/thaw semantics; the daemon-owned execute path then performs request-correlated freeze/thaw around snapshot creation when a connected backend advertises those capabilities.

Suspend snapshot creation records planned image metadata under `metadata/suspend-images/<snapshot>.json`, with image path `suspend-images/<snapshot>.bin`, image format marker, existence state, and preparation timestamp. This is a metadata contract only; the scaffold does not serialize guest memory yet. Restoring a suspend snapshot requires that planned image file to exist, updates the recorded existence state, writes `suspend_image` into `metadata/last-restore.json`, and prints suspend image status in the CLI restore output.

Disk snapshot tests should assert metadata and command construction, cover `snapshot chain` output, then cover the explicit `snapshot disk-create` execution path. That path should create the overlay when the backing disk exists and `qemu-img create` succeeds, record the attempt under `metadata/snapshot-disks/<snapshot>-create.json` or similar metadata, update the active disk to the overlay, and fail safely when `qemu-img` is missing, `qemu-img create` exits unsuccessfully, or the backing disk is missing. Tests should also cover `qemu-args` and runner startup using the active chain member, plus restore rewinding the active disk to a snapshot backing image. Suspend snapshot tests should cover planned image metadata creation, restore failure when the planned image is absent, restore metadata when the image exists, and CLI restore output that includes the suspend image status. Application-consistent snapshot tests should cover preflight readiness metadata, daemon-owned freeze/thaw execution, always-thaw behavior, fake-backend fsfreeze ordering, and the opt-in live real-fsfreeze path. Future tests should add real suspend image serialization/restoration and higher-level application quiescing coverage.
