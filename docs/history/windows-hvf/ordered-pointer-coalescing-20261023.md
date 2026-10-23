# Ordered-route pointer move coalescing (display hypothesis D4) — 2026-10-23

Classification: an app-side fix for hypothesis D4 and fix F2 from the
2026-10-22 deterministic investigation of the 3D-off display stutter. The
evidence is automated tests under the XCTest shim and static reasoning. No live
run has exercised the change. **The live POINTERINPUT round trip is unknown,
and whether this changes perceived pointer smoothness is not measured.** A9
stays OPEN with its `known_defect`, and the product remains
ENGINEERING_PREVIEW.

## Mechanism before the change

Paths are under `apps/macos/Sources/BridgeVMControl/HvfEngine/`.

- The display link routes at most one absolute move per frame:
  `HvfFramebufferView.flushPendingPointerMove` drains `HvfPointerMoveMailbox`
  into `HvfEngineSession.sendPointerAction`, which routes
  `.pointer("move:<x>x<y>")`. Presses, clicks and releases (`releaseall:`)
  take the same route.
- After INPUTCAPS in an owned boot, `HvfSessionInputRouter.route` queues every
  event through `HvfRecoverableInputStream`, `HvfNegotiatedInputStream` and
  `HvfAcknowledgedInputStream` into `HvfOrderedInputQueue`. Once activated,
  the router never falls back to legacy input.
