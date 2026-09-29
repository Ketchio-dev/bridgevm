# A19 snapshot output admission and setup interruption — deterministic

Classification: deterministic development evidence (automated tests, evidence
rank 3). The current A19 criterion, defect wording and product state come from
`capabilities/windows-hvf.json`. A19 remains OPEN; no live sample, physical
power-loss case or lifecycle count is added here.

## Snapshot output could delete an unrelated directory

`bridgevm app snapshot-export ID OUTPUT` accepted any absolute output outside
the VM bundle. The helper's create path refused only source overlap. It then
removed any existing `.NAME.staging` sibling recursively, and publication
exchanged any existing destination directory with the staged snapshot and
recursively removed the swapped-out tree. An existing directory at the export
path therefore lost its contents.

New tests reproduced this on the unmodified source before any production change:
9 of 13 new Rust admission tests failed, including a user file replaced by
`disk.raw`, `vars.fd` and `manifest.json`, an unclaimed staging tree deleted,
and data that arrived during the copy deleted by publication. A Swift CLI test
using the real packaged helper failed the same way: the export reported
`complete=true` and the directory's only file was gone.

Create now admits a destination only when it is absent, an empty directory,
or a previous snapshot of exactly the three regular files whose manifest parses
and whose sizes match. Anything else, including Finder metadata, is refused
before staging and left intact. The same check runs again immediately before
publication. Stale staging is cleared only when it holds nothing but files
BridgeVM writes there, including exFAT AppleDouble companions of those files.
The quota refusal still precedes admission, so the T21 quota contract is
unchanged. After the change the 13 admission tests, 5 added staging and
companion tests, the 10 A19 snapshot and quota tier contracts, and both Swift
admission tests passed.

Residual limits: the parent lease is cooperative, so a writer that ignores it
can still change an admitted directory between the final check and the
exchange. Exporting into an existing empty directory on exFAT still fails with
`ENOTSUP` after the copy, as it did before, because publication exchanges any
existing directory.

## Snapshot helper output could deadlock the app and CLI

The app and CLI waited for the snapshot helper to exit before reading its
combined output from one pipe. Four new watchdog tests with helpers writing
more than 64 KiB hung on the unmodified source. The helper's output is now
drained concurrently, capped at 1 MiB with a fail-closed overflow result, and
stdin is `/dev/null`. The overflow result says a create or restore may still
have completed. Tests pin exactly 1 MiB as success and one byte more as
failure.

## Interrupted managed-storage setup and repeated restore

A child process that exits after the managed root directory is created, but
before its `original` marker exists, previously left a root that every later
open refused. The new test failed on the unmodified source with that refusal.
Setup now builds the root under a private sibling, creates and syncs the
marker there, renames it into place and syncs the parent. A leftover sibling
is removed only while it is empty or holds only the empty marker; anything
else is kept and named in the error. Roots already left empty by an older
binary remain refused, because they cannot be distinguished from a root whose
selected generation was lost.

A second child-process test exits at four points inside a repeated restore's
generation exchange over an existing restored generation. It passed on the
unmodified source, so it is coverage, not a defect reproduction: a fresh owner
selected exactly the old or the new complete generation, and a retry published
the new one.

## Limit

These are process-death and filesystem tests on one host. They do not model
sudden power loss, do not add a live sample and do not change the undeclared
lifecycle sample count. A19 remains OPEN and product state remains
ENGINEERING_PREVIEW.
