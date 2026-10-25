# d6 run 4 reaches filtering; the library search field's own element now reports a value set — 2026-10-23

Classification: one live single diagnostic run (evidence rank 2), in-process
observations of AppKit and SwiftUI on the operator's Mac and automated tests
(rank 3), and one remaining hypothesis (rank 4). It is not criterion evidence
and changes no product wording. No live run has exercised the change.

## Live run 4 (rank 2)

Job `bvm-8d3eb052-native-ui-v2-20261023-04`, tier `d6-app-ui-host-v2`, source
`8d3eb052aa3e8016e81d5bc6be9e8fdaed6a60e3` (the seal with the actions menu
identifier of [the menu record](d6-menu-identifier-20261023.md)). Submitted
19:41:49, ran 19:41:55 to 19:42:12 UTC.

- **Receipt:** both copies identical, SHA-256 beginning `11870c80`;
  `outcome=diagnostic-incomplete`, `failure_code=observation-refused`,
  `worker_cleanup_verified=true`, `claim_eligible=false`. Manifest SHA-256
  beginning `db19503b`. The unsigned launcher build was identical to run 3's.
- **Console:** unlocked in every sample. Load 5.32 / 4.82 / 3.95; another
  tool's Virtualization.framework VM used about a quarter of a core; the
  operator's Aside window was open.

| Seq | Request | Driver reply |
| --- | --- | --- |
| 1 to 6 | welcomeControls to importForm | as in runs 1 to 3 |
| 7 | pressOverview | performed |
| 8 | overviewCards | observed, Amber and Indigo visible |
| 9 | setSearchIndigo | performed, read back "Indigo" |
| 10 to 38 | searchState | observed: Amber and Indigo visible, search value "Indigo" |
| 39 | searchState | refused `deadline-exceeded` |

- **Driver completion:** `failureCode=deadline-exceeded`, 39 requests, 5
  mutations, `axRestarts=0`. **Host completion:** `deadlineExceeded`;
  `overview_opened` true, `search_filtered` false; every tripwire 0;
  `model_creations` 0 before seeding, 2 after, 2 seeded.
- **Timing:** polls 54 to 100 ms apart; request 10 to reply 38 took 4.71 s.
- **The host's overview capture** shows the workspace list with an empty
  search field and both rows with their actions menus, and no overview cards.
  Run 3's `AXMenuButton` was therefore a row's actions menu, and with its
  identifier no accessibility read failed in this run.

What this shows: the overview press, the card observation and the search set
now succeed. The set changed the field's accessibility value but, for 4.7 s,
neither the list nor the clear button, which both follow the query binding.

## Why the query did not change

In-process observations (rank 3), from a standalone probe kept outside the
repository (operator evidence directory `dev-20261021-evidence/d6-search-binding/probe`),
on macOS 26.7:

- SwiftUI's `TextField` is backed by an `AppKitTextField` whose delegate, a
  `PlatformTextFieldCoordinator`, updates the binding from
  `controlTextDidChange`. Setting the text alone does not update the binding;
  `FirstRunOptionalRecoveryFixture` calls that delegate for the same reason.
- An AppKit text field is accessible through its cell: the control is not an
  element (role `AXUnknown`), and the cell is (`AXTextField`). A cell's
  `setAccessibilityValue` changes the text and posts no change notification.
- The new test reproduces the live symptom at that level: at `8d3eb052` a
  value set on the field's element leaves the query empty while the field
  keeps showing "Indigo".
- Hypothesis (rank 4): the remote set reached the text the same way. SwiftUI's
  own node, which carries the identifier for a remote client, is not reachable
  in-process, and granting this shell accessibility (TCC) is excluded.

## Change

- `apps/macos/Sources/BridgeVMControl/LibrarySearchTextField.swift` (new): an
  `NSViewRepresentable` whose field's cell is the accessible element and the
  only one carrying `bridgevm.library.search`, so a walk finds it once. A value
  set through either entry point (`setAccessibilityValue`, or the older
  `accessibilitySetValue(_:forAttribute:)`) becomes the field's text and is
  reported to the query; a set on the control is passed to the cell. Typing
  still goes through `controlTextDidChange`.
- Kept: the placeholder, the plain look (no border, bezel, background or focus
  ring), the tooltip and the hidden ⌘F shortcut, which now asks the field to
  become first responder.
- `LibrarySearchField.swift` drops from 36 to 32 lines; its ceiling is
  lowered.

## Tests

`apps/macos/Tests/BridgeVMControlTests/LibrarySearchFieldAccessibilityTests.swift`
hosts the real `LibrarySearchField` in a window that is never shown. The logs
stay in `dev-20261021-evidence/d6-search-binding`.

| Check | At 8d3eb052 | After |
| --- | --- | --- |
| the element is an `AXTextField` and carries the identifier, the control does not | identifier missing | passes |
| a value set on the element reaches the query | query stays empty | passes |
| emptying the query, as the clear button does, empties the field | field keeps "Indigo" | passes |
| a set on the control reaches the query | query stays empty | passes |
| typing reaches the query | passes | passes |

Red: `red-shim-suites-8d3eb052-plus-search-test.log` (BridgeVMControl 1464
passed, 1 failed). Green: `green-shim-suites.log`.

## Not verified

- whether a remote client's set arrives at the overridden entry points, as it
  does in-process: only a live d6 run can show it
- whether the filtering and clearing phases then complete
- the field's look beside SwiftUI's plain field, beyond the host's captures
- the ⌘F focus in a live session
