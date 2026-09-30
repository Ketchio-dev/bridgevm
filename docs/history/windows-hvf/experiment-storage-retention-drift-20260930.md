# Experiment storage retention drift — 2026-09-30 audit

Document status: **Historical evidence**

Classification: a storage audit record, not guest or performance evidence. It
retracts the retention statements of the
[2026-09-06 cleanup](experiment-storage-cleanup-20260906.md) and changes no
campaign conclusion, criterion or product state.

## What the 2026-09-06 record says

The 2026-09-06 cleanup removed ordinals 2–20 of T15 campaign
`b930c0b3d0c074d4347c4bcd718fb154` and T16 campaign
`a40f06e9d0835f324f413371fed57d75`. It kept one representative output disk per
campaign (`20260901-115951-25472-19428` and
`t16-a40f06e9d0835f324f413371fed57d75-001`) and all 2,164 hashed non-disk files
of the 40 runs, including every run's vars. It also left in place the disks of
the two invalid earlier T16 campaigns and their campaign inputs.

## What exists now

A read-only audit on 2026-09-30 found that a later cleanup on 2026-09-21
removed completed-job media from 326 queue jobs. That cleanup was recorded only
in a private report outside this repository. It removed:

- both representative disks and the vars of all 40 T15/T16 runs;
- the media of the invalid T16 runs `t16-cbacd53c634542839dd3e0152a3bf733-001`
  to `-004` and `t16-651d8eb0ae06c6f641673d3580f29abe-001` and `-002`;
- the three T16 campaign-input trees; and
- the RAW files of five T1 restore jobs, including
  `t1-restore-cfae89d1-managed-pair-r4` and
  `t1-restore-ff78cca1-relocated-pair-r5`.

A direct check of the 40 T15/T16 run directories found each directory present
with no vars file and no disk image. A re-hash of the 2026-09-06 preserved sets
matched 960 of 980 T15 files and 1,164 of 1,184 T16 files. No file changed; the
40 missing files are the vars. The two T1 restore jobs keep both receipt copies
and no RAW file. The T16 v1 campaign registry file was not found in a
name-scoped search.

## Consequences

- The 2026-09-06 statements that the ordinal-1 disks, the vars and the invalid
  T16 campaigns' disks are retained no longer hold. The investigation route
  "representative disk and its vars" is not available for T15 or T16.
- `scripts/archive-experiment.py` can no longer plan either campaign, because
  it requires the ordinal-1 disk.
- Neither campaign's STOP depended on those disks, so both STOPs stand as
  recorded. No failed result is replaced by this record.
- A19's measured note cites the outcomes of the two T1 restore jobs. The
  receipts that record those outcomes are present; their disks can no longer
  be re-examined.

Whether the 2026-09-21 rule (remove completed-job media once a job is done)
should replace the representative rule in
[experiment retention](../../reference/experiment-retention.md) is an operator
decision. It is not made here. No file was deleted, moved or changed by this
audit.
