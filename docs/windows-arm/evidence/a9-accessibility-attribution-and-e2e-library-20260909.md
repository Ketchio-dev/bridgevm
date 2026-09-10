# A9: the Accessibility wall was attribution, and the E2E library was not empty (2026-09-09)

## What the seven receipts actually said

Every retained T17 job after the 2026-09-01 pilots -- `t17-5bc84381-v1.1.0-pilot-r1`
through `-r3`, `t17-86c5fd68-v1.1.0-pilot-r1`, `t17-5280ef56-local-pilot-r1`,
`t17-e013d1bc-hosted-pilot-r1`, and today's `t17-e013d1bc-hosted-pilot-r2` --
carries `integration-failed` in its receipt and, in the lane result beneath it,
`failure_code=accessibility-untrusted` with `vm_created=false` and
`ui_frontend_automated=false`. Eight days, four commits, one wall.

The 2026-09-01 next gate was to grant Accessibility to the helper. Today that
grant was given to the exact helper the manifest names
(`dev.bridgevm.product-e2e`, ad-hoc signed, inside the sealed
`e013d1bc...-v1.1.0-run34053212006` artifact) and a fresh job `-r2` was queued.
It failed identically in 41 seconds.

## The same binary, from a shell, passes

The helper was then run by hand from a Terminal shell against the same
verified inputs, with a request built by `make-windows-product-e2e-request.py`
and a lane root inside the enforced `/tmp/bridgevm-e2e-*` boundary. Its lane
result: `artifact_preflight=true`, **`ui_frontend_automated=true`**,
`failure_code=ui-element-missing`. The Accessibility check --
`AXIsProcessTrusted()` in `T17Accessibility.init`, which tests the calling
process -- passed, and the run proceeded into UI automation.

The queue worker is a LaunchAgent whose program is `/usr/bin/env -i ...
bridgevm-live-worker.sh`; the tier then exec'd the helper binary directly. TCC
attributes a request to the responsible process of that chain, not to the
bundle the user trusted. `AXIsProcessTrusted()` reads a cache and leaves no
`tccd` log line, so the unified log could not show this; the shell run did.

The tier now launches the helper through LaunchServices
(`launch-product-e2e-helper.sh`: `open -W -n -a <helper.app> --args ...`), which
makes the helper bundle its own responsible process. This is proven for the
attribution mechanism by the shell run, and by the live-tier contract smoke
driving its fixture helper through the same launcher; it is not yet proven on
a queued job, because the next wall arrived first.

## Past the check: the E2E library had a VM in it

With Accessibility passing, the helper stopped at `ui-element-missing`.
`T17Blocker` is thrown with a detail naming the element, and `T17ProductRunner`
kept only the code -- so the retained result could not say which. The product
app launched standalone with the same `--e2e-library-root` (a fresh, empty,
validated directory) opened with a window titled **`ubuntu-dev`**, and that
directory afterwards contained `ubuntu-dev/`. `LibraryModel` runs
`VMLibrary.migrateLegacyIfNeeded` for whatever root it is given, and the guard
is "library empty"; the E2E root is empty by definition, so the user's legacy
`~/.bridgevm-control/config.json` (an `ubuntu-dev` from June, whose bundle path
no longer exists) was imported into the sealed library. The product journey is
measured against an empty library, and `verifyCreatedVM` compares against
exactly one created VM; this would have failed later even if the first press
had succeeded. The import is now a launch-policy decision: on for the default
library, off for an explicit E2E root.

## What the shell can do that the helper could not, and what it cannot explain

From the same trusted shell, a plain Swift process spawning the app the way the
helper does found `bridgevm.library.toolbar.create` 0.2 s after spawn, saw no
sheet or dialog, found the button enabled, pressed it with `AXError` 0, and
then saw all seven `bridgevm.create.*` identifiers of the create sheet. The
UI path the helper's `createVM` walks is reachable by Accessibility. The helper
still timed out on that first press within 35 s of launch -- consistent with
its 30 s `element()` timeout after a few seconds of `prepareLane` and app
launch. Sandboxing was ruled out (the helper carries no entitlements); a stale
`AXUIElement` created before the app registered was ruled out (an element
created 0.2 s after spawn resolves). The remaining difference between the
helper and a plain process is not visible from outside it, so the helper now
says more: `failure_detail` rides beside `failure_code` in the lane result, and
the element timeout reports the identifier, the app's window count as the
helper sees it, and the timeout. The next run answers this with data.

## Cost recorded, not solved

The helper is ad-hoc signed. TCC keys an ad-hoc bundle's grant by code hash, so
every rebuild is a new identity and the Accessibility grant has to be given
again by hand. A retained clean-machine flow cannot stay green on that basis; a
stable signing identity for the helper is the structural fix and is out of
scope tonight.

## State

A9 stays `OPEN`. No VM creation, Windows installation, guest integration,
shutdown or snapshot/restore was proven today. What changed is that the wall
has a name, a mechanism, a fix in the tier, and instrumentation on the far side
of it.

## Same evening: what the first instrumented runs said

With `failure_detail` in the lane result, the next local runs stopped being
guesses.

- **The LaunchServices launch had its own ambiguity.** A locally packaged
  build from the fixed head was granted Accessibility and run through the
  tier (`open -W -n -a <helper.app>`): `accessibility-untrusted`, with the
  detail "macOS Accessibility permission is not granted". The same helper
  binary exec'd directly from the shell: `ui_frontend_automated=true`. The
  grant was on the right helper; `-a` was not launching it. `lsregister`
  knows 24 registrations for `dev.bridgevm.product-e2e` -- every artifact ever
  unpacked on this Mac -- and with `-a` LaunchServices may pick a registered
  copy over the path it was given. The launcher now opens the bundle URL
  without `-a`.
