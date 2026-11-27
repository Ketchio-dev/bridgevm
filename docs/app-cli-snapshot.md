# Create, restore and export a powered-off snapshot from the CLI

```sh
bridgevm app snapshot-create dev-vm
bridgevm app snapshot-restore dev-vm
bridgevm app snapshot-export dev-vm /Volumes/Backup/dev-vm.snapshot --json
```

Use the exact ID from `bridgevm app list`. These commands operate only on a
saved, installed own-HVF Windows VM. Installation-pending and unsupported VM
entries are refused. The snapshot contains the VM's current NVMe disk and
matching UEFI variable store as one managed pair. Create and export both resolve
the selected managed generation, so an export after restore cannot silently copy
stale logical originals. Export atomically publishes `disk.raw`, `vars.fd`, and
`manifest.json`, whose hashes are computed from the copied files.
The destination must be absent, an
empty directory or a prior snapshot of exactly those files; anything else, even
Finder metadata, is refused and left intact. Restore uses the newest snapshot
(the app's snapshot list can restore an older one) and verifies it before
selecting the pair. The media lease refuses an active runtime rather than waiting.

The export destination is the only media path accepted on the command line. It
must be absolute and outside the managed VM bundle. Disk and vars paths come
from the exact saved app-library entry; secrets remain off argv. The bundled
helper must be a canonical regular executable in the BridgeVM installation.
If an export reports a staging directory it cannot identify, choose a fresh
output path or move that directory aside before retrying.

On an exFAT drive, give each export a new folder name. macOS can add hidden
metadata files alongside the disk and UEFI variables. BridgeVM preserves the
earlier export when the folder contains these files.

Text output describes the host operation and its limitation. JSON uses
`schema: "bridgevm.app-snapshot.v1"` with the operation, exact VM ID,
library path, snapshot path, completion state and refusal reason when present.

Ctrl-C (SIGINT) or SIGTERM stops the running helper and waits for it: the
VM's disk and vars, a new snapshot and an export destination are each left as
they were or complete (a restore leaves the old pair or the snapshot), never a
mix. The command then reports `cancelled` (JSON `cancelSignal`) and exits 130
or 143. A cancelled create or export also removes its partial copy, the hidden
`.NAME.staging` folder next to the destination.

A restore gives back the disk space of the disk it replaces: the VM's original
disk file keeps its size but no longer holds data, because BridgeVM always uses
the restored copy from then on.

Exit codes are 0 for a completed host operation, 1 for refusal or failure,
130 or 143 when cancelled, and 2 for invalid usage. Success does not prove
Windows boots after restore, a guest marker survived, or A19 is complete. Those claims require the live sample and
interruption evidence in the [V1 snapshot scope](windows-arm/snapshot-scope-v1.md).

## Recover a manually moved VM with older snapshot storage

Older snapshot storage binds the managed disk and UEFI variables to their
original absolute paths. If an externally moved VM reports
`unbound managed media exists`, shut it down and return the entire VM bundle
to its exact original location. Use the saved registration for that location.
Run `bridgevm app snapshot-export ID OUTPUT` with a fresh absolute destination
outside the bundle. A successful export confirms that BridgeVM matched the selected
pair and upgraded its storage identity. You can then move the VM through the
app.

If the original location or registration is unavailable, preserve the
moved bundle and recover the complete VM and its registration from a backup.
