# Create, restore and export a powered-off snapshot from the CLI

```sh
bridgevm app snapshot-create 개발-vm
bridgevm app snapshot-restore 개발-vm
bridgevm app snapshot-export 개발-vm /Volumes/Backup/개발-vm.snapshot --json
```

Use the exact ID from `bridgevm app list`. These commands operate only on a
saved, installed own-HVF Windows VM. Installation-pending and unsupported VM
entries are refused. The snapshot contains the VM's current NVMe disk and
matching UEFI variable store as one managed pair. Create and export both resolve
the selected managed generation, so an export after restore cannot silently copy
stale logical originals. Their byte quota also comes from the leased selected
pair; create rechecks that ceiling if selection changes before capture. Export
atomically publishes and verifies `disk.raw`, `vars.fd`, and `manifest.json`.
The destination must be absent, an
empty directory or a prior snapshot of exactly those files; anything else, even
Finder metadata, is refused and left intact. Restore verifies before selecting
the pair. The media lease refuses an active runtime rather than waiting.

The export destination is the only media path accepted on the command line. It
must be absolute and outside the managed VM bundle. Disk and vars paths come
from the exact saved app-library entry; secrets remain off argv. The bundled
helper must be a canonical regular executable in the BridgeVM installation.

Text output describes the host operation and its limitation. JSON uses
`schema: "bridgevm.app-snapshot.v1"` with the operation, exact VM ID,
library path, snapshot path, completion state and refusal reason when present.

Exit codes are 0 for a completed host operation, 1 for refusal or failure, and
2 for invalid usage. Success does not prove Windows boots after restore, a guest
marker survived, or A19 is complete. Those claims require the live sample and
interruption evidence in the [V1 snapshot scope](windows-arm/snapshot-scope-v1.md).
These commands use the native app library; older `bridgevm snapshot` commands
use the separate compatibility store in [CLI setup](app-cli.md).
