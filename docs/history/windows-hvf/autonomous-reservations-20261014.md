# Reservation and cancellation follow-up — 2026-10-14

Evidence rank: **test**. This sixth isolated integration follows
[the import and admission checkpoint](autonomous-admission-20261014.md).
Focused counts overlap existing suites and must not be summed as unique tests.
Combined exact-source local and hosted checks remain separately required.
Earlier hosted failures remain failed; these repairs do not make earlier
sources release eligible or promote any live criterion or product state.

## Deferred GPU configuration notifications

A masked GPU configuration notification could be delivered again after
MSI-X unmasking: pending-message replay left its source latch set. Replay now
consumes the latch only when a newly delivered message matches the configuration
vector, preserving unrelated vectors and existing caller output.

The final unchanged nine-case baseline passed three and failed six; all nine
passed after repair, alongside 142 related GPU cases. An earlier two-pass,
four-failure baseline and its shell-footer error remain recorded. The unrelated
vector fixture was strengthened to prevent a prior pending configuration bit
from hiding an incorrect global clear. The platform baseline stopped at its
first vector-mask failure; repaired coverage reaches both mask branches, with
an independent direct function-mask baseline also failing before repair.
This is device/platform model evidence, including shared-vector coalescing and
acknowledgment. It proves neither physical MSI delivery nor guest rendering;
guest status-zero reset is outside this repair. Existing notification semantics
are restored without a new intentional guest-contract deviation.
Patch SHA-256:
`cf11a0f53fb92a6073ae6d85974e2c5b26b7f6d3f66869f43dc8a7689663eb40`.

## Canonical helper identity in optional import fixtures

The optional-state clearing fixture compared the production resolver's canonical
helper path with an expected path containing an aliased ancestor. Its retained
baseline binary ran four methods with three failures under an owned alias and
four passes under the physical path. The three relevant baseline source hashes
match the candidate; this was not a whole exact-candidate baseline build.

The single test-only change resolves the expected helper path's symlinks before
the same exact equality assertion. Fresh native physical-path and alias-path
selections each passed four cases. The actual-shim alias selection passed nine
with zero skips, including five unchanged transaction controls. The real helper
copied tiny owned disk/vars fixtures and registration was read back. This uses
the DEBUG helper override and a deferred unshown window; it proves no physical
input, real recovery/key operation or guest behavior. Production helper lookup,
release policy, validation and import behavior are unchanged.
Patch SHA-256:
`28d73d0acff868e44b48ebb81f630689b4fc67dd8542923356db40eca9503b50`.

## Preserving occupied CLI clone destinations

A failed public CLI or `VmStore` clone could delete a preexisting dangling
destination symlink. Clone output now requires exclusive, nonrecursive directory
creation before copying or cleanup; occupied destinations return `AlreadyExists`.
The same six CLI/API fixtures passed four and failed two before repair, including
a formatting-only baseline repeat, then passed all six after repair. Eighteen
storage clone cases also passed: five new controls and thirteen existing cases.

Controls cover occupied files/links, source-lease refusal, populated-output
cleanup and four concurrent creators producing one surviving winner. No compile
or repaired-run failure was recorded. These synthetic-file tests use no live
guest, real media or external image tool. Reservation assumes a non-hostile
namespace; path cleanup does not authenticate a later replacement of that root.
No mid-copy I/O fault, crash durability or atomic publication is proved.
Patch SHA-256:
`a7bf2ce11a99c1f9e273c06eb18fca7d9466a4db8623ae549e59af9ffeda2b80`.

## Retained installer cancellation availability

After its registration disappeared, the retained install route still offered
Cancel during commit and while cancellation awaited acknowledgment. Its two
production-line repair uses the session's existing `canCancel` policy and the
ordinary control's cancelling label. Pipeline and commit behavior are unchanged.

The final two-method baseline failed three assertions: pending cancellation
remained enabled, and finalization allowed activation that latched cancellation.
The repaired native selection passed eleven cases; the two new owned-SSD cases
also passed the actual shim with zero skips. The other nine native controls use
the system temporary directory; no SSD-location claim is made for them.
Precommit activation invokes cancellation, repeats and premature dismissal are
refused while acknowledgment is pending, and terminal dismissal removes the
retained record. Gated finalization no longer admits that activation.

Earlier NSButton and accessibility-tree probes failed to find SwiftUI's button
in an unshown host and remain fixture failures, not product baselines. The final
public button-style probe reaches the real registration-reload/detail route and
checks environment availability plus action delivery in a deferred unshown host.
Rendered label and physical hit testing are not measured; the label condition
is source-reviewed. Injected pipeline effects launch no installer or guest and
touch no real Windows media, TPM keys or Keychain. No sanitizer result is claimed.
Patch SHA-256:
`d821212edc1dfe51c5440fecca7b025731d4113095ca06e36b6b2a317e194fd4`.

## Reserving a clone source against later generic work

The preceding admission repair checked generic writers already active when a
clone was requested. In the reverse order, accepted clone work could be waiting
in its scheduler while a generic Start or resource restart admitted overlapping
assets. The corrected baseline passed two methods and failed three, with eight
assertion failures; distinct-asset controls passed. An earlier fixture failed
compilation by capturing itself before field initialization and is retained.

Accepted clones now retain their original source configuration by slug before
scheduling, until transfer completion. Generic Start and resource admission use
the existing overlap classifier against that reservation, including after source
configuration changes or removal. Ordinary binding/staleness checks still run
first; HVF routing and ordinary unowned Stop policy remain unchanged.

Final native and the same actual thread-sanitized-shim selections each passed 67
cases with zero shim skips, including twelve new controls. They cover aliases,
distinct assets, same-turn duplicates, retained identity, completion and retry.
A follow-up replaces the default function conversion with a stateless closure;
the introduced Sendable warning is absent. Earlier receipts remain. The initial
tracked-only whitespace check missed an added test's trailing blank; that claim
is corrected by checking all seven staged files and ratcheting that test 54 to 53.

Reservation is local to this MainActor `LibraryModel`, not a durable or
cross-process writer lock. New fixtures use fake transfer/backend effects; they
measure neither native clone copying, OS-level VM locking nor guest corruption.
The default transfer still uses the existing protected clone operation.
Patch SHA-256:
`1008b617f0fb8f20b36c01b7639b1175ad0a43fd1b20857c989911a262e05d9a`.

## Boundaries

These focused deterministic conclusions retain failed experiments and independent
hash-bound review receipts. Existing structural ceilings are not raised. No
private paths or guest assets are included. Capability wording, live sample
counts and gate definitions remain unchanged. Full integrated local and hosted
checks still require their own receipts; release artifact preparation stays held.
