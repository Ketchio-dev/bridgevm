# d6 overview AX refusal persists at seal 6121a878; the driver now names the failing element — 2026-10-23

Classification: one live single diagnostic run (evidence rank 2), plus
deterministic tests (rank 3) of the change made in response. It is not
criterion evidence and changes no product wording. No live run has exercised
the change.

Branch `agent/d6-ax-failure-locus-20261023` from the seal `6121a878` of draft
PR #428:

- `a132612e`: a d6 refusal's AX context admits a well-formed failure locus, and
  every reply validator refuses a malformed one.
- `d37197a0`: the d6 driver records that locus when an AX call of its read-only
  pass fails.

Only `d6-app-ui-host-v2` changes. The install-cancel driver records no locus,
and its reply validator refuses one.

## Live run 2 (rank 2)

Job `bvm-6121a878-native-ui-v2-20261023-02`, tier `d6-app-ui-host-v2`, source
`6121a878deb4cb638ae838de903e2499de2b6a57`, on the physical-Mac live queue.
Submitted 17:38:44, ran 17:39:28 to 17:39:58 UTC. The seal carries the three
changes of [the earlier record](d6-overview-ax-race-20261023.md) as `5faeec4a`
(the pass restart), `30c9dd90` (`axRestarts`) and `30d1c447` (the host's wait
for both seeded rows).

- **Receipt:** `receipt.json` and the public copy are identical, SHA-256
  beginning `57ff82b9`. `outcome=diagnostic-incomplete`,
  `failure_code=observation-refused`, `worker_cleanup_verified=true`,
  `claim_eligible=false`. Manifest SHA-256 beginning `9095f8b5`.
- **Console:** unlocked in every sample, no Screen Sharing. Load before submit
  was 3.57 / 7.95 / 6.51, and another tool's ffmpeg processes ran from about
  17:33 to 17:36 UTC, before the job.

| Seq | Request | Driver reply |
| --- | --- | --- |
| 1 to 6 | welcomeControls, pressCreate, createForm, cancelCreate, pressImport, importForm | as at 5de1f8ca: observed, performed, observed (commit visible, three empty selections), performed, performed, observed (name visible) |
| 7 | pressOverview (phase `overview`) | refused: `ax-failure`, `axError` -25200 (kAXErrorFailure), `axContext` `copyAttributeValue` / `AXIdentifier` / `element` |

- **Driver completion:** `success=false`, `failureCode=ax-failure`, 7 requests
  processed, 3 mutations, `axRestarts=109`. Every restarted read-only pass of
  request 7 failed the same way until the phase deadline.
- **Host completion:** `failure=deadlineExceeded`, cleanup verified. The report
  marks every action through `import_opened` and not `overview_opened`; all four
  tripwires are 0; `model_creations` is 0 before seeding, 2 after and 2 seeded.
  Six screenshots were captured.
- **Launcher:** both applications exited and both identities were verified;
  the scenario was not complete.
- **Timing,** from file modification times only: reply 6 at 17:39:52.671,
  request 7 53 ms later, reply 7 at 17:39:57.702 (4.98 s after request 7, at
  the phase deadline) and the host completion 55 ms after reply 7.

Between reply 6 and request 7 the host captured the import form, called
`seedOverviewMetadata()`, which adds the two fixture VM rows to the library
sidebar, and requested `pressOverview`. The importForm walk of request 6 had
read every node's `AXIdentifier` without error.

Run 1, `bvm-5de1f8ca-native-ui-v2-20261023-01` at main `5de1f8ca`, refused the
same request with the same status and context 140 ms after reply 6. No d6 v2 run
has passed the overview press; the runs of 2026-10-14 and 2026-10-15 failed at
welcomeControls with -25201. The files are kept outside the repository, in the
operator's evidence directories `dev-20261021-evidence/live-20261023/d6-r2/result`
and `dev-20261021-evidence/live-20261023/d6/result`.

## The race hypothesis was wrong

The earlier record proposed, from static reasoning only, that the walk raced
SwiftUI's re-render of the library after seeding. That hypothesis was wrong.
With the press requested only after both rows were in the model, 109 restarted
passes, each after a 25 ms yield, met the same -25200 on the same read for
about 5 s. The status is persistent, not a transient re-render race, and the
restart cannot recover it. The earlier record is left as written.

