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

## Cached launch-readiness follow-up

Source: `61c02283814e9e1f1c754097f50cf348dcb539dd`.

A new test first established launch readiness with private disk/vars fixtures
and executable placeholders, then created a pending relocation record. Before
this fix the library excluded the VM but the cached configuration still returned
launchReady=true. The failed baseline is retained; no guest was launched.

Library-derived HVF configurations now carry their originating registration and
library root. Readiness returns the launch blocker `relocation-pending` while
that registration has an unresolved record. UI configuration editing preserves
the context by copying the session configuration before updating editable fields.
The isolated-root regression passes its explicit library root to the mapper.

The focused readiness and relocation tests passed 15/15, and
`scripts/check-project.sh` passed. The regression requires the specific pending
relocation blocker, rather than accepting an unrelated missing-file failure.

This does not establish process-launch race freedom, native runtime enforcement,
or protection of independently constructed development configurations without
library context. The test validates readiness, not a real Windows launch or UI
interaction. Automatic repair and power-loss durability remain unproven.

## Pending-source clone follow-up

Source: `bf6e910d17a41561d215049ff6175224f602d634`.

An actual VMLibrary clone test using a temporary bundle reproduced copying from
a source with a pending relocation record. The afterCopy callback was reached;
it returned false to stop before clone identity preparation or registration.
The failed baseline is retained and was not reclassified as a passing result.

The clone entry point now rejects a pending source record before destination
reservation or copying. The focused clone, relocation and cached-readiness tests
passed 20/20, and `scripts/check-project.sh` passed. The regression confirms the
copy callback is not reached, the pending record remains and source fixture
bytes are unchanged.

The lookup uses the supplied library root. This preflight is not a substitute
for media ownership, does not prove concurrent direct API calls are race-free,
and does not repair the pending relocation. No Windows live criterion is closed.
