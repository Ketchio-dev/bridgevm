# HVF session heartbeat republish (display hypothesis D3) — 2026-10-22

Classification: an app-side fix for hypothesis D3 from the 2026-10-22
read-only review of the 3D-off display stutter. The evidence is automated
tests under the XCTest shim and static reasoning. No live run has exercised
the change. **Whether it changes the visible stutter is not measured.** A9's
`known_defect` (the app's live display window was choppy with 3D off) stays
OPEN, and the product remains ENGINEERING_PREVIEW.

## Mechanism

- `HvfEngineSession` is the `ObservableObject` behind the runtime dashboard,
  the retained runtime view, the library runtime controls and the live display
  window. By SwiftUI's observation rules, which were read but not observed in a
  running app, each `objectWillChange` re-evaluates `HvfLiveDisplaySurface`
  and runs `HvfFramebufferView.updateNSView` on the main thread. That thread
  also runs the display link's frame refresh.
- `poll()` runs from a 100 ms timer, and every 10 ms while the ordered-input
  pump holds queued input. After the first guest heartbeat, every poll
  reassigned the `@Published` heartbeat age, so every poll invalidated every
  session observer. Only two texts show that age, at 0.1 s precision.
- Owned runtimes had a second source. Every controller tick calls
  `ownedRuntimeChanged`, which reassigned an unchanged `.stopping` while a stop
  was pending (up to 180 s guest grace plus 12 s observation). After an
  unconfirmed stop it reassigned an unchanged `.timedOut` until teardown was
  confirmed. For a retained runtime, kept after its VM left the library list,
  the retained-control store also republished the library model on every such
  tick through `$connectionState`.

## Fix

| Commit | Change |
| --- | --- |
| `496ba125` | The age moved to `HvfRuntimeHeartbeat` in `apps/macos/Sources/BridgeVMControl/HvfEngine/HvfRuntimeHeartbeat.swift`. Only the status card's heartbeat item and the retained runtime caption observe it. Both keep their text, the `bridgevm.windows.runtime.heartbeat` identifier and value, and their per-poll updates. No UI driver reads that text. |
| `dd47e1bc` | `ownedRuntimeChanged` assigns `.stopping` or `.timedOut` only when the state differs, with the same precedence. |

- The start paths keep their explicit publishes unchanged.
- Each agent event still republishes the session once, because it is a new
  event-feed entry. That includes the heartbeat line, which arrives every 30 s.
- `FBLayerView.refreshFrame` re-checks the text-input target on every
  display-link frame. While the window is shown, a stale IME composition is
  therefore still cancelled without these SwiftUI updates.

## Tests

Counts from 20 polls under the XCTest shim, before → after:

| Scenario | Session `objectWillChange` | `$connectionState` | `HvfRuntimeHeartbeat` |
| --- | --- | --- | --- |
| Heartbeat only (`HvfEngineSessionPublishTests`) | 20 → 0 | 0 → 0 | 20 (new object) |
| Owned stop pending (`HvfEngineSessionOwnedTickPublishTests`) | 20 → 0 | 20 → 0 | 0 |
| Owned stop unconfirmed (same test) | 20 → 0 | 20 → 0 | 0 |

- Attach, READY, a heartbeat line and stop still publish the session. So do
  an owned stop request and an unconfirmed stop. A stop request now publishes
  once instead of 3 times.
- The whole BridgeVMControl shim suite passed at `dd47e1bc`:
  1451 passed, 0 failed, and 3 skipped (opt-in, release-only and live-fixture
  tests).

## Not measured

- What the app window shows. Frame pacing, dropped frames and main-thread time
  were not measured, and no live run used this change. An earlier hardware run
  measured only the export side
  ([r52](a9-r52-display-export-tail-20260930.md)).
- No automated check renders the two heartbeat views. The only app-render
  test is an opt-in WindowServer diagnostic, and it was skipped.
- The review's other display hypotheses are untouched and untested. They are
  that the 33 ms exporter, the 30 ms pointer reports and the 60/120 Hz refresh
  are unsynchronised, and that each frame costs a main-thread copy and a
  CPU-backed image.
