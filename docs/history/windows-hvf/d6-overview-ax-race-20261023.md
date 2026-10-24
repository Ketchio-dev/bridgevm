# d6 overview AX refusal at main 5de1f8ca — 2026-10-23

Classification: one live single diagnostic run (evidence rank 2) with a cause
that is static reasoning only (rank 4), plus deterministic tests (rank 3) of
the change made in response. It is not criterion evidence and changes no
product wording. No live run has exercised the change.

Branch `agent/d6-overview-settle-20261023` from `5de1f8ca`:

- `819ddff6`: the driver restarts its read-only AX pass after a transient AX
  status, within the request deadline.
- `feef2dd3`: the driver completion records those restarts as `axRestarts`.
- `a18b5313`: the host requests the overview press only once the library model
  holds both seeded rows.

Only `d6-app-ui-host-v2` changes. `d6-app-ui-host-v1`, `d6-app-ui` and the
install-cancel driver are unchanged; the install-cancel driver stays
single-shot.

## Live run

Job `bvm-5de1f8ca-native-ui-v2-20261023-01`, tier `d6-app-ui-host-v2`,
source `5de1f8cac4664225f71babdb7bc9d67dbfd83df9` (the merge of #427), on the
physical-Mac live queue. Submitted 15:31:20, ran 15:31:29 to 15:31:42 UTC.

- **Receipt:** `receipt.json` and the public copy are identical, SHA-256
  beginning `69070924`. `outcome=diagnostic-incomplete`,
  `failure_code=observation-refused`, `worker_cleanup_verified=true`,
  `claim_eligible=false`.
- **Inputs:** manifest SHA-256 beginning `a1aaad04`, host executable
  `dcbf255b`, driver launcher `f1a1f516`.
- **Console:** unlocked in all three samples, no Screen Sharing.

| Seq | Request | Driver reply |
| --- | --- | --- |
| 1 | welcomeControls | observed: create and import visible |
| 2 | pressCreate | performed |
| 3 | createForm | observed: commit visible, all three selections empty |
| 4 | cancelCreate | performed |
| 5 | pressImport | performed |
| 6 | importForm | observed: name visible |
| 7 | pressOverview (phase `overview`) | refused: `ax-failure`, `axError` -25200 (kAXErrorFailure), `axContext` `copyAttributeValue` / `AXIdentifier` / `element` |

- **Host:** its completion records `axFailure` with cleanup verified. The
  report marks every action through `import_opened` and not
  `overview_opened`. All four tripwires are 0 and `model_creations` is 0
  before seeding, 2 after and 2 seeded, the exact counts the overview re-spec
  requires. Six screenshots were captured.
- **Launcher:** both applications exited and both identities were verified;
  the scenario was not complete.
- **Timing,** from file modification times only: reply 6 at t, request 7 at
  t + 58 ms, the refusal at t + 140 ms, 82 ms after request 7.
- At 97de386c the driver processed six of seven requests, because the host
  stopped itself on the `model_creations` tripwire at the overview phase (see
  [the overview re-spec](d6-overview-respec-20261022.md)). Here the driver
  answered request 7.

The files are kept outside the repository, in the operator's evidence
directory `dev-20261021-evidence/live-20261023/d6/result`.

## Hypothesis: static reasoning, not proven

- Between reply 6 and request 7 the host captured the import form and called
  `seedOverviewMetadata()`. That call adds the Amber and Indigo fixture VMs to
  the library model, which renders two new rows in the library's workspace
  list, and the host sent `pressOverview` immediately after it.
- The driver walks the whole main window and reads `AXIdentifier` of every
  node. Its AX wrapper tolerates only no-value and attribute-unsupported, and
  the service ended the whole session on any other status.
- The walk may therefore have raced SwiftUI's re-render of the library. The
  same race could recur at `overviewCards` right after the press, and at
  `searchState` after `setSearchIndigo` removes a row.
- Nothing establishes this. It is not known which element failed, whether the
  status was transient, or whether a second walk would have succeeded. There
  is no AX trace of the failing element, and the failure was not reproduced.

## What changed

The assignment specified all three changes.

### Driver: restart the read-only pass (`819ddff6`)

`apps/macos/scripts/AppUIDriverAXRestart.swift` runs each request's read-only
pass, which `apps/macos/scripts/AppUIDriverAXOperations.swift` now separates
from the mutation:

- **The pass:** the window lookup, the walk and every read that locates or
  observes an element, including the role and enabled checks before a press.
- **Transient statuses:** exactly -25200 (kAXErrorFailure), -25202
  (kAXErrorInvalidUIElement) and -25204 (kAXErrorCannotComplete) restart it
  from the window lookup.
- **Pacing:** each restart follows a yield of at least 25 ms on the driver's
  main run loop. A run loop with no source to run returns at once, so the
  rest of that yield is slept. Cancellation is checked before and after
  every yield.
- **Deadline:** restarts continue only while the request's existing deadline,
  min(session, phase), leaves room for the yield. Nothing is longer and no
  new deadline exists; a 5 s phase admits at most 200 restarts.
- **Single-shot:** the press, the search value and the readback after it run
  once, outside any restart. A mutation's own AX error is terminal and is never
  replayed.
- **Terminal:** every other AX status and every non-AX refusal, such as an
  absent or ambiguous element or cancellation, stays terminal at once.
- **Persistent status:** when no restart fits, the refusal carries the last
  transient status and its call context, even if the deadline arrived in the
  middle of a restarted pass. A persistent failure therefore reads as the
  5de1f8ca refusal did, never as `deadline-exceeded`.

### Restart count (`feef2dd3`)

`apps/macos/scripts/AppUIDriverService.swift` adds every request's restarts,
on success and on refusal, to a session total. It publishes the total in
`driver-completion.json` as `axRestarts`. One restart per 25 ms of the 90 s
session bounds it at 3,600.

- `AppUIDriverValidation.completion` requires the key as an exact integer in
  0...3600, on publication and on the supervisor's read. The canonical decoding
  already refuses a missing or unknown key.
- The tier verifier `scripts/live-gates/app_ui_host_v2_diagnostic.py` requires
  it among the completion's exact keys.
- Refusal replies keep their shape.
- Old completion files without the key are refused by the new readers. No
  reader re-validates archived evidence: the supervisor reads its own session
  and the verifier reads only the job it just ran, both from one sealed
  commit. `schemaVersion` therefore stays 1.
- With the key, d6 completion bytes no longer decode as an install-cancel
  completion. The existing contract for that still refuses, now at decoding,
  so the new contracts exercise the install validator's kind check directly.

### Host: press only after seeding (`a18b5313`)

`apps/macos/Sources/BridgeVMControl/AppUIHostOverviewRequests.swift` runs the
overview phase's requests. It seeds, then waits under the overview deadline
until `library.vms` holds `ui-amber` and `ui-indigo` exactly once each, and
only then requests `pressOverview` and the `overviewCards` observation. If the
rows are not there in time, the phase is refused without any request. The
operations, their order, the request count and the d6 invariants are
unchanged.

`seedOverviewMetadata()` assigns the model synchronously, so in production the
wait passes on its first check and cannot be observed failing. It is kept as a
guard, and its test uses a deferred seeding to stand in for a late model.

## Tests

The logs stay in the operator's evidence directory
`dev-20261021-evidence/d6-overview-settle`.

| Test | Before | After |
| --- | --- | --- |
| AX restart contracts: `tests/integration/app-ui-driver-ax-restart-contracts.swift`, `tests/integration/app-ui-driver-ax-restart-terminal-contracts.swift`, `tests/integration/app-ui-driver-ax-transient-access.swift` | do not compile against 5de1f8ca (`red-c1-pristine-5de1f8ca.log`); behind a pass-through seam, 78 of 111 checks fail (`red-c1-seam-5de1f8ca-behaviour.log`) | 111 pass (`green-c1-ax-restart.log`) |
| Restart count, Swift: `tests/integration/app-ui-driver-ax-restart-count-contracts.swift` | does not compile against 819ddff6 (`red-c2-swift-restart-count.log`) | 129 checks in the restart binary pass (`green-c2-swift-restart-count.log`) |
| Restart count, Python: `tests/integration/app-ui-host-v2-restart-count-contract.py` | 1 failure and 5 errors against 819ddff6 (`red-c2-python-restart-count.log`) | 4 tests pass (`green-c2-python-restart-count.log`) |
| `AppUIHostOverviewRequestsContractTests` (Apple XCTest) | with the 5de1f8ca order, 5 failures: deferred seeding pressed before the rows existed, and no rows or one row still pressed (`red-c3-host-overview-order.log`) | 2 tests pass (`green-c3-host-overview-order.log`) |

The pass-through seam keeps the 5de1f8ca operations and adds only the restart
property, which those operations never use. Against it, the 33 checks that need
no restart already pass:

- A persistent status is refused with its own status and context, and nothing
  is pressed.
- Other statuses are refused at once.
- A failing press, search value or readback is attempted once.

Those properties were preserved, not added.

Tests (a) to (f) of the assignment map onto these as follows:

- **(a), (b):** each transient status, once, on `AXIdentifier`, `AXChildren`,
  `AXRole` or `AXEnabled` before the press, gives exactly one overview press
  and a restart count of 1.
- **(c):** a persistent -25200 until the injected deadline is refused
  `ax-failure` -25200 with the `copyAttributeValue` / `AXIdentifier` context,
  with no press.
- **(d):** -25211, -25201, -25203, -25206 and -25208 are refused at once with
  no restart.
- **(e):** a press, a search value or a readback that returns a transient
  status is refused and never attempted twice.
- **(f):** the scripted client never sees `pressOverview` before the model
  holds both seeded rows.

Other checks run on this branch:

- `scripts/check-app-ui-driver-contracts.sh` passed for each driver commit.
  Every existing check keeps its count, compared with a run from a git archive
  of 5de1f8ca. There only the packaging test could not run, because the
  archive has no git metadata.
- The Apple XCTest host contracts of `scripts/check-app-ui-host-contracts.sh`
  at a18b5313: 13 of 13 pass with the exact inventory. They ran from an
  evidence copy that differs only in `-j 3` and an explicit repository root.
- `scripts/run-xctest-shim-suites.sh` at a18b5313: all four suites pass, with
  425, 1462 (3 skipped), 65 and 407 tests. The shim builds without
  `BRIDGEVM_APP_UI_HOST` and has no host suite, so it compiles the overview
  requests out and does not run their contract.
- `scripts/check-refactor-budgets.sh` at every commit,
  `scripts/check-python-scripts.py`, `scripts/check-shell-scripts.sh`,
  `tests/integration/hvf-reset-record-contract.py` and `git diff --check`.

`apps/macos/scripts/AppUIDriverAXRestart.swift` is on every hand-listed
driver source list. The AX graph compile uses it, as does the new restart
compile, which that script sources. The install-cancel AX compile and the two
AX call compiles list it without using it. The service compile and the
production driver build glob it.

Not run: `scripts/check-project.sh`, which the operator excluded, and hosted
CI, because nothing was pushed.

## Not verified

No live run of `d6-app-ui-host-v2` has exercised any of these commits. Until
one does, these remain unknown:

- whether the -25200 was a transient re-render race at all
- whether a restarted walk succeeds in a real window, and how many restarts
  the overview, filtering and clearing phases need
- whether those three phases complete, and whether both rows are on screen
  with these labels
- how the driver's main run loop behaves during the yield in the signed
  driver; the yield was measured only in a command-line test process
- which refusal the host records in a persistent failure: the driver's last
  pass can end at the phase deadline, which is also when the host stops
  waiting for the reply, so the host may record a deadline refusal while the
  driver's reply records `ax-failure`; no test covers that interleaving
