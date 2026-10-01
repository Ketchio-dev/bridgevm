# A9 r53 helper lost at snapshot restore — failed physical diagnostic

Classification: one failed, nonpromoting physical diagnostic. The current A9
criterion, defect wording and product state come from
`capabilities/windows-hvf.json`. A9 remains OPEN and the product remains
ENGINEERING_PREVIEW.

## Sealed observation

Physical job `t17-8ccdc0f8-export-order-pilot-r53` ran
`8ccdc0f8fe8ac1f5ed198de2b70ef55ec7304011`, the PR #287 head that joins the
ramfb display export before the final report (see
[r52](a9-r52-display-export-tail-20260930.md)). The app was a local
exact-source Apple Development build, and the Accessibility preflight and
console check passed. The input manifest SHA-256 was
`a83648cb0c8c8dd5aef1ef4070b408935e6572099f2f83f217141778b097e830`.

The tier ran on Mac17,9, macOS 27.0.1, from 23:45:24 to 00:03:13 UTC. It ended
with `cleanup-failed`, and the worker fenced the queue. Byte-identical receipts
(SHA-256 `d1f837b2e13fc0e128117a631b876f26bd9aa805c399771617ae5e184326a1d1`)
passed the strict read check. The helper wrote no lane result, so the receipt
counts no stage. All 37 half-minute samples taken while the job ran found the
console unlocked. The product app was frontmost in 36 of them; the other was
taken at job start, before the app launched.

## What the lane reached

The helper archives `first-run.log` only after the first shutdown, the
B7-aligned audio check and the audio and first-shutdown stages have passed. It
archives `mutation-run.log` only after a snapshot, a second boot to READY, the
MarkerB workload and a second clean shutdown. Both files were present in the
lane, along with a snapshot pair that the helper verified after creating it at
23:58 UTC. The mutation boot's log ended `stop: PSCI 0x84000008 (system off)`.
So r53 is the first hardware run to pass:

- the audio check;
- the first clean shutdown;
- product snapshot creation; and
- the mutation boot.

It also reconfirmed keyboard and pointer, clipboard, folder share and network.
In every archived probe process, the display export's stop record now came
before the report's stop line. Export cadence held 30.3 polls a second, with
11.8 to 24.4 changed frames published a second.

The journey then pressed the product's snapshot restore. Snapshot-restore
verification hashes the 64 GiB lane disk and the 64 GiB snapshot disk back to
back. About three minutes after the mutation shutdown, the helper process
ended. It left:

- no result file;
- no crash report;
- a still-running product app, which its own cleanup would have terminated.

The `open --stderr` helper log is empty in r52 and r53 alike, so a blocker
message on that stream would not have survived.

## Reading

The helper's file digest read 1 MiB chunks with no autorelease pool, so every
chunk stayed resident until the digest returned. With that loop, a 4 GiB sparse
file peaked at 4.32 GB resident; with a pool, the same digest peaked at 7.8 MB.
Two 64 GiB disks hashed in one expression therefore fit the helper being
killed under memory pressure. No jetsam record was found, so that cause is
strongly indicated, not proven.

## Cleanup and a lost record

After the job, the r53 product app was still running. It exited at once on
SIGTERM. No process or mount referenced the lane tree. The tier's owned-tree
cleanup then removed the tree, and the job was finalized as the worker does:
its worktree was removed, the job was moved to `done` and the fence was cleared.
The lane's VM logs were not copied to the private job directory before that
cleanup. The r53 CoreAudio continuity records are therefore lost; only the
values quoted above, read before cleanup, remain.

## Follow-up

The following changes have not run on hardware:

- Each digest read now runs in its own autorelease pool. A helper test failed
  on the previous loop (1.08 GB of peak growth for 1 GiB) and passes on the
  fix. The app's installer-image cache identity hash got the same fix.
- A separate change starts CoreAudio playback after a 40 ms reserve, aimed at
  the r52 underruns.

A9 and A11 remain OPEN, and product state stays ENGINEERING_PREVIEW.
