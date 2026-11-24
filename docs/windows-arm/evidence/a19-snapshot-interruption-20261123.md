# A19: random kills during powered-off snapshot create, restore and export

A19's known defect says full interrupted-operation coverage is unproven. These
runs kill `bridgevm app snapshot-*` and its `snapshot_pair_cli` helper with
SIGKILL at a random moment of the operation, on a real installed Windows pair,
and check what the VM is left with. A SIGKILL is a process crash: it tests the
helper's ordering and recovery, not a host power loss (outside A19).

## Harness

    APP=<BridgeVM.app> TARGET=disk.raw VARS=vars.fd OUT=<new dir> TRIALS=9 \
      scripts/verify-snapshot-interruption.sh

- Private APFS clones of the installed Windows 11 ARM64 pair used by the A19
  lanes (64 GiB disk, about 17 GB allocated), with their own vars, in a fresh
  app library; deleted afterwards. 6144 MiB, 4 vCPUs, 3D off.
- Each trial picks the operation (the committed script cycles create, restore,
  export; the two earlier runs alternated create and restore), sleeps a time
  drawn uniformly from 0.2 s to 1.05 × that operation's last measured
  duration, then SIGKILLs the CLI and every descendant at once. The app CLI
  starts the helper in its own process group, so killing the CLI alone would
  not reach it.
- Then it requires:
  1. the VM resolves to exactly one current disk+vars pair;
  2. that pair is byte-exact (sha256 by the script): unchanged after an
     interrupted create or export, the old pair or the newest snapshot after
     an interrupted restore;
  3. the same operation then succeeds without repair and is exact;
  4. after a create or restore, Windows boots that pair through the packaged
     release probe and reads back the marker written into exactly that pair,
     writes a new one and shuts down cleanly (SYSTEM_OFF, NVMe write-back).
  The committed script also requires an interrupted export to leave its
  destination absent, as the previous export, or complete; and, after every
  retry, no `.NAME.staging` and no snapshot directory with content but no
  manifest.

## Runs (Mac Studio M4 Max, macOS 27.0.1, 2026-11-23 UTC)

| Run | Package (sha256 of helper / probe) | Trials | Interrupted | Finished before the kill | Result |
|---|---|---|---|---|---|
| pre-#719, scratch harness, 05:48–07:03Z | ad-hoc of main `d7e76a5e` (`bff425df…` / `6d67ccc0…`) | 10 (5 create, 5 restore) | 8, helper killed in all 8 | 2 restores | PASS |
| post-#719, scratch harness, 06:08–08:12Z | ad-hoc of main `cd333427` (`2b841505…` / `b35b3b7b…`) | 20 (10 create, 10 restore) | 18, helper killed in all 18 | 2 restores | PASS |
| `scripts/verify-snapshot-interruption.sh` as committed, 08:25–09:11Z | ad-hoc of the tree of main `1a3ba015` (after #722–#726; `67661cf1…` / `65c86095…`) | 9 (3 create, 3 restore, 3 export) | 8, helper killed in all 8 | 1 restore | PASS |

- Kill moments: creates 0.9–51.6 s into runs of 45–96 s; restores 7.8–178.9 s
  into runs of 47–195 s; exports 12.8–43.9 s into runs of 45–46 s (the host
  ran other VM work at the same time, so durations are noisy).
- Every interrupted create and export left the current pair unchanged, and
  every interrupted export left its destination as it was (absent, or the
  previous export). Every interrupted restore left the old pair; the five
  restores that finished before the kill left the snapshot. Every retry
  succeeded and was exact. All 36 boots after a retry read the marker of
  exactly the pair they booted.
- On the candidate package no retry left any staging behind, and the library
  ended at 4 snapshots and about 109 GB as counted by `du` (the post-#719 run
  ended with 10 orphaned stagings and about 417 GB). A restore retried after
  an interrupted restore took 137–144 s against 47 s otherwise: with an
  earlier attempt's staging present it still hashes the source before
  reclaiming that staging (#723).

## Findings, fixed after these runs

- **Leaked staging (#725).** Each interrupted create left a hidden
  `.NAME.snapshot.staging/{disk.raw,vars.fd}` and an empty `NAME.snapshot`
  reservation that no later create removed: 5 after the first run, 10 after
  the second. The snapshot list ignored them; on APFS each pinned the disk's
  old blocks as the VM changed, elsewhere each was a full copy. A create now
  clears what killed creates left beside it. The committed script checks this.
- **Ctrl-C did not stop the helper (#722).** Killing or interrupting only the
  CLI left `snapshot_pair_cli` running, so a restore could change the VM after
  the command had ended. The CLI now forwards SIGINT/SIGTERM to the helper,
  waits and reports `cancelled`.
- **Restore hashed its source twice (#723).** Now once, as the staged copy it
  publishes (median 156.6 s → 74.0 s on this pair).

## Reading

On these packages the snapshot helper is crash-safe at every moment the
random kills reached (34 helper kills in 39 trials): no interrupted create,
restore or export left a mixed, partial or unbootable pair, and no retry
needed repair. Exports were interrupted only in the candidate run (3 kills). Rank: local
physical-Mac runs, no queue seal. Not covered: host power loss, a full or
failing volume, non-APFS destinations, and kills of the app (GUI) process.

## November 24 follow-up

Across five earlier packages, the full local record contains 54 successful trials.
The harness found a helper present for 47 kill attempts across 23 creates, 16
restores and eight exports. Seven restores finished before the scheduled signal.
Those attempts are distinct from observing a child exit on SIGKILL.

Ten independent Windows lifecycle runs on the locally installed build from
`b8dcb32a` restored the original guest file, after a verified overwrite, on all
ten boots of the exported pair. On the hosted build of the same source code,
the restore helper was killed immediately after the current directory changed
and reaped on SIGKILL. The ordinary app CLI retried the restore, exported the
pair and booted Windows. The original file was present with its expected
contents. All three boot phases ended in guest shutdown and NVMe write-back.
[Hosted package build](https://github.com/Ketchio-dev/bridgevm/actions/runs/45299518867)
· [Restore recovery](https://github.com/Ketchio-dev/bridgevm/pull/741)

The hosted package's snapshot helper also ran with a writable 128 MiB APFS
disk image on the development Mac. The source was on another volume, so each
operation needed space for a copy. A replacement export had 9,207,808 bytes
available for a 4 MiB disk and 16 MiB vars. A managed restore had 31,404,032
bytes available for a 4 MiB disk and 48 MiB vars. Both returned a space error.
The old export including its manifest still verified, the selected managed
disk and vars still matched their prior hashes, and neither attempt left new
staging entries. Removing the test filler allowed both ordinary retries to
publish the complete new pair. These storage fixtures used synthetic files.

A19 remains open. The ten-lane campaign belongs to the designated release
build. The per-operation byte quota was removed in [#711](https://github.com/Ketchio-dev/bridgevm/pull/711).