- **Ad-hoc grants are per code hash, six times over.** `tccutil reset
  Accessibility dev.bridgevm.product-e2e` reported six records reset, one per
  build that had ever been granted. A "+"-added entry for a new build merged
  into the existing row without updating its hash; only reset-then-add took.
- **The press was refused, not the lookup.** Direct exec reached
  `createVM` and failed at `identified UI element does not support press`:
  `element()` found `bridgevm.library.toolbar.create` and
  `AXUIElementPerformAction` returned non-success. A plain process launching
  the same app the same way (`Process`, stdio redirected, valid lane) found the
  app already frontmost and pressed with AXError 0. AXPress on a window that
  is not key is refused, and this Mac runs a terminal manager (`cmux`) that
  re-raised itself over System Settings and Finder throughout the evening.
  The helper now re-fronts the app before a refused press and retries once,
  and the detail reports the frontmost state when it still fails.
- **Earlier experiments that "found no button" were launch failures**: the
  app refuses `--e2e-unattend-path` that is not a regular file beside the
  library root, and with stdio redirected that refusal was invisible.

None of this is a queued pilot yet. A9 stays `OPEN`.

## Codex continuation: the first hosted check exposed a stale policy test

Commit b27feaea retained the activation recovery and bundle-URL launcher,
and now preserves both AXPress error codes without activating the app again
while formatting a failure. Activation remains a recovery attempt; the code
does not establish a general rule that every refused press is a focus error.

The full project check passed before that commit. After the commit, another
full run failed only capability-registry freshness; all its other steps passed.
These are different outcomes, and the earlier pass does not seal the new head.
Hosted Security and quality run 34426219459 also failed: the closure policy
still searched the caller for firstboot readiness after that function moved
to agent-channel-lib.sh. Commit 7a97d6f8 makes the policy check the imported
helper and requires the source connection. All four CR-tolerant guest-output
assertions remain required. The live-gate policy smoke then passed 99 checks.

A11 is OPEN pending a fresh complete check and exact-head hosted CI/Security.
The locally built package passed packaging checks and its exact nested helper
was re-added through System Settings with Accessibility visibly enabled.
That setting is not a queued-process trust receipt. The retained package was
built from b27feaea; 7a97d6f8 changes only the deterministic policy test.
No pilot has been submitted by this continuation, and A9 remains OPEN.

## Ten-hour continuation: r3 passes trust and exposes a modal AX deadlock

After hosted CI 34426736591 and Security 34426736616 both passed at
fe8d6707, queued pilot t17-fe8d6707-local-pilot-r3 ran on Mac17,9 from
2026-09-10 02:05:44 to 02:06:33 UTC and failed. Receipt SHA-256:
9c48f0a9f5de411b459cca8156c21d61ddab910cc1d3801c2349f028ec50a75c.
The private lane reports artifact_preflight=true, ui_frontend_automated=true,
vm_created=false and cleanup_verified=true. The first create control is now
past; the failing control is bridgevm.create.windows.iso. Both AX calls
returned -25204 while activation_succeeded=true and frontmost=true. The SDK
defines -25204 as cannotComplete, not actionUnsupported (-25206).

A separate read-only UI diagnostic launched the same packaged app with an
empty isolated E2E library. Pressing ISO opened the real file chooser. A
one-second sample while it stayed open retained the causal call chain:
NSAccessibilityEntryPointPerformAction -> accessibilityPerformPress ->
CreateVMSheet.pickISO -> chooseFile -> NSSavePanel.runModal. The synchronous
modal loop keeps the original AX request on the stack until the chooser is
closed, while the E2E caller waits for that request before entering a path.
Sample SHA-256:
692bfb00cfad7f392fd8826cd7429b3f96bf0c184bf4b269a4eac8101d936299.
The diagnostic chooser and owned application were closed without creating a
VM or touching canonical media. This observation is not a criterion receipt.

The candidate switches all six CreateVM file/directory pickers to the
asynchronous NSOpenPanel completion API. Four native XCTest cases pass:
deferred selection, cancel preserving the old selection, OK without a URL,
and directory/file filtering. The candidate still needs a new sealed pilot;
neither these tests nor the successful queued Accessibility check closes A9.

## Queued chooser-state pilot r6, 2026-09-10

`t17-f35bac2d-local-pilot-r6` tested
`f35bac2df59d1804f7e69c7015f777c3ccc6d17d` from 03:04:33 to 03:05:28 UTC.
The exact checkpoint's hosted CI `34431457559` and Security `34431457586`
were green before submission. Its private lane result records artifact preflight
and UI entry, but no proven VM creation, and cleanup verified. The failure was
`input-selection-failed`: `file chooser could not be activated`.

This is the mandatory activation guard in the new chooser driver, before its
AX Raise/Go To shortcut. It is not evidence that PID-targeted keyboard delivery
or GoToWindow discovery failed: neither operation was reached. Read-only
observation while the owned product process was alive found an Open panel at
Documents with Open disabled; no manual input was injected into the pilot.

The public receipt SHA-256 is
`85a32248f7d4bbc4a95c5541777164f5580394970fcdc7888bda8e1adaeb05d6`;
the input manifest SHA-256 is
`b0736d1b6922215962a9a01d2ab23fe30d2a3d1b112c277bf21738f74039350e`.
The failed receipt remains retained. No Windows installation, full product
journey, clean-machine flow or signed 3D policy provenance is proven by r6.
A9 remains OPEN and product 3D injection remains unavailable.
