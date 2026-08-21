# B4: the 20-clone batch localises the loss below the input stack (2026-08-20)

## The measurement

Studio tier `t8-pointer-reliability`, job `20260820-134138-21205-18560` at
commit `13e83a58d0e423e8b4ad9cac4d53f62b1436c234`, gate log SHA-256
`b34c42ca9da1144cbd03ec84e4e516357669ae5ec576ceda5c2cfd9b07d40332`,
Mac16,9 / macOS 26.5.2, 20 independent APFS clones of the sealed
`net-live-20260724` image, each with its own vars file. Result:
**landed 0/20** — the gate honestly FAILED, and in failing it produced the
first measurement that separates the loss classes fixed in
`b4-pointer-reliability-gate-20260820.md`.

Per run, all 20/20 identical:

- host injection fired: `emitted_move_reports=1 emitted_button_reports=1
  emitted_release_reports=1`, `busy_rejections=0`, `marker_seen=true` — the
  xHCI device model completed the move, button and release TRBs and posted
  their transfer events;
- no `dci5_drain_blocked` trace in any run log — the transfer ring never
  stalled while a report was queued;
- the in-guest probe ran across the click window (`BVPTR begin` before the
  fire, `BVPTR summary` after) and reported `presses=0 releases=0 stuck=0`;
- no framebuffer change at any delay checkpoint: every
  `pointer-input-*` capture in run1 has the identical content hash
  (`b533d118…`), i.e. the ramfb checkpoints all show the same frame.

## What this falsifies and what it indicts

The 2026-08-17 batch proved every host-visible counter is identical between a
landed and a lost click. This batch adds the guest side: **the Windows input
stack never saw the button transition** (`GetAsyncKeyState` polled at 4 ms for
240 s spanning the click; zero transitions in 20/20 runs). Combined:

- falsified: foreground-routing loss, application-side loss, press/release
  coalescing inside Win32 — the report never reached `user32` at all;
- falsified: host-side queue rejection, ring stall, event-post failure — all
  host counters show clean emission and no drain-block trace;
- indicted: the segment between the posted transfer event and the HID class
  driver's report delivery — completion-code/length semantics of the event
  TRB, interrupter/MSI delivery for that specific event, endpoint interval
  semantics, or the guest driver discarding the report.

Two caveats keep this honest. First, the probe polled bit 0x8000 only; a
press+release pair completing within one PS 5.1 poll iteration would be
invisible. The probe now also latches bit 0x0001 (`BVPTR edge`), so the next
batch can distinguish "never delivered" from "delivered faster than the
poll" — the 2026-08-17 batch's single landed click argues delivery is
possible, so this hole must be closed by measurement, not assumption.
Second, the ramfb checkpoints captured the boot framebuffer, not the active
virtio-gpu scanout (all hashes identical across five minutes of a live
desktop), so the visible-reaction leg of the gate measured the wrong surface;
the same limitation was measured for F4 in `GOAL.md`. A landed click must be
proven by the probe's transitions, which are surface-independent.

## Status

B4 stays **OPEN**. The gate did its job by failing: the loss is at or below
the HID class driver boundary, and the next instrument (per the fixed gate
doc) is the correlation of the exact button-report TRB with its completion
code, transfer length and interrupter delivery on a run where the probe's
latch bit is armed. That instrument now exists:
`crates/bridgevm-hvf/src/xhci/trace_dci5_emission.rs` prints one line per
emitted DCI5 report (kind, raw 6 bytes, TRB GPA, transfer/written length,
completion code, event TRB GPA/status/control, IMAN.IP/IE, EHB, ERDP) under
`BRIDGEVM_TRACE_DCI5_EMISSION=1`, which the gate script sets for every batch
run. The next batch therefore records, for each of move/button/release,
whether the completion semantics or interrupter state differ at the moment
the guest stops seeing them.

### 2026-08-21 correlated batch

