# A9 pilots at main 97de386c — 2026-10-22

Classification: physical pilots, each a live single run. A pilot is never
criterion evidence: every receipt keeps `claim_eligible=false` and
`criterion_pass=false`. None ran on a clean machine. A9 remains OPEN and the
product remains ENGINEERING_PREVIEW. The operator approved these runs on
2026-10-22.

All runs used the Mac Studio (Mac16,9, macOS 26.7) and the live queue on the
`BridgeVMLive24h` image. That queue's installed worker control code is
identical to main's `scripts/live-gates`. Apps were development-signed,
exact-source builds. A console sampler recorded the following every 30 s:

- the lock state, through the repository parser;
- the HID idle time;
- the frontmost app;
- Screen Sharing.

## Summary

| Job | Source | Window (UTC) | Result |
| --- | --- | --- | --- |
| `t17-97de386c-main-pilot-r82` | `97de386c` | 04:13:59–04:15:55 | FAIL, `ui-element-missing` |
| `t17-97de386c-main-pilot-r83` | `97de386c` | 04:25:04–04:46:28 | FAIL, `integration-failed`; confounded |
| `t17-97de386c-main-pilot-r84` | `97de386c` | 04:51:36–05:38:04 | **PASS** |
| `t19-97de386c-main-import-pilot-r1` | `97de386c` | 05:42:13–06:06:14 | FAIL, `integration-failed` |
| `t19-26bfec47-import-dashboard-pilot-r2` | `26bfec47` | 06:18:42–06:41:28 | FAIL, `integration-failed` |

## T17 r82: install view not shown

- **Inputs:** the private rows name their canonical paths. `~/BridgeVM` has
  been a symlink to the external disk since 2026-10-07, after the last Studio
  pilot. The manifest SHA-256 begins `53663c86`.
- **Progress:** all three file choosers passed. After the create commit, the
  helper waited 60 s for `bridgevm.windows.install.view`. It did not appear,
  and `vm_created=false`.
- **Suspected cause (not proven):** `WindowsHVFProductPolicy.stageISO` clones
  or copies the ISO into the bundle, then hashes it, before the VM is
  published. Here the ISO was on the external disk and the lane on the queue
  image, so this was a full copy.
  - Copying the same 7,994,415,104-byte image between those two volumes later
    took 54.6 s.
  - The image's backing file grew to 8.19 GiB allocated.
  - The tier's normal cleanup removed the app log, so the timing inside the app
    was not observed.
- **Confounder:** the frontmost window at submission was the crash dialog
  left by the preceding native UI diagnostic. See
  [that record](app-ui-417-live-diagnostic-20261022.md).

## T17 r83: first READY, then a click lost to a stale dialog

- **Inputs:** the ISO was first copied onto the queue volume, the same device
  as the lane. Its SHA-256 matches the sealed row. The manifest SHA-256 begins
  `0d379eca`.
- **Progress:** VM creation, installer source, Windows installation, Secure
  Boot provisioning and first READY passed. The run log shows INPUTCAPS
  negotiated (`TEXTINPUT KEYINPUT POINTERINPUT`), two `POINTERINPUT`
  commands, and no legacy pointer.
- **Failure:** the lane failed with "display click was not inserted as guest
  pointer input; display=presses=1 last=queued". The final frame shows the
  challenge screen with the guest pointer at about (0.04, 0.51).
- **Confounder:** at 04:45:08.400 UTC, UserNotificationCenter handled a
  mouse-up on the stale crash dialog. Problem Reporter then opened.
- **Validity:** the run is invalid as a measurement of the click path.
  Problem Reporter was closed afterwards with SIGTERM, and no permission was
  changed.

## T17 r84: pass

- **Inputs:** the same manifest as r83. Before submission, the on-screen
  window list held only Terminal and an unrelated full-screen overlay that
  had been running since 2026-10-07.
- **Receipts:** `receipt.json` and the public copy are identical, SHA-256
  beginning `08d260e6`. The exact-source strict verifier passes both.
  Fields: `outcome=completed`, `pass=true`, `passes=1`,
  `campaign_mode=pilot`, `elapsed_ms=2779000`.
- **Lane result:** every journey stage is true, from artifact preflight
  through second shutdown, including the snapshot restore.
- **Console:** unlocked in all 94 samples. The product app was frontmost from
  04:51:55 to 05:29:39, its whole life. Terminal was in front only before
  launch and during the tier's own finalization.
- **T19 handoff:** `verified=true`. The selected disk, vars and vTPM tree
  were retained on the queue volume with a T19 manifest. In r81 the handoff
  had refused.

## T19 r1: waited for the runtime view behind the dashboard

- **Inputs:** the first T19 run on hardware. It imported the r84 handoff with
  the r82 app build. The manifest SHA-256 begins `a6768978`.
- **Progress:** source authentication passed, and the import form was
  driven through its choosers.
- **Failure:** the helper then waited the full 1,200 s for
  `bridgevm.windows.runtime.view` and failed `ui-element-missing`. The last
  retried detail was an `AXIdentifier` read of -25200 on an `AXMenuButton`.
  `ui_imported=false`.
- **Source check:** a re-verification of the handoff manifest afterwards
  matched every source hash.
- **First reading (static):** since the three-pane dashboard, a committed
  import selects the VM and shows its dashboard. The runtime view sits behind
  `bridgevm.dashboard.advanced` (고급).
  - T17 learned to press it after install in `d34415a5` (2026-09-20).
  - `A9ImportProductRunner` (2026-09-16) still waited for the runtime view
    directly.
  - T19 r2 below shows that this was not the whole cause.

## Navigation change and T19 r2

`26bfec47` adds `A9ImportRuntimeNavigation`. It waits up to the same 1,200 s
for the dashboard control, presses it unless the runtime view is already
shown, and then requires the runtime view within 60 s.

- A fixture that models the imported dashboard failed one of three tests
  with the old wait, and passes with the change.
- 366 focused A9Import/T17 product E2E tests pass.

T19 r2 ran this source against the same retained handoff, with a fresh app
build. Its manifest SHA-256 begins `8277a67f`.

- **Receipts:** identical, SHA-256 beginning `18c0e896`. Cleanup was
  verified.
- **Console:** unlocked throughout. The product app was frontmost for its
  whole life.
- **Failure:** the dashboard control did not appear within 1,200 s either:
  "required accessibility identifier was not found:
  bridgevm.dashboard.advanced". The AX diagnostic found one window, no read
  errors, and none of the known runtime or dashboard identifiers in its
  first 128 nodes. No `hvf-runner` process ran.
- **What r1 and r2 show:** in both runs the app did keychain work 1.5 to
  2 minutes after launch, which fits the vTPM recovery step of the import.
  Neither run reached the dashboard or the runtime view.
- **Conclusion:** the import never committed, or it stopped on an error or
  recovery state that the helper cannot see. The cause is unknown.
  - Product code selects the VM and shows the dashboard only after a
    committed import.
  - The import error text has no accessibility identifier, and the failed
    lane's app log is removed by the tier's normal cleanup.

The navigation change stays, because it matches the product and T17's
hardware-proven flow. It is not live-validated, and T19 still fails before
that point. The next step is diagnostic: expose the import stage and error
state to the helper, or retain the failed lane's app log privately, before
another T19 run.