What the refusal does not say is which element's `AXIdentifier` read fails.
That is what the next run must show.

## What the refusal now records

`apps/macos/scripts/AppUIDriverService.swift` runs each request's operations
through `apps/macos/scripts/AppUIDriverAXLocusAccess.swift`. A pass starts at
the application element and ends at the first mutation. When a read inside the
pass throws an AX error, with any status, the refusal's `axContext` gains
`locus` and `distinctLoci`
(`apps/macos/Sources/BridgeVMControl/AppUIHostDriverProtocol/AppUIDriverAXLocus.swift`):

| Field | Content |
| --- | --- |
| `depth` | AXChildren steps below the main window, which is the first element whose children the pass reads; -1 for the window lookup |
| `indexPath` | the first 32 child indices from the main window down; at depth -1, [] for the application or [i] for its i-th AXWindows entry |
| `visited` | elements whose children the pass had read before the failing call |
| `role`, `subrole` | the element's AXRole and AXSubrole |
| `parentRole` | the AXRole of the element through which the pass reached it |
| `attributeNames` | at most 64 sorted unique names from AXUIElementCopyAttributeNames that match `^AX[A-Za-z]{1,40}$`; empty when unreadable |
| `identifierListed` | whether AXIdentifier is among all of those names, including any past the 64th |
| `distinctLoci` | distinct (depth, index path) pairs that failed over the request's passes, up to 256 |

- **Role fields:** each is an AX constant matching `^AX[A-Za-z]{1,40}$`, or
  `absent` when there is no value or no parent, or `unreadable` when the read
  failed or returned any other text.
- **The locus reads:** only after the failing call, only of the failing element
  (AXRole, AXSubrole, its attribute names) and of its parent (AXRole). They never
  throw out of the refusal and never mutate. The attribute-name message is
  prepared like any other: owner pid, deadline, cancellation and messaging
  timeout.
- **Never read or recorded:** a title, value, description, label, help text, or
  the identifier of any element.
- **No locus:** a press, a search value and the readback after it, and every
  non-AX refusal. Their replies keep their earlier shape.

## Validators

- **Swift:** `AppUIDriverValidation.reply` admits a locus only with
  `distinctLoci` in 1...256, only on a call a pass makes (getPID,
  setMessagingTimeout, copyAttributeValue, copyAttributeValues or
  getAttributeValueCount, with a known attribute), and only well formed: depth
  and index path agree, every bound holds, each role field is an AX constant or
  `absent` or `unreadable`, the names are sorted and unique, and
  `identifierListed` agrees with them and with the 64-name bound.
- **The host's reply decoding:** the host reads a reply through the private
  mailbox's canonical decoding, which refuses unknown or missing keys, and its
  admission runs that validator on every reply.
- **Install-cancel:** `AppUIInstallCancelValidation.reply` refuses any locus or
  distinct count, because its driver records none.
- **Python:** `scripts/live-gates/app_ui_host_v2_diagnostic.py` `verify_driver`
  is unchanged. It reads only the session, ready and completion records. None of
  their keys or bounds changed, and its exact key set still refuses any added
  key. No Python verifier reads d6 replies.
- **Earlier replies:** they stay valid, including the exact request 7 and reply 7
  bytes of run 2.

## Host completion code: not changed

The optional change, recording the driver's `ax-failure` in the host completion
when its overview wait ends on that refusal, was not made. Run 2 shows why it is
not small: the driver's last pass ends at the phase deadline, which is also
where the host stops waiting, and its admission refuses any reply read at or
after that deadline. Recording the driver's code reliably would mean admitting a
reply after the host's own deadline. The driver's reply file carries the code
and the locus either way.

## Tests

The logs stay in the operator's evidence directory
`dev-20261021-evidence/d6-ax-failure-locus`.

