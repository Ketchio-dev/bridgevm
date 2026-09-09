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
