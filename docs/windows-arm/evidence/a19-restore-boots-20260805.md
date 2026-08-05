# A19 acceptance item 5: a restored snapshot boots, with its marker (2026-08-05)

## What was missing

The pair gate proved bytes round-trip through create, verify, clobber and
restore at 64 GiB. That is not the claim a user cares about. They care that
rolling back gives them the machine they snapshotted -- that Windows boots from
the restored pair and the state they had is the state they get.

## The gate

`scripts/verify-snapshot-restore-boots.sh`, tier `t1-restore-boot`. Three
Windows boots around one snapshot:

1. boot, write `BV-ORIGINAL-<epoch>` to `C:\bv-snapshot-marker.txt`, power off
2. snapshot the powered-off pair, then verify it
3. boot, read the marker back, overwrite it with `BV-CLOBBERED-<epoch>`, power off
4. restore
5. boot, read the marker

Step 3 is the part that makes this a test. Without it, a restore that did
nothing at all would pass, because the original marker would still be sitting
on the disk. The gate requires the clobbered marker to be **gone** as well as
the original to be **back**.

The marker is on `C:` and not in the shared folder, on purpose: the share is
host-side storage and is not part of the snapshot, so a marker there would also
survive a no-op restore.

## Result: PASS

Job `20260805-091656-10876-16297`, `result=pass exit_code=0`, against
`rethink-fresh-12041-agent-vioserial.raw` (68,719,476,736 bytes).

```
phase1-original  marker-before: BV-NO-MARKER
phase3-clobber   marker-before: BV-ORIGINAL-1785935910
phase5-restored  marker-before: BV-ORIGINAL-1785935910
```

The restored guest booted Windows to agent service state and reported the
marker written *before* the snapshot. The marker written *after* it
(`BV-CLOBBERED-1785937744`) was gone.

Hashes agree across all three operations:

```
disk_sha256  56b1361282933f2cf3e409604cdc2da81f8c6ad8536de2bdcc472818e204280d
vars_sha256  8cc43bf0a11d8d1d68253069fed42667950a701c2f508a38c69e35b72fca3563
```

identical in `create`, `verify` and `restore`.

## Two wrong turns worth recording

**The first image had no agent.** The initial run failed at phase 1 with
"guest never reached agent service state", which reads like a boot failure. The
ramfb capture showed a fully booted Windows 11 desktop with a running clock.
`canonical-fresh-12041-agent-20260730` has "agent" in its name but no ARM64
virtio-serial driver -- the 2026-07-30 re-diagnosis note says vioser went into
a *separate* candidate. No channel, no answer, however healthy the guest. The
gate now distinguishes the two cases: framebuffer output but no `BVAGENT` line
is reported as a probable missing driver rather than as a failed boot.

**The CLI takes positional arguments.** The first version passed
`--disk/--vars/--out/--quota` from memory and got a usage message -- after
phase 1 had already spent 20 minutes booting Windows and writing its marker.
The fixed version was checked end to end against scratch files before another
boot was spent on it.

## Scope

Acceptance items 1, 2, 3, 4 and 6 were proven by the pair gate. With item 5
proven here, the A19 acceptance boundary in `docs/windows-arm/snapshot-scope-v1.md`
is closed.
