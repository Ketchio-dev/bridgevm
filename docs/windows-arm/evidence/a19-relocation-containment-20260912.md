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

## Read-only recovery guidance

Source: `fdf0bf88ffd45ff1d7ecbd307eb5883ef0a3a3d5`.

Library issues now display recorded original and destination bundle paths when
the pending record decodes with the expected schema and matching VM identity.
Malformed records retain generic blocking guidance. Displaying recorded paths
does not establish that either location exists or contains complete media.

Record reading uses O_NOFOLLOW, O_NONBLOCK, an opened-descriptor fstat check,
and a read capped at 1 MiB plus one overflow byte. Non-regular and oversized
files are refused. This replaces an initial path-size check followed by an
unbounded whole-file read, which did not bound allocation if the file grew.

The focused suite passed 23/23 tests and `scripts/check-project.sh` passed.
Additional tests cover actual-library path guidance and malformed-record
fallback, ordinary file reads, oversized sparse files, symlinks, directories,
missing files and FIFOs. These tests use private temporary files only.

The reader does not claim snapshot consistency against concurrent writers or
protection against substitution of intermediate path components. Guidance is
read-only: no record is removed and no media is selected or repaired. These
results do not close a live Windows or power-loss recovery criterion.

## Failure before a move attempt

Source: `05fc066f247aad545cec4c80e8cdaaaf910af210`.

Destination-parent creation now follows path-safety preflight but precedes the
pending record. If that creation fails, the VM is not marked as requiring move
recovery because no move has been attempted. Failures once the move operation
begins still retain the pending record; no inference about partial copies was
added.

A FileManager fault-injection test verifies zero move attempts, preserved source
registration, no pending record and no destination directory after setup fails.
The focused suite passed 24/24 tests and `scripts/check-project.sh` passed.
This is deterministic setup-failure coverage, not a cross-volume or live guest
recovery result.

## Exclusive synchronized intent creation

Source: `05b122c0fe875f64a18f4ff3bd0a6aac031dbf99`.

Pending-record creation now reuses the private-file writer for O_EXCL and
O_NOFOLLOW creation at mode 0600, complete writes and file fsync. The relocation
writer then opens and fsyncs the parent directory before returning. The earlier
atomic JSON replacement described above is superseded for intent creation.
Existing records are not overwritten, including when another writer wins the
creation race after the initial existence check.

The focused suite passed 26/26 tests, and `scripts/check-project.sh` passed.
New tests exercise normal record creation and its permissions, refusal to
overwrite existing contents, and refusal of symlink destinations without
changing the target. These are private-file tests, not power-cut experiments.

Failed creation or synchronization can leave a partial or complete pending
record and prevents the move from starting. Such records remain fail-closed.
Record removal is not yet directory-synchronized. No F_FULLFSYNC guarantee,
complete transaction durability, recovery automation or power-loss criterion
pass is claimed by these changes.

## Concurrent record creation coverage

Source: `1c6364d697237a1e26c4931c2059db0b741e5a08`.

An additional private-file test calls the production record writer from 16
concurrent attempts against the same destination. Exactly one succeeds, the
remaining 15 report EEXIST, and the retained bytes match the successful writer.
Result collection uses a lock; no guest media or key material is involved.

The focused suite passed 27/27 tests and `scripts/check-project.sh` passed.
This checks contention over exclusive record creation, not multiple complete VM
move operations, process-crash recovery or power-loss durability.

## Registration replacement synchronization

Source: `e8e1f45703738bde46d2a406d0d1501b4bdc0c60`.

VMLibrary.save now writes a private temporary file in the registration directory,
fsyncs its contents through the existing private-file writer, replaces vm.json
with rename, and fsyncs the parent directory before reporting success. Existing
symlink preflight and Boolean failure reporting remain in place. A failure after
rename can still mean the replacement is visible; false does not promise that
the previous registration is intact.

The selected registration, library and relocation suite passed 46/46 tests;
`scripts/check-project.sh` passed. New private-file tests cover creation and
replacement with mode 0600, cleanup after rename refusal, and preservation of
a directory mistakenly used as the destination. Existing lifecycle tests also
exercise the new writer through VMLibrary.save.

Failure inside private-file creation may leave a private temporary file. This
change does not synchronize guest media, guarantee hostile-path race safety or
prove complete power-loss recovery. These limits remain outside the tested
registration replacement behavior.
