# A19: managed and relocated Windows restore boots

Status: A19 remains OPEN. These are two distinct single-run receipts, not a
pooled sample set and not proof of all interrupted-operation requirements.
Canonical images were not modified; each job used separately staged private media.

## Normal managed generation: r4

- Job: `t1-restore-cfae89d1-managed-pair-r4`.
- Runtime: `cfae89d18e4de9ce51adc5631825ae891c1a7aed`.
- Main hosted CI: [34667123912](https://github.com/Ketchio-dev/bridgevm/actions/runs/34667123912), success before submission.
- Run: 2026-09-12 02:19:58 to 02:24:06 UTC; result pass, exit 0, sample count 1.
- [Public receipt](a19-managed-pair-20260912-normal-receipt.json), SHA256 `20983d1dba0b36304b4fdef70a63fccf2766e26b32e7bd44f1492b9eaa249332`.

| Phase | Marker read before writing | Marker written and read back |
| --- | --- | --- |
| Original | `BV-NO-MARKER` | `BV-ORIGINAL-1789179645` |
| Clobber | `BV-ORIGINAL-1789179645` | `BV-CLOBBERED-1789179734` |
| Restored | `BV-ORIGINAL-1789179645` | `BV-FINAL-1789179813` |

Phase 5 attached and wrote back the 64 GiB disk under
`live/.bridgevm-pair-764b9df8fda8cbde82a68b11a51c59a9827e3ade360c308d1f5de0012403d3f8/current/disk.raw`.
This confirms the native VM used the selected generation, not the logical original.

## Whole-directory relocation before restored boot: r5

- Job: `t1-restore-ff78cca1-relocated-pair-r5`.
- Runtime: `ff78cca110f3713f2ea8683b780d07c9b540b2ba`.
- Main hosted CI: [34668599418](https://github.com/Ketchio-dev/bridgevm/actions/runs/34668599418), success before submission.
- Run: 2026-09-12 02:51:00 to 02:55:07 UTC; result pass, exit 0, sample count 1.
- [Public receipt](a19-managed-pair-20260912-relocated-receipt.json), SHA256 `fc7143c070af231b00df3ba5a1f326b5bb72a7e1d5782ae5f4e96d9e35718d45`.

| Phase | Marker read before writing | Marker written and read back |
| --- | --- | --- |
| Original | `BV-NO-MARKER` | `BV-ORIGINAL-1789181507` |
| Clobber | `BV-ORIGINAL-1789181507` | `BV-CLOBBERED-1789181589` |
| Restored after move | `BV-ORIGINAL-1789181507` | `BV-FINAL-1789181674` |

After restore, the gate moved the complete owned `live` directory, including
hidden generation storage, to `relocated-live`. Phase 5 attached and wrote back
`relocated-live/.bridgevm-pair-v2-241b98d380470e00261ebee78389687c0e6e6dea0f2e551a919a99ae4b1c4409/current/disk.raw`.
The original marker returned; the clobbered marker did not.

## Shutdown evidence and limits

All three phases in each job recorded `shutdown.exe /s /t 0 exit=0`, a 64 GiB
NVMe writeback and PSCI `0x84000008` system-off. No host-forced termination is
counted as successful guest shutdown. Raw logs remain in the private queue's
completed job directories; public receipts contain hashes and counts, not media.

The [old two-rename failure](a19-restore-atomicity-defect-20260912.md) remains a
failed experiment. A later tiny CLI fixture also reproduced stale-original
selection after moving an absolute-path-bound generation. Relative V2 identity
fixed that fixture, and r5 adds real Windows boot evidence for a whole-directory move.

These runs do not prove power-loss safety at every publication/fsync boundary,
all quota/refusal cases on real media, arbitrary export/import, old-binary
compatibility, live snapshots or RAM resume. Copying only logical original files
does not copy current managed state. Already-relocated legacy storage without
a valid binding is refused rather than guessed. The original A19 criterion and
its release-blocking OPEN state are unchanged.
