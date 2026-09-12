# A19: a powered-off snapshot survives the disk being clobbered (2026-08-05)

## Why this document exists

A19 claimed its figures against `docs/windows-arm/snapshot-scope-v1.md` and two
source files. That scope document defines the acceptance items; it does not
contain the numbers A19 quoted, so the claim could not be checked from anything
the repository shipped. The run those numbers came from is still on disk, and
this records it.

## The run

Live-queue job `20260805-091656-10876-16297`, tier `t1-restore-boot`, from
`~/BridgeVM/live-queue/done/`.

From `receipt.public.json`:

| field | value |
| --- | --- |
| `outcome` | `completed` |
| `pass` | `true` |
| `commit` | `e8eb8b9e3ac7` |
| `host_model` | `Mac16,9` |
| `macos_version` | `26.5.2` |
| `finished_at` | `2026-08-05T14:12:12Z` |

The image under test, from `create.txt`:

```
vm_id       a19-restore-boot
disk_bytes  68719476736
disk_sha256 56b1361282933f2cf3e409604cdc2da81f8c6ad8536de2bdcc472818e204280d
vars_bytes  67108864
vars_sha256 8cc43bf0a11d8d1d68253069fed42667950a701c2f508a38c69e35b72fca3563
```

`68719476736` bytes is 64 GiB exactly, so this is the real image rather than a
scratch file.

## Recorded observations

The job boots the guest three times and records a marker file from inside the
guest each time:

| phase | `marker-before.txt` |
| --- | --- |
| `phase1-original` | `BV-NO-MARKER` |
| `phase3-clobber` | `BV-ORIGINAL-1785935910` |
| `phase5-restored` | `BV-ORIGINAL-1785935910` |

Phase 3 reads the original marker before attempting to write a different one.
Phase 5 boots after restore and reports the original marker again.

These marker-before files alone do not prove the clobber write succeeded.
The legacy waiter could accept an unrelated END record. This historical
receipt needs its per-command logs; the stricter
[2026-09-12 run](a19-restore-boot-20260912.md) supplies fresh restore-boot evidence.

## What this does not cover

This job is `t1-restore-boot`. The other acceptance items in
`docs/windows-arm/snapshot-scope-v1.md` are proven by `t1-snapshot` through
`scripts/verify-powered-off-snapshot.sh`, which is a different job. This
document records only the restore-boot half, because that is the job whose
receipt is retained.

The original document reported per-phase logs under that job directory.
That directory was unavailable at the documented path on 2026-09-12.
