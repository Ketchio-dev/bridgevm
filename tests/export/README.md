# Export And Import Tests

VM export/import tests live here.

Current scaffold coverage:

```bash
bridgevm export dev --output dev.vmbridge
bridgevm import dev.vmbridge --name dev-copy
bridgevm export dev --output dev.tar
bridgevm import dev.tar --name dev-tar-copy
```

The current export path writes either a `.vmbridge` directory bundle or a `.tar` archive, selected from the output path, and records `metadata/export.json`. The current import path copies the matching directory or tar bundle back into the VM store, optionally rewrites the VM identity, and writes `metadata/import.json`.

Integration coverage checks both directory and tar export/import metadata parity through local and socket-backed paths, including preserved manifest/snapshot/export/import metadata, port forwards, and shared-folder policy tokens. Tar coverage also rejects absolute or traversal archive member paths and verifies live socket/lock metadata is excluded from exported directories, exported tar archives, and imported bundles.

Integration coverage now includes a conservative current-schema manifest migration boundary before export/import compatibility checks. Future tests should validate signatures, sparse disk handling, interrupted export/import recovery, and concrete older-schema to newer-schema upgrades once a second manifest schema exists.