Exact-SHA Studio job `20260821-003915-18894-18703` at
`56e9297b6f932de0350379ef636692af51837f82` again failed the fixed gate:
**0/20 landed**, p95 `none` (gate log SHA-256
`3f6ab581d1b9a49ed8c3e95431dab74e7ee8478a5496cfdeb8d5897f5c094892`;
summary SHA-256
`1b28844faeae291b7a56650db8cfd712f7fedab3dcc2eaf3101637699ca57ee5`).
All 60 move/button/release records were written 6/6 bytes with completion
code 1 (Success), IMAN.IP/IE set and EHB set. Each run used one guest-selected
interrupter (distribution: vector 1: two runs, 2: three, 3: five, 4: ten), so
the failure is not specific to vector 4. In every run ERDP remained at the
move event while the button and release events occupied the next two event
TRBs. Run 20's 4 ms `GetAsyncKeyState` latch caught one edge 10,648 ms after
probe start (press=1/release=1 by parser), proving the report can occasionally
reach user32 between polls; it still did not produce a measured visible
change and therefore did not land. The earlier categorical statement that no
button transition ever reached user32 is retracted for this batch.

This receipt falsifies malformed transfer length/completion and a single bad
interrupter vector. It did not yet distinguish an MSI-X message that was never
generated from one rejected by the host GIC.

A follow-up diagnostic job, `20260821-040346-74328-24747` at
`a2381bce92a2f0f4b7cdb378cbc70f8e12f1bbcd`, enabled the existing
`--trace-irq` path. It was intentionally cancelled after three failed runs
because the first run already supplied the requested separator; this
cancelled job is diagnostic measurement, not criterion evidence. In run 1,
the guest-programmed DCI5 vector 4/INTID 135 was delivered with host GIC
`status=0` for move/button/release (run log SHA-256
`ac4235e146eac18a86711c744ff9953ae602614135c22fa162ac04946061c6cf`).
The failure is therefore not a missing MSI-X message or host-GIC rejection.

The event sequence instead exposed a missing device-model transition:
BridgeVM sent each normal per-event MSI, then cleared IP when software
acknowledged the first event, but did not re-notify the two events still beyond
ERDP. Initial code reasoning also suppressed repeat MSI while EHB was set;
that conclusion was wrong and is retracted. Fixed-head t8 job
`20260821-052328-2196-32526` immediately falsified it: run 1 reached the agent
but Windows never enumerated the pointer interface (`hid_report_descriptor_gets=0`,
DCI5 configure/doorbell=0), so the job was cancelled and is not evidence.
Windows requires the existing per-event delivery during enumeration.

The corrected model therefore preserves per-event MSI and adds only the
measured missing transition: ERDP/EHB acknowledgement re-notifies event-ring
entries still beyond the guest dequeue pointer through the platform MSI-X
queue. Unit tests cover the controller-local sequence and full BAR-to-platform
MSI-X path. Corrected-head t8 job `20260821-060243-17261-16687` restored full
pointer enumeration and injection but its first run still failed with the same
ERDP-at-move shape and no probe transition, so it too was cancelled. This
proves re-notification alone is not the B4 fix.

The next separator is guest-probe validity. The agent is installed as an
interactive-logon task, but the `Win32_Process.Create` child had never proven
that it retained the active input desktop; `GetAsyncKeyState` may return zero
outside that desktop. The probe now records nonzero session ID, successful
`OpenInputDesktop`, and foreground HWND, and the parser fails closed unless
all three prove an interactive desktop. Diagnostic t8 job
`20260821-064039-29474-29147` at `1d62a904413596775e04fabd2bdff042cc83dd79`
then proved exactly that in its first run (`session=1`,
`input_desktop_open=1`, foreground HWND `197210`) while the click still had
zero transitions; the run log SHA-256 is
`a3046b05be4182fdc7f09723e292ab37f22d4a6a1d4f5276861544c4f2b07ac1`.
The job was cancelled after this separator and is diagnostic, not closure
evidence. The next probe records cursor-coordinate transitions on that same
active desktop, separating loss of all DCI5 reports from button-only semantic
loss. Diagnostic job `20260821-071532-41007-26374` at
`9f7d57a2b6dcaaefb98332fabbab59c932b6dce1` supplied that separator in run 1:
the active-desktop cursor moved from `(400,300)` to `(544,380)` at 18,023 ms,
while press/release/edge remained zero (run log SHA-256
`77ccd1051e9f85d8a7177058e3bd41adca3e9bbc20f30638c19a81e84182eb2a`).
The job was cancelled after this diagnostic result. Move reaches user32;
button/release specifically do not.