- `HvfOrderedInputQueue` sends one ticket and waits for its receipt (the
  agent's exit-0 header, `BVINPUT_INSERTED <id> <count>` and END) before the
  next. It is FIFO and holds at most 64 events, the in-flight one included,
  and 65,536 bytes. When it is full, `enqueue` returns false, the router
  answers `.refused`, and the move is dropped with a diagnostic.
- The view coalesced moves within one frame, but the queue did not coalesce.
  Whenever the round trip exceeded the display period, the queue filled to 64
  and refused most new moves, and each move it did send was seconds old.

## Rule

`HvfOrderedInputQueue.enqueue` replaces instead of appending when all of these
hold:

- a ticket is in flight, that is, sent and awaiting its receipt;
- the new event is a pointer move (the `move:` verb);
- the newest queued, unsent entry is also a pointer move of the current
  generation.

The replacement is a new ticket with a fresh id and the new absolute position.
It keeps the replaced entry's deadline, and the byte count changes by the
difference. It needs no free slot, so a full queue whose newest entry is a move
still accepts it. A replacement that would exceed 65,536 bytes is refused, and
the queued move stays.

- **Never merged:** a move never crosses any other event, because the newest
  entry must itself be a move. That covers press, release, click, right-click,
  scroll, wheel, key and text.
- **Never modified:** the in-flight ticket is held apart from the unsent
  entries, so it is never rewritten.
- **Receipts:** a request, and with it a receipt expectation, is created only
  when a ticket is sent (`HvfUnicodeInputRequest` takes the ticket's id). A
  replaced ticket was never sent, so nothing waits for it. A sent move still
  expects exactly one inserted event under its own id.
- **Binding:** a queue lives inside one router, whose binding is fixed. A
  different binding fails the router and empties the queue before any enqueue.
  Cancellation empties the queue and starts a new generation, so the
  generation check is defensive.
- **Cleanup:** `HvfRecoverableInputStream` keeps a copy of the unsent events.
  Pointer cleanup uses it to know which press, move or release was sent. When
  an admission does not grow the stream, the queue replaced a move, so the copy
  replaces its last entry. The copy therefore follows the queue's actual
  decision rather than a second copy of the rule.
- **Drag:** press, moves, release is sent as the press, the newest move before
  the release, and the release. The release carries the mouse-up position.
- **Code:** `HvfOrderedInputQueue.Event` moved unchanged into
  `apps/macos/Sources/BridgeVMControl/HvfEngine/HvfOrderedInputEvent.swift`
  with the new `isPointerMove`. This keeps the queue file within its 87-line
  budget.

### Narrowed from the requested rule

The rule as requested also merged moves when nothing was in flight. That
version failed the unchanged
`HvfLateGuestResetInputTests.testSlowShutdownExpiryIsEndedByTheLaterResetRecord`
in all six of its cases. The test routes two moves before any poll and expects
expiry to report `.cancelled(.expired, discarded: 2)`; the literal rule
reported `discarded: 1`. The rule therefore applies only while a receipt is
awaited, which is the only time a move waits behind a round trip. In the app a
queued route polls at once (`HvfSessionInputDriver.route`), so with nothing in
flight the move is sent immediately. The narrowing changes nothing there.

## What the guest sees

The guest builds `move:<x>x<y>` in `BuildPointer`
(`scripts/win-assets/bvagent-pointer-input.cs`) as one `INPUT_MOUSE`:

- flags 0x8001 (MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE);
- the normalised absolute coordinates;
- mouseData, time and extra info all zero.

Each request is one `SendInput` call, and the request id labels only the
receipt. A move carries nothing but its position. As far as that code shows,
the guest therefore sees the same final position, fewer intermediate positions,
and fewer `SendInput` calls and receipts. That is not invisible:

- an application that follows the path (drawing, gesture velocity or hover
  along the way) gets fewer points;
- before the change, at round trips above the display period, most moves were
  refused outright and the rest arrived seconds late.

The agent wire format is unchanged. The ordered route does not use the xHCI HID
endpoints described below.

## Tests

### The scripted round trip

`HvfOrderedPointerBacklogTests` drives the real `HvfSessionInputRouter`, as the
investigation's bench did:

- one move per display tick for 10 s;
- the agent's receipt visible exactly `rtt` after each send;
- polls as in `HvfSessionInputDriver`: after each queued route, a 10 ms pump
  while input is held, and a poll without lines after an inserted receipt.

Lag is the send time plus `rtt` minus the time that position was routed. The
test keeps polling after motion stops until nothing is held, so it also checks
that the final position is sent.

| Display, RTT | Refused (of routed) | Max queue | Sends during motion | Lag during motion p50 / p95 / max | Worst lag, final move included |
| --- | --- | --- | --- | --- | --- |
| 60 Hz, 50 ms, before | 337 of 600 | 64 | 200 | 3183 / 3183 / 3183 ms | 3183 ms |
| 60 Hz, 50 ms, after | 0 of 600 | 2 | 200 | 50.0 / 50.0 / 50.0 ms | 66.7 ms |
| 120 Hz, 20 ms, before | 637 of 1200 | 64 | 500 | 1273 / 1278 / 1278 ms | 1278 ms |
| 120 Hz, 20 ms, after | 0 of 1200 | 2 | 500 | 23.3 / 26.7 / 26.7 ms | 28.3 ms |

- Requests per second are the same before and after: one per round trip. The
  change is which position each request carries.
- After the change, every sent move's lag is at most the round trip plus one
  display period: 66.7 ms at 60 Hz and 50 ms, and 28.3 ms at 120 Hz and 20 ms.
  The worst case in both runs is the final move, which waits for the
  in-flight receipt.
- Before the change, the move routed on the last tick was refused, so the
  last position sent was an earlier one, about 3.2 s and 1.3 s late.
- **Correction to the investigation's bench.** The bench reported 364
  refusals (61%) and a lag of 3.44 s p50 at 60 Hz and 50 ms, and 671 (56%) at
  120 Hz and 20 ms. Those numbers were too high for the model. A replay of the
  bench loop shows why:
  - with the bench's `Double` seconds it reproduces the bench's log line for
    line;
  - with exact time it reproduces the test's numbers above.

  In the `Double` replay at 60 Hz, 135 of 173 intervals between sends were
  60 ms instead of 50 ms. Each time, the receipt became visible a rounding
  error after the poll that should have consumed it, and it waited for the
  next 10 ms pump. The bench therefore sent 174 requests in 10 s instead of
  200, and 467 instead of 500 at 120 Hz. The mechanism and the seconds of lag
  stand; the 61% does not.

### Ordering and accounting

`HvfOrderedPointerCoalescingTests` has eight tests. Seven fail on the code before
the change and pass after it. The eighth, the binding guard, passes on both by
design.

| Test | Before | After |
| --- | --- | --- |
| Drag | Press, three moves, release, all sent. | Press, `move:40x40`, release. |
| Moves around nine other events | All 20 moves sent. | Each pair is sent as its newest move. Press, release, click, right-click, right press, scroll, wheel, key and text keep their order and payloads. |
| An in-flight move is never rewritten | The second queued move is sent next. | It is replaced by the third. The in-flight move's own receipt still completes it. |
| A replaced move is never awaited | Three sends. | Two sends. 31 s later nothing expires, and input continues. |
| Cleanup follows what was sent | A rejected move left the cleanup release at the earlier position. | The release goes to the sent position. A completed drag leaves no release. |
| Queue: full queue and byte accounting | A full queue refused the move. | The replacement needs no slot, and the bytes run 87 → 79 → 0. An idle queue still keeps two moves. |
| Queue: byte limit | A move without room to append was refused. | The replacement over the limit is refused. A smaller one replaces the queued move. |
| Another binding | Refused, router failed, queue emptied. | The same. |

### Runs

The focused runs used the shim pattern on 46 test files. Those were every
BridgeVMControl test that exercises the ordered transport, plus their
fixtures.

| Run | Passed | Failed | Of the failures |
| --- | --- | --- | --- |
| Before (base code, final tests) | 146 | 22 | The 9 new behavioural tests and 13 others |
| Literal rule (experiment) | 152 | 16 | HvfLateGuestResetInputTests, 2 new tests and the same 13 |
| After (`cf5bd6c1`) | 155 | 13 | The same 13 only |

- The 13 others are in `HvfOwnedRuntimeCodecTests`,
  `HvfOwnedRuntimeControllerTests` and `HvfEngineSessionGuestResetGateTests`.
  The focused runner copies test files elsewhere, so their fixture paths,
  derived from `#filePath`, point outside the repository. They fail the same
  way before and after.
- In the literal-rule run, one of the two new failures was the idle-queue
  check, which that rule breaks. The other showed that my first byte-limit test
  expected the wrong thing: a 16-byte replacement over 65,520 bytes fits
  exactly. That test was corrected, and the before run was repeated with the
  final tests.
- The whole BridgeVMControl shim suite at `cf5bd6c1`:
  1462 passed, 0 failed and 3 skipped (the opt-in, release-only and
  live-fixture tests). That is 10 more passes than the 1452 passed and
  3 skipped reported for 9fa07eb4, a figure not re-measured here. The
  13 fixture-path tests pass there. In the same run BridgeVMApp passed
  425, AppleVzRunnerCore 65 and BridgeVMProductE2E 403, with none
  failed.
- `scripts/check-refactor-budgets.sh` and
  `scripts/check-documentation-system.sh` passed at each commit.

`scripts/render-capability-status.py --check` and
`scripts/check-capability-registry.sh` fail from `cf5bd6c1` on. Both report
"code changed since tested_commit" and ask for A11 to be re-proved. That is
the expected state of an unsealed code commit, and
`capabilities/windows-hvf.json` was not edited.

Not run:

- `scripts/check-project.sh`, which the operator excluded.
- Hosted CI, because nothing was pushed.

The logs stay outside the repository, in the operator's evidence directory
`dev-20261021-evidence/pointer-coalesce`.

## Deviation entry

Commit `497161cf` adds `xhci-hid-report-pacing` in the new module
[`qemu-virt-deviations-usb-input.json`](../../machine-contract/qemu-virt-deviations-usb-input.json). It records
existing behaviour and changes nothing.

### BridgeVM side, verified in code

- **One device, two endpoints.** BridgeVM presents one high-speed composite HID
  device. Its keyboard (DCI3) and pointer (DCI5) endpoints declare
  `bInterval` 10, which the xHCI model does not enforce.
- **Interval.** Reports are paced in host time. One interval applies to each
  endpoint against its own previous emission: `BRIDGEVM_XHCI_REPORT_INTERVAL_MS`,
  default 30 ms, 0 to disable, clamped at 10,000. Its documented reason is that
  Windows dropped keystrokes when reports landed microseconds apart.
- **When reports go out.** Reports are written only by a vCPU thread after an
  exit: the pre-run and data-abort drains, xHCI register writes and the live
  input poll. Each drain releases at most one report per endpoint.
- **Held pointer reports.** A held pointer report arms a host thread that forces
  a vCPU0 exit at its deadline.

The investigation cited `xhci_input.rs`, which is
`crates/bridgevm-hvf/src/platform_virt/xhci_input.rs`. The probe constants are
in `crates/bridgevm-hvf/examples/hvf_gic_boot_probe/probe_env.rs`.

### QEMU side, recalled

None of this was re-read. My confidence in each claim:

- **High:** `usb-kbd` and `usb-tablet` are separate devices.
- **Medium-high:** QEMU's HID devices set no host-time minimum spacing between
  reports, and `hid.c` merges tablet motion into an unread event while the
  buttons are unchanged.
- **Medium:** the high-speed tablet declares a 1 ms interval.
- **Low:** how QEMU's xHCI schedules interrupt transfers by interval.

### Not covered

The composite-device topology differs from QEMU's two devices. No entry covers
that difference, and none was added here.

## Not measured

- The POINTERINPUT round trip on a live guest, and whether live sessions ever
  exceed one display period.
- Whether the pointer feels or looks smoother. That needs a live run and a
  screen recording.
- The real driver's timer and main-thread scheduling, and the run.log tail
  latency. The test models these as exact.
- What the guest observes of the xHCI report spacing.
