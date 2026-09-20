# A19 managed-pair export — 2026-09-20

Classification: historical deterministic evidence. Current product state and
capability wording remain in the
[registry](../../../capabilities/windows-hvf.json). A19 stays OPEN.

## Product path

Source `697b4df50abaccf12d93578ea1d0a7bb40d66286` adds the native command
`bridgevm app snapshot-export ID OUTPUT`. It resolves an exact saved VM ID,
requires a completed own-HVF installation, and exports the currently selected
managed disk-plus-vars generation through the bundled snapshot-pair helper.
The command verifies the published snapshot before reporting success.

The output must be an absolute non-root path without parent traversal. Before
helper dispatch, the app resolves the nearest existing ancestor so symlinked
parents cannot route an apparently external output back inside the managed VM
bundle. The helper retains its explicit byte quota, media lease, atomic
directory publication, replacement and hash-verification boundaries. A held
runtime lease refuses export without publishing a destination.

Both the Rust front end and native app keep machine-readable output on the
`bridgevm.app-snapshot.v1` schema. Human output states that successful export
does not prove a Windows boot.

## Deterministic results

- five native parser, export, bundle-containment and parent-symlink contracts
  passed;
- fourteen managed-pair contracts passed, including export after restore from
  the selected managed generation while the logical original files were
  deliberately stale, and lease refusal without destination publication;
- twenty-one Rust app-CLI unit contracts and two executable forwarding
  contracts passed;
- structural budgets passed without raising an existing ceiling;
- the complete source-head project check passed every other executable, app,
  security, documentation and structural step and correctly failed only the
  stale capability identity that preceded this record. Its retained
  7,647-line log SHA-256 is
  `512661396063450d9d929b01e755d86c99602760c29854677ac9b32caa50597c`.

## Evidence limit

This closes the deterministic product-path gap where copying the logical
original files after restore could export stale state. It does not add a live
Windows export/restore/boot sample, simulate physical power loss, cover every
filesystem interruption point, or satisfy the required product-lifecycle
sample count. A19 therefore remains OPEN. Exact metadata-head local and hosted
verification remain required.
