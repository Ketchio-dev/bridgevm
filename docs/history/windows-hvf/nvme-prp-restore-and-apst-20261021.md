# NVMe PRP handling after restore and in APST data — 2026-10-21

This records automated tests of the own-HVF NVMe model. Product state, capability
wording, criterion states, thresholds and known defects remain unchanged. No guest,
physical Mac or live gate was used.

## How they were found

A read-only review of guest-facing devices reported both defects from reading the code.
It found no guest-reachable panic, no host memory or file access outside its range, and
no host allocation bounded only by the guest in TPM TIS and PPI, pflash, NVMe queues and
LBA arithmetic, and virtio-blk. The review's claims were taken as hypotheses. The tests
below confirm the two defects; the review's other judgments are static reasoning only.

## Defects

- **Restore broke PRP lists.** A checkpoint restore cleared the controller's scratch
  buffer. PRP-list decoding reads each list page into that buffer. After a restore, every
  read or write needing a PRP list, which means more than two pages, failed with Invalid
  Field on both data paths. The direct path never regrows the buffer, so the failure
  persisted until the controller was reset. Restore now gives the buffer back its
  constructor size. The field's comment states that minimum. Checkpoint restore is reached
  only through the developer `BRIDGEVM_RESTORE_STATE` runner variable; the app does not
  set it.
- **APST data ignored PRP2.** GET FEATURES for Autonomous Power State Transition wrote its
  256-byte table straight from PRP1. A table starting near the end of a page overran into
  the next guest page, and PRP2 was never written. The table now goes through the same
  admin-data writer as Identify and Get Log Page. A command without PRP1 keeps its earlier
  lenient success. Only the guest's own memory was affected.

## Results

| Test | Unchanged source | With the change |
| --- | --- | --- |
| Restored controller, 3-page transfer through a PRP list | status 2, no data moved, in all 4 read/write by direct/buffered cases | all 4 succeed with the expected bytes |
| APST table with PRP1 at page offset 0xF80 | page after PRP1 zeroed, PRP2 untouched | page after PRP1 intact, table split across PRP1 and PRP2 |
| NVMe suite | 199 passed, 1 benchmark ignored | 201 passed, 1 benchmark ignored |

Clippy with `-D warnings` passes for the crate's targets. A deliberately planted lint
confirmed that it checks this crate. The new head requires its own complete local and
hosted checks.
