# A19: powered-off snapshot pair, measured on the real image (2026-08-04)

## What was wrong

The registry recorded A19 as "the atomic pair hash gate has not been re-run on
the release runtime". That was inaccurate in a way worth naming: the pair had
never existed. Three separate defects made it impossible.

1. `WritableMedia::persist` wrote UEFI variables with `fs::write`, which
   truncates the destination first. An interruption left a truncated vars file
   and a guest that no longer knew how to boot.
2. `DiskBackend::export_to_path` called `flush()` and returned. That pushes the
   process buffer to the kernel and nothing further, so the bytes were not on
   disk when the function claimed success.
3. The copy-on-write overlay on a read-only disk had no ceiling. A guest
   writing across a 64 GiB image could consume 64 GiB of host RAM, one chunk at
   a time, with nothing to stop it.

## What was built

`crates/bridgevm-hvf/src/snapshot_pair.rs` stages both files in a directory,
fsyncs them, writes a manifest with SHA-256 for each, and publishes the pair
with a single `rename`. Restore verifies both hashes before replacing either
live file. 43 tests cover it, including a refused snapshot leaving no
directory, a tampered snapshot not restoring and not touching the live pair,
and an interrupted create leaving the previous snapshot still verifiable.

The overlay ceiling is in `crates/bridgevm-hvf/src/nvme/disk_export.rs` and is
falsified by disabling it: three of six tests then fail.

## What the live run proved

Against `canonical-fresh-12041-agent-20260730.raw` (68,719,476,736 bytes):

```
before disk sha256: 6fef8f980adbc6af1ed41a1934d1ebafd93b8a1b4d2ab9a3f8bd07ae950d9b54
before vars sha256: d329b5b03cc5673435d3949f1b5b5b2bfe3f4dfe1f6f0b56ca5894f3bce87d3f
create  -> disk_sha256 6fef8f98...  vars_sha256 d329b5b0...
verify  -> disk_sha256 6fef8f98...  vars_sha256 d329b5b0...
```

Create and verify reproduce the source bytes exactly at full scale.

## Restore, proven on the second attempt

The first run could not reach restore. Staging its copy needed 66GiB against
59GiB free, and the gate skipped rather than reporting a pass it had not
earned. The fix was not more disk: `copy_and_sync` now clones with APFS
`clonefile` where the filesystem allows it. Measured on this image, cloning
takes 2ms and adds no used bytes, against minutes and a full second copy.

With that in place the whole chain ran against the real image:

```
before disk sha256: 6fef8f98...      vars: d329b5b0...
create    -> 6fef8f98...             d329b5b0...
verify    -> 6fef8f98...             d329b5b0...
clobbered -> fc4913c0...             (live disk deliberately destroyed)
free: 119GiB, restore needs about 66GiB to stage
restore   -> 6fef8f98...             d329b5b0...
after     -> 6fef8f98...             d329b5b0...
tampered snapshot refused: vars.fd does not match the manifest
live pair untouched by the refusal
```

A 64 GiB disk was destroyed and restored byte-for-byte, and a tampered
snapshot was refused without touching the live pair. Acceptance items 1, 2, 3,
4 and 6 are proven at full scale.

Note on why cloning is safe here and not merely fast: a clone stops sharing
storage the moment either side is written, so restoring a snapshot and then
booting the VM cannot write back into the snapshot. Two unit tests state that
directly, and `clonefile` refuses an existing destination rather than
overwriting one.

## What it still did not prove

Acceptance item 5 -- a restored snapshot boots Windows and preserves a
guest-created marker -- has not been attempted. Byte-exactness is necessary for
that and not sufficient: it says nothing about whether the firmware still finds
its boot entry, or whether the guest comes up. **A19 stays open.**

## Note on method

Two defects here were invisible to tests over synthetic files, because those
tests use kilobytes, where "needs a second copy" costs nothing. Both were found
by running the thing against real data.