| Test | Before | After |
| --- | --- | --- |
| Locus protocol: `tests/integration/app-ui-driver-ax-locus-protocol-contracts.swift`, `tests/integration/app-ui-driver-ax-locus-wire-cases.swift` | do not compile against 6121a878 (`red-c1-locus-protocol-6121a878.log`); with the fields present but a pass-through validator, 66 of 109 checks fail (`red-c1-locus-protocol-seam-behaviour.log`) | 109 pass (`green-c1-driver-contracts.log`) |
| Driver locus: `tests/integration/app-ui-driver-ax-locus-contracts.swift`, `tests/integration/app-ui-driver-ax-locus-sweep-contracts.swift`, `tests/integration/app-ui-driver-ax-locus-names-contracts.swift`, fake `tests/integration/app-ui-driver-ax-locus-fake.swift` | do not compile against a132612e (`red-c2-driver-locus-a132612e.log`); behind seams of the recorder, no locus fails 401, an added title read 381 and a pass that never ends at a mutation 2 of 823 (`red-c2-driver-locus-seam-A.log`, `-B.log`, `-C.log`) | 823 pass (`green-c2-driver-contracts.log`) |

They show:

- **The live shape:** a persistent -25200 on a nested cell's AXIdentifier is
  refused with `depth` 3, `indexPath` [9,0,0], `visited` 12, `role` AXCell,
  `subrole` absent, `parentRole` AXRow, its five AX names,
  `identifierListed` false and `distinctLoci` 1. The 4 restarts and 5 passes
  are the same as without the recorder.
- **Unreadable fields:** an AXRole read that also fails is recorded
  `unreadable`, and the refusal keeps the identifier's -25200. Failing names
  give an empty list, and text that is not an AX constant is never recorded.
- **Distinct loci:** two elements failing across the restarts give
  `distinctLoci` 2, counted by index path, not by failure.
- **Every stage:** the application, a window entry, the main window's children,
  a walk role, the enabled check, a sheet element (through the sheet from the
  main window) and an observation's label read are each placed exactly.
- **Bounds:** a 40-step path keeps `depth` 40 and its first 32 indices. More
  than 64 names keep the first 64, with AXIdentifier past them still listed. The
  distinct count saturates at 256.
- **The failure path:** every pass read of every operation, made to fail, is
  followed only by those role and name reads, with no mutation. Through
  `AppUIDriverAX` with injected messaging, it reads exactly AXRole, AXSubrole and
  the parent's AXRole.
- **No locus outside the pass:** a press, a search value and the readback carry
  none. Without a failure every operation's reads and results are unchanged.
- **Host validity:** every located refusal passes the host's reply validator.

Other checks run on this branch:

- `scripts/check-app-ui-driver-contracts.sh` passed at a132612e, at d37197a0
  and at the final tree. Every existing check keeps its count: 91, 50, 69, 129,
  28, 111, 91, 26 and 95, and the Python suites 5, 10, 15, 4, 8, 18 and 7.
- `scripts/check-app-ui-host-contracts.sh` at a132612e and at the final tree:
  13 of 13 Apple XCTest contracts pass.
- The six queue contracts of the hosted workflow's first step: 11, 19, 8, 15, 4
  and 18 tests pass.
- `scripts/run-xctest-shim-suites.sh`: all four suites pass, with 425, 1462
  (3 skipped), 65 and 436 tests.
- `scripts/check-python-scripts.py`, `scripts/check-shell-scripts.sh`,
  `tests/integration/hvf-reset-record-contract.py`,
  `scripts/check-refactor-budgets.sh` at every commit and `git diff --check`.

The new driver file is on every hand-listed driver AX source list:

- the AX graph list `ax_graph`, which the AX and install-cancel AX compiles
  take and the restart compile now takes too;
- the AX call list, where `AppUIDriverAX` now needs it;
- the new locus compile in `scripts/check-app-ui-driver-ax-locus-contracts.sh`.

The service compile and the production driver build glob it.

Not run: `scripts/check-project.sh`, which the operator excluded, and hosted CI,
because nothing was pushed.

## Not verified

No live run of `d6-app-ui-host-v2` has exercised the locus. Until one does,
these remain unknown:

- which element fails, and whether it lists AXIdentifier
- whether it is always the same element: `distinctLoci` will say
- whether the locus reads themselves succeed on that element in a signed driver
  against the real host; any that fail are recorded `unreadable`
- whether the overview, filtering and clearing phases can complete
