# Snapshot retention, evidence parsing and input deadlines — 2026-10-02

Classification: deterministic tests and static review. No live guest run or
release gate was performed. The capability registry retains all criterion
states, known defects and ENGINEERING_PREVIEW product state.

## Confirmed defects and resulting behavior

- T19 retention read the original disk and variables after T17 restored a
  managed generation. It now exports the selected pair through the sealed
  product's snapshot helper under the pair lease, rechecks its authenticated
  hashes, and verifies the retained import manifest before making it immutable.
- Retention cleanup chose any destination that existed after an error. A
  concurrent destination containing an unrelated file was deleted in a
  deterministic reproduction. Publication now uses an exclusive rename;
  cleanup checks ownership, quarantines its directory and traverses checked
  directory descriptors. Concurrent destinations and replacement trees stay
  intact. This is a private cooperative workflow, not a claim against every
  possible hostile mutation by another process under the same account.
- The snapshot parser accepted malformed documents and interpreted `1e2` as
  format version 1. Such a document could authorize replacing a snapshot.
  Typed object deserialization rejects ambiguous known fields, malformed JSON
  and noninteger values; control-character and Unicode identifiers round-trip.
  Valid unknown extension fields remain compatible with version 1.
- Snapshot verification read an unbounded manifest and could block on a FIFO.
  Verification, restore and replacement admission now share a 64 KiB reader
  that requires an opened regular file and refuses symbolic links and FIFOs.
- Native export evidence accepted numeric success flags, Boolean versions and
  duplicate JSON names. Bounded reads now reject these values and hash the
  exact metadata bytes that were checked.
- T23 lost completed lane counts when a later runner error interrupted its
  return. It now retains the checked records before proceeding. Extra or
  incomplete lane directories still prevent strict publication; cleanup
  residue still fences the job. Contradictory prepared-image identities remain
  fail-closed and cannot be converted to passing evidence.
- T17 chooser and button actions could send input after a blocking readiness
  read or retry pause consumed their deadline. Stage and press guards now stop
  that next action while retaining its failure context. This proves stage
  boundaries; it does not bound every nested Accessibility operation.

## Deterministic validation

- Snapshot suite: 123 tests passed; strict Clippy and Rust formatting passed.
- Chooser and button suites: 83 tests passed, including delayed-action cases.
- Actual snapshot-helper retention: four tests passed; two mutation cases
  refused changed selected media and removed owned staging. Logical original
  media stayed unchanged.
- Retention publication and cleanup races: five and three tests passed.
- Export JSON: five new adversarial tests passed. Campaign progress: three
  new tests passed, along with the existing export and campaign suites.
- Structural ceilings were only reduced or registered at actual size.
- New regressions run in `scripts/check-project.sh` and a GitHub-hosted
  snapshot-evidence workflow. Deterministic work never used the live queue.

## Failed experiments retained

The first derived manifest struct also accepted a positional JSON array. Its
new adversarial test failed; the final object wrapper rejects that array.

The first retention fixture run failed after its teardown followed a helper
symlink and removed the source executable's execute bit. Teardown now skips
symlinks; the corrected real-helper suite passed.

Two complete local project-check invocations failed. Process queries and
LaunchServices were unavailable inside the agent sandbox; nested SwiftPM
sandbox execution was also refused. The first invocation additionally saw an
in-progress lockfile change. A minimal signed shell app and a minimal signed
native app both reproduced the LaunchServices refusal. No check was relaxed.
The later full local log SHA-256 is
`ab2495382245f65dd473246426fbfcf1055838c55e551372c110106461ce0d72`.
Exact-head GitHub-hosted verification is required before this branch is done.