In the same run, move/button/release events occupied consecutive Event Ring
slots while ERDP stayed at the move event. DCI5 event-consumption pacing was
then tried at `2eadc8619516fc2a6630435f0a1edbb74004d9b2` (job
`20260821-075716-55962-11868`). Run 1 proved the gate worked — each next report
posted only after ERDP advanced — but move alone still reached user32 and the
button was lost (run log SHA-256
`f00c67b873ea34bd31026719543dcd4963a29a3a14aa14b85e70347c1f2e9644`).
The job was cancelled and the non-causal pacing was removed.

The surviving code-level separator is below that experiment: guest ERDP MMIO
called `XhciController::mmio_write_with_mem`, which directly late-drained the
next pointer report and bypassed `VirtPlatform`'s host-time report pacing. The
2026-08-17 300 ms pacing experiment therefore never guaranteed a button hold:
button and release could still be emitted back-to-back through successive
ERDP writes. The fix removes controller-internal pointer late-drain and routes
post-MMIO pointer delivery through the platform pacing gate. A platform BAR
regression requires ERDP MMIO at unchanged host time to leave button pending,
then release button and release reports only at consecutive 30 ms host-time
steps. The generic EHB experiment and event-consumption pacing are absent. First-run
job `20260821-084346-79164-23719` at
`cc77de82e24280f261a969e9765a96eb39012080` proved the bypass was removed but
still lost the button at the default 30 ms interval (run log SHA-256
`545670861e4aaecde1059c3265cb2b192d321bf05dad81dbb6cee2704480980e`), so it
was cancelled. A normalized 100 ms hold was then measured at
`0a11c25a224693637ed7529172aa3098138d554c` in job
`20260821-091826-91520-28485`; runtime confirmed the 100 ms interval, but run 1
again delivered only move (run log SHA-256
`a7f268bf002a4b4e187bbd4e2f0e099155281e1facae621819acd2dfae52391c`).
The job was cancelled. Duration is not the separator, and the override was
removed.

The remaining guest HID semantic difference was the descriptor itself. The
6-byte payload already matched QEMU USB tablet, but BridgeVM advertised a
63-byte local variant (3 buttons, different constant padding, no physical
axis/wheel declarations) instead of QEMU's Windows-proven 74-byte tablet
descriptor. That descriptor was tested at `4f8f18a4f7cfa005b801968b5c8c907590f676dd`
in job `20260821-095849-7083-26220`; run 1 enumerated the 74-byte descriptor
but again delivered only cursor move (run log SHA-256
`d5d32658dd63c9ab7eca01ac6e1d5702ac1f7b5dc10ca65e16db3af43579e31d`).
The job was cancelled. Descriptor shape is not the separator, so that change
was removed rather than retained as an unrelated compatibility change.

The next trace added Normal TRB report-buffer GPA, TD-end GPA, and Event Data
parameter. Diagnostic job `20260821-104402-25279-6208` at
`33c8600ebaffb6371ed35660ed865c69464130b4` showed distinct move/button buffers
(`…8800`, `…8a00`) and distinct Event Data cookies; release reused move's
buffer, not button's. Button cannot have been overwritten by release in the
same DMA buffer. Run 1 still delivered only move (run log SHA-256
`7b0ca99616bd00abddf56ce78fb19339102606dc455966f96147d4d21ae9a8af`), and the
job was cancelled.

The next diagnostic sent move+press with no release. As required, job
`20260821-111848-37586-7823` at `b43f1bc770f76c9fdc663e4adb923b8c8f41a2b8`
failed the gate (`press=1`, `release=0`, `stuck=1`), but its first run proved
the button path: on the active desktop, cursor move arrived at 31,847 ms and
button high-bit press at 31,871 ms, 24 ms later (run log SHA-256
`f9d992b81cd05716308152fcdf54182fae064297dd3abbde462bab4e02d0ea75`).
The job was cancelled after that separator. Descriptor, payload, DMA buffer,
completion, MSI and user32 press delivery are all exonerated; the loss occurs
only when release follows before the guest/probe observes the transition.

