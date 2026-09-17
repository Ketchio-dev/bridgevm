# Create and restore a powered-off snapshot from the CLI

```sh
bridgevm app snapshot-create 개발-vm
bridgevm app snapshot-restore 개발-vm
bridgevm app snapshot-create 개발-vm --json
```

Use the exact ID from `bridgevm app list`. These commands operate only on a
saved, installed own-HVF Windows VM. Installation-pending and unsupported VM
entries are refused.

The snapshot contains the VM's current NVMe disk and matching UEFI variable
store as one managed pair. `snapshot-create` verifies the completed pair before
returning success. `snapshot-restore` verifies the saved manifest and hashes
before selecting the restored pair. The helper's media leases refuse an active
runtime rather than waiting for it.

Disk paths, UEFI variable paths, snapshot destinations, passwords and recovery
keys are not accepted on the command line. They are resolved from the exact
saved app-library entry. The bundled helper is required to be a canonical,
regular executable inside the selected BridgeVM installation.

Text output describes the host operation and names its limitation. JSON uses
`schema: "bridgevm.app-snapshot.v1"` and includes the operation, exact VM ID,
library path, snapshot path, completion state and refusal reason when present.

Exit codes are 0 for a completed host operation, 1 for refusal or failure, and
2 for invalid usage. Success does not prove that Windows boots after restore,
that a guest marker survived, or that A19 is complete. Those claims require the
live sample and interruption evidence defined by the
[V1 snapshot scope](windows-arm/snapshot-scope-v1.md).

This command controls the native app library. The older `bridgevm snapshot`
commands address the separate manifest-based compatibility store described in
[CLI setup](app-cli.md).
