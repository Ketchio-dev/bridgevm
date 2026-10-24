# d6 run 3 names the failing element: an AXMenuButton; the VM actions menus get an identifier — 2026-10-23

Classification: one live single diagnostic run (evidence rank 2), a hypothesis
from static reasoning (rank 4), and deterministic tests (rank 3) of the change
made in response. It is not criterion evidence and changes no product wording.
No live run has exercised the change.

## Live run 3 (rank 2)

Job `bvm-22678cd5-native-ui-v2-20261023-03`, tier `d6-app-ui-host-v2`, source
`22678cd56eec076ddfdfa508b79166c7e4e4664c` (the seal that adds the failure locus
of [the locus record](d6-ax-failure-locus-20261023.md)), on the physical-Mac
live queue. Submitted 18:53:41, ran 18:53:59 to 18:54:15 UTC.

- **Receipt:** `receipt.json` and the public copy are identical, SHA-256
  beginning `92e71d13`. `outcome=diagnostic-incomplete`,
  `failure_code=observation-refused`, `worker_cleanup_verified=true`,
  `claim_eligible=false`. Manifest SHA-256 beginning `fb2cc369`; only the host
  binary and the launcher differ from run 2's inputs.
- **Console:** unlocked in every sample. Load before submit 4.06 / 4.66 / 3.98.
  Another tool's Virtualization.framework VM process was using about 14% of a
  core, and the operator's Aside window was open, as in runs 1 and 2.

| Seq | Request | Driver reply |
| --- | --- | --- |
| 1 to 6 | welcomeControls to importForm | as in runs 1 and 2 |
| 7 | pressOverview (phase `overview`) | refused: `ax-failure`, `axError` -25200, `copyAttributeValue` / `AXIdentifier` / `element`, with the locus below |

| Locus field | Value |
| --- | --- |
| `role`, `subrole` | `AXMenuButton`, `absent` |
| `parentRole` | `AXOpaqueProviderGroup` |
| `depth`, `indexPath` | 6, [0, 0, 2, 5, 0, 3] |
| `visited` | 68 |
| `identifierListed` | true, among 30 AX attribute names |
| `distinctLoci` | 1 |

- **Driver completion:** `failureCode=ax-failure`, 7 requests, 3 mutations,
  `axRestarts=110`. **Host completion:** `deadlineExceeded`, cleanup verified;
  every tripwire 0; `model_creations` 0 before seeding, 2 after, 2 seeded.
- **Timing,** from file modification times: request 7 53 ms after reply 6,
  reply 7 5.00 s after request 7, at the phase deadline.

What this shows: in all 111 passes the first `AXIdentifier` read that failed
was on the same element, an `AXMenuButton` that lists `AXIdentifier` among its
attributes. Its role, subrole, its parent's role and its attribute names were
all readable in the signed driver; only the identifier read fails. A pass stops
at its first failure, so elements after it in the walk are unobserved.

## Which element (rank 4, not verified)

- The app declares exactly two SwiftUI `Menu`s: the actions menu beside each
  VM's workspace row (`LibraryWorkspaceList.swift`) and beside each overview
  card (`LibraryVMCard.swift`). SwiftUI exposes a `Menu` as `AXMenuButton`; the
  create sheet's pickers are pop-up buttons.
- Both exist only once the library holds VMs. Request 6, before seeding, read
  every element's identifier; request 7, after seeding, never got past this
  one.
- Neither menu set an accessibility identifier.
- **Hypothesis:** SwiftUI answers `AXIdentifier` on this menu with
  kAXErrorFailure when no identifier is set. That the element is the row menu
  rather than the card menu, and that an identifier changes the answer, are
  both unverified.

## Change

`apps/macos/Sources/BridgeVMControl/LibraryVMActionsMenu.swift` is the one
actions menu, shown by both the row and the card. Its label, menu style,
indicator, size and items are unchanged, and the card keeps its help text. It
adds `.accessibilityIdentifier("bridgevm.library.vm-actions")` on the `Menu`
itself. The identifier is the same for every VM, carries no VM name, and no
driver or host step matches on it. Moving both menus into it lowers the
ceilings of `LibraryWorkspaceList.swift` (160 to 154) and `LibraryVMCard.swift`
(86 to 80).

## Tests

The logs stay in the operator's evidence directory
`dev-20261021-evidence/d6-menu-identifier`.

| Test | Before | After |
| --- | --- | --- |
| `apps/macos/Tests/BridgeVMControlTests/LibraryMenuIdentifierSourceTests.swift`: every `Menu` the app declares is in the shared file, which sets the identifier, and the row and card each show it once | fails at 22678cd5: both bare menus are found (`red-shim-suites-22678cd5-plus-source-scan.log`) | passes |
| `apps/macos/Tests/BridgeVMControlTests/LibraryVMActionsMenuValueTests.swift`: through the real view values, one accessibility attachment carries the identifier and wraps the `Menu` itself; the card shows the shared menu and no bare one | does not compile at 22678cd5 (no shared menu) | passes |

Both run in the BridgeVMControl shim suite, which takes its test files by
directory. No hand-listed source set names the changed files.

## Not verified

- whether an identifier makes this element's `AXIdentifier` read succeed in
  the signed app: only a live d6 run can show it
- whether the element is the row menu or the card menu
- whether the overview, filtering and clearing phases then complete
