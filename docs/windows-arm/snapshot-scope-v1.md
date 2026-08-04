# BridgeVM V1 snapshot scope

Status: **owner-approved for V1** (2026-07-30).

## Decision

BridgeVM V1 ships **cold snapshots only**:

- the VM must be powered off before create or restore;
- a snapshot contains the Windows NVMe disk image and its matching UEFI variable
  store;
- restore replaces both files as one versioned pair;
- creating/restoring a snapshot while the VM is running is rejected;
- live RAM/vCPU/device resume is deferred to V2.

This is the owner decision for V1 criterion B5. The product promise must call
this “Snapshot” or “Powered-off snapshot”, not “Save state” or “Suspend”, which
would imply live execution state.

## Why this boundary

The repository has two different mechanisms that must not be conflated.

1. `WritableMedia` supports durable media copies through `snapshot_path` and
   labels them as snapshots (`crates/bridgevm-hvf/src/media.rs`). This is the
   correct primitive for a powered-off disk/vars pair.
2. `VirtPlatform::snapshot_state` serializes substantial device state and has a
   matching restore method (`crates/bridgevm-hvf/src/platform_virt/snapshot.rs`),
   but it is used only by probe checkpoint glue. BridgeVMControl has no product
   path that atomically captures guest RAM, all vCPU architectural state,
   CoreAudio/renderer external state, disk write ordering, and that device blob.

A live snapshot built from only the second mechanism would be unsafe: restored
Windows could observe device/RAM/disk generations that never coexisted. Calling
that a V1 snapshot would overstate what is implemented.

## V1 acceptance boundary

A future V1 implementation is complete only when it proves all of:

1. snapshot creation refuses a running VM;
2. disk and vars are copied to a temporary snapshot directory and atomically
   renamed into place together;
3. a manifest records format version, source VM identifier, byte sizes, and
   SHA-256 for both files;
4. restore verifies both hashes before replacing either live file;
5. a restored powered-off snapshot boots Windows and preserves a guest-created
   marker file;
6. interrupted create/restore leaves either the old complete pair or the new
   complete pair, never one file from each.

## Implementation status (2026-08-04)

Items 1, 2, 3, 4 and 6 are implemented in
`crates/bridgevm-hvf/src/snapshot_pair.rs` and covered by 35 tests, including
the ones that would otherwise be assumed: a refused snapshot leaves no
directory, a tampered snapshot does not restore and does not touch the live
pair, and an interrupted create leaves the previous snapshot still verifying.

Two adjacent defects were fixed at the same time, because they made an atomic
pair impossible regardless of what this module did. `WritableMedia::persist`
used a truncating `fs::write` for UEFI variables, and
`DiskBackend::export_to_path` called `flush()` with no `fsync`, so its bytes
were not on disk when it returned. Both now write to a temp file, fsync,
rename, and fsync the parent.

The byte quota named in the criterion is implemented as a ceiling on the
copy-on-write overlay (`crates/bridgevm-hvf/src/nvme/disk_export.rs`), which
was previously unbounded: a guest writing across a 64 GiB read-only image
could consume 64 GiB of host RAM.

**Item 5 is not proven.** No powered-off snapshot has been taken of a real
Windows guest, restored, and booted to check a guest-created marker survives.
A19 therefore stays open. The mechanism is tested; the end-to-end claim is not
yet earned.

## Deferred to V2

Live save/resume requires a quiesce protocol and atomic capture of RAM, every
vCPU, all device state, renderer/3D external resources, audio, timers, and
storage. That work remains valuable, but it is explicitly outside V1 rather
than an implicit partial feature.
