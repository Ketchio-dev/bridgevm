# Dashboard-owned HVF runtime lifecycle

## Observed split

The installed-Windows dashboard previously used three different lifecycle
boundaries. A stopped VM called `ControlModel.start()`, which launched the
historical shell backend. A running VM resolved and attached an
`HvfEngineSession` before opening its display. The stop button returned to
`ControlModel.stop()`, whose legacy backend locates processes from the disk
path. The visible controls therefore did not share one runtime owner.

## Deterministic repair

Source `40f214990852c36ad3eb82bf5bf91e0aa0a3f5c5` routes an installed HVF VM's
dashboard controls through one retained `HvfEngineSession`:

- Start rebuilds `HvfEngineConfig` from the current saved library record and
  submits it through `requestGUIStart`. An accepted operation opens the display
  immediately; a refusal is surfaced without pretending that a launch began.
- Open preserves the existing attach-or-present path. A retained active session
  is presented directly, while an observed running process must first be
  attached successfully.
- Stop requires the exact `HvfOwnedRuntimeIdentity` retained by that session and
  calls `stopOwned(expectedToken:)`. An attached external or historical runtime
  is not terminated by a disk-path process scan.
- The action strip observes session state, disables conflicting controls during
  startup or shutdown, shows pending work and launch failures, and refreshes the
  legacy status observer after the typed start worker completes.

Non-HVF controls retain their existing `ControlModel` path. Invalid HVF launch
configuration remains fail-closed and now reports the missing session or saved
launch configuration instead of silently falling back to a different engine.

## Verification and limits

Seven focused XCTest cases cover typed start acceptance, start refusal,
attachment before presentation, retained-session presentation, missing and
unattachable sessions, exact owned-stop targeting, and refusal to stop an
external attachment. Structural budgets pass with each new file registered at
its actual size and no existing ceiling raised.

The complete deterministic project check passed on the source tree. A later
metadata-inclusive attempt correctly failed only the stale capability identity
before this record and was interrupted after that known failure. Its retained
log SHA-256 is `5f4279342459f4902f92b87fa23cd0dadf9f124e4078c418f8693000e5b761e6`. Its four
Swift shim suites reported 425 BridgeVMApp tests, 830 BridgeVMControl tests with
the two required live-only skips, 62 AppleVzRunnerCore tests, and 109
BridgeVMProductE2E tests. Rust workspace and own-HVF suites, release override
checks, packaged entitlement checks, and product E2E contracts also passed.
The retained 7,656-line seal log has SHA-256 `bb6d0b93ec81df88c2dfd10ced717e1faed6528ffa3822ffbf8bbcc05cd0778e`.

This checkpoint proves deterministic dashboard routing and ownership behavior.
It does not prove guest boot, display frames, guest shutdown, or a live installed
application journey. The user's currently running VM was not stopped or changed.
A9, A11, A14 and A19 remain OPEN, product state is unchanged, and experimental
3D remains outside the release path.
