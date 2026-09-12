# Relocation failure containment

Source: `936cfe0954814fd317903285ac39db002e8f6952`.

## Observed defect

A focused filesystem experiment using production relocation code and registration
and receipt adapters reproduced a stale registered destination after successful
physical rollback: the recovery registration write failed. The original bundle
existed while the persisted destination did not. This was not a Windows live run.

## Implemented containment

Before moving a registered bundle, the relocation path writes a pending record
containing original and destination configurations. It removes that record after
successful completion or observed-location registration recovery that saves
successfully. Failed initial moves and unresolved recovery retain the record.
Library scanning excludes entries with pending records and reports an issue.
Existing pending records cannot be silently overwritten by another move.

This does not repair an unwritable registration directory. Excluding an entry is
not proof that its media is complete, nor a global lock against direct API calls.

## Deterministic evidence

`swift test --package-path apps/macos --filter VMRelocation`: 14 tests passed.
Three new cases use actual VMLibrary persistence, relocation and scanning:

- Pending records exclude entries, resist replacement, and remain blocking when
  their contents are malformed.
- A registration-directory permission fault during rollback leaves stale
  registration, preserves the original fixture bytes and pending record, and
  excludes the entry from library results.
- An injected initial move failure leaves both locations present, preserves the
  pending record and excludes the entry without guessing which copy is complete.

The fixtures use temporary directories, not canonical disks or real VM keys.
The partial-move case injects a FileManager failure; it is not a cross-volume
media-copy or power-loss test.

`scripts/check-project.sh` passed after the tests were added. An earlier check
failed at 68 lines against the existing 67-line relocation budget. Extracting
rollback handling resolved that failure without raising the ceiling.

## Remaining requirements

Automatic recovery, recovery UI, complete interrupted cross-volume copying and
power-loss durability remain unproven. Atomic JSON replacement alone is not a
power-loss durability claim. These tests do not close A19 or promote the product.
Hosted CI must separately succeed for the pushed evidence-seal SHA.
