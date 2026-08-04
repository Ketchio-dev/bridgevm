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

## What it did not prove, and why that matters

The first live attempt failed at restore with `No space left on device`, after
create and verify had both passed. That is not a flaw in staging a copy --
staging is what makes the swap atomic, since a `rename` cannot fail for lack of
space. It is a requirement nobody had written down: a restore needs the volume
to hold the live pair, the snapshot, and a full second copy at the same time.

Restore now checks free space first and cleans up its staging files on failure.
On the machine as it stands that check reports:

```
free: 59GiB, restore needs about 66GiB to stage
SKIP: not enough free space to stage a restore of this pair
```

So restore at full scale is **unproven**, and acceptance item 5 (a restored
snapshot boots Windows and preserves a guest-created marker) has not been
attempted at all. A19 stays open.

## Note on method

Two defects here were invisible to tests over synthetic files, because those
tests use kilobytes, where "needs a second copy" costs nothing. Both were found
by running the thing against real data.