The normal move+click workload was restored with a 200 ms report interval in
job `20260821-115318-49986-28036` at
`7658ba6b00b060bc76ff7941f8e2d285c4dc09e6`; run 1 still delivered only move
(run log SHA-256
`b5ab849349adff862115fc17846e895573ac9a251f57ebf243ddf60805d3156a`).
The job was cancelled. Even a near-limit hold is not sufficient after the
leading move report.

The next diagnostic sent click-only at
`c086581f40101ceb90283a7c70ff57476730c913` in job
`20260821-122823-62528-13973`. It still delivered only cursor move (from the
button report's absolute coordinates), with no button transition (run log SHA-256
`829f03af282d6f08bf9d40b16a2c34e2be0fb66ac7e7d2368838d0735ad1ce9d`); the
job was cancelled. A leading move
report is not the cause; release following press is the separator.

The next diagnostic used a deliberately over-limit 1000 ms hold at
`50b9dc25f6b745dcbd879340419b76ad5ae25c96` in job
`20260821-130407-75761-3874`. It still delivered only move, but monotonic trace
proved the supposedly 1000 ms path emitted move→button after only 622 ms and
button→release after 4,745 ms (run log SHA-256
`b5b820e8286100e262474539d3ddc34ef4874f96a616de353da270370fc23918`).
The job was cancelled.

The cause is a stale clock sample: `queue_xhci_pointer_input_actions_with_mem`
stored cached `platform.host_now` as the first emission time, but pointer fire
already receives a newer `now`. Every prior interval experiment therefore
measured from an old pre-run sample rather than actual first DMA. Pointer fire
now writes its actual `now` into the platform immediately before queue/drain.
A synthetic-clock regression seeds a 900 ms stale sample and requires release
to remain blocked until 1000 ms after the actual trigger, not 100 ms after it.
The normal move+click workload uses a 200 ms interval, below the fixed 250 ms
limit. Actual-time pacing alone was then measured at
`ec73d63e917e0daeaf2b47fbeb20dad2df1daabc` in job
`20260821-134702-91382-12896`: move→button was 578 ms and button→release
4,462 ms, but all three events still posted before ERDP moved and only cursor
move reached user32 (run log SHA-256
`1335743a0ca79ebc936593ccd8dd3c5a49779ae5e9997d6e263f4887c8f21ae7`).
The job was cancelled.

The two measured necessary conditions had only been tried separately:
event-consumption pacing advanced ERDP but did not enforce a real hold;
actual-time pacing enforced a real hold but still batched events ahead of
ERDP. The final candidate requires both before every later DCI5 report: the
configured host-time interval has elapsed from actual DMA, and the previous
pointer event's interrupter ERDP has advanced. A BAR/platform regression checks
both halves independently for move→button→release. B4 remains OPEN pending the
normal 20/20 receipt.

The first doorbell-gated run (`20260821-151222-27082-29574`, run 1) finally
reached the active desktop with `press=1 release=1 stuck=0`; host trace showed
ERDP advancing before each later report and a 202 ms button hold. It still
reported `first_changed_ms=none` because the gate parsed boot ramfb, a surface
this document already classifies as non-evidentiary after virtio-gpu takes
scanout ownership. The run is diagnostic, not closure evidence.

Two latency/evidence corrections follow without changing the fixed criterion.
The coalesced button report moved the cursor but Windows again discarded its
button bit, so that experiment was retracted. Instead, each run prepositions
the pointer through the normal live-input path before the latency window, and
the fixed trigger injects click-only at the already-settled target. The guest
probe fails closed unless its initial cursor is `(544,380)`, proving the
precondition rather than assuming it. The measured click remains normal
button+release with a 200 ms hold.

Visible reaction is parsed only from existing `virtio-gpu-checkpoint-*`
active-scanout artifacts. The gate enables both the proven display framebuffer
sink and 100 ms PPM exporter/readback feed, still sampling at the fixed
5..1000 ms delays. A ramfb-named artifact fails closed and cannot land a run.
B4 remains OPEN pending the corrected normal 20/20 receipt.
