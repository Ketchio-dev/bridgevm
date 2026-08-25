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

### 2026-08-22 prepared-media diagnostics

The first attempt to move t8 from the non-display `net-live` source to the
immutable t7-proven driver-ready pair, job `20260822-045326-10470-19681` at
`1cc1feaf6797c19ecab5c08e6a1c091577e64fbc`, was cancelled before guest boot.
The source is correctly mode 400 and APFS `cp -c` preserved that mode on the
private clone, so NVMe open failed with `Permission denied`. The correction
changes only each run's disk and vars copies to mode 600; source hashes and
mode 400 permissions remained unchanged. This is host-staging diagnosis, not
B4 evidence.

The corrected staging run, cancelled job `20260822-050458-14669-13015` at
`78c777860ca532241f007edd3c298eae3ad013f6`, proved the prepared guest is
GPU-driver active (9,366 virtio-gpu trace records), but also falsified the
claim that changing the image alone supplied the fixed visible target. The
guest repeatedly requested 1280x1024 while the host advertised its default
1280x800: 929 `SET_SCANOUT` commands returned `ERR_UNSPEC`. The preposition
report moved the cursor to `(871,650)`, not required `(544,380)`, so the parser
correctly returned `invalid-desktop`. All pointer checkpoints consequently
fell back to `ramfb-checkpoint-*`, which the active-scanout parser rejects.

That run also emitted 6/6 Success move, button and release reports with event
ERDP advancing before each report, but button-to-release took 12,543 ms
(`host_elapsed_ms=64851` to `77394`) while waiting for the prior event to be
consumed. It observed no user32 transition. The job was stopped after this
separator and is not closure evidence. Active host/guest geometry and a real,
stable click target must now be prepared outside the measured latency window;
the fixed N=20 and p95 <=250 ms criteria remain unchanged. The briefly raised
hypothesis that Event Data status `0x01000006` reports an invalid six-byte
residual was also wrong and is retracted: with ED=1 the low field is EDTLA, so
six is the correct actual transferred length.

### 2026-08-23 gate and delivery corrections

Two defects were measured in how the gate observed a run, and one in how the
host paced the click. None of the jobs below is closure evidence.

The watcher itself perturbed the surface it measured: it locked the IOSurface
and copied 64 KiB every 5 ms, contending with the GPU blits whose latency it
was timing. It now declares `IOSurfaceGetSeed`, polls the seed lock-free at
1 ms, and locks and hashes the target region only when the producer publishes
a new frame. Job `20260823-004833-19395-16687` at `9ff9fb1` measured three
consecutive passing runs under that observer (445 ms, 186 ms, 240 ms, each
`fired=true press=1 release=1 stuck=0 landed=true`) before run 4 failed with
`FAIL: guest resize failed`. Because 20/20 was then impossible, the batch was
cancelled and its clone reclaimed. Run 4's failure was not resize itself: the
host accepted `RESIZE 1600x900`, but no `BVAGENT CMD`/`END` pair appeared for
300 s while heartbeats printed `awaiting-reply=true`, i.e. the agent control
round-trip was absent.

Those three passing runs also show the residual variability is in the
presentation path, not lost guest input: the first post-button blit took
~442 ms in one run while press and release were both observed in the guest.

The host-side defect was periodic wake bursts. A live click was paced from a
cached host time and the vCPU was woken on a fixed 16 ms/500 ms cadence, so a
configured 200 ms hold measured 219-300 ms. The fix stamps the platform clock
with the command's own instant, wakes once per control-file append, and arms
one shot at `VirtPlatform::xhci_pointer_report_deadline()`. Job
`20260823-013113-34370-3941` at `fd9c870` was submitted against that head and
stalled in run 1 boot (`BVAGENT SERVICE alive observed=0 ... seconds=352`); it
was cancelled to free the 48 GiB clone, so it is not evidence either.

B4 therefore remains **OPEN** with no N=20 receipt. The fixed criterion is
unchanged: 20 independent clones, guest-observed press and release in every
run, `stuck=0`, exactly one target click at the expected coordinates, a real
active CGL IOSurface reaction 20/20, and first-change p95 <=250 ms.

### 2026-08-23 first complete N=20 batch: 17/20, p95 531 ms — FAIL

Studio tier `t8-pointer-reliability`, job `20260823-054152-13053-2622` at
`main` commit `31df3c313bfa925d215883394d407950e25ae1d2`, gate log SHA-256
`3c9f7d72e26f7e0f0d526a696728c2a54ef14409734057f46eae09f5cb16c5d5`, summary
SHA-256 `4192f7b059d34b368de60b9e29b0acaf9828efda2ececedfc30b132c8f633e8e`,
Mac16,9 / macOS 26.5.2. This is the first batch since the gate was fixed that
ran all 20 runs to completion instead of being cancelled. It **failed** on both
halves of the criterion: `landed 17/20`, `p95_first_changed_ms=531` against the
250 ms limit.

The 17 landed runs were unanimous on the input half: `fired=true press=1
release=1 stuck=0`, exactly one target click at the expected coordinates, and a
real active CGL IOSurface reaction. So the delivery work holds — a host click
now reaches the Windows input stack and produces a visible frame.

The failure is elsewhere, and the distribution says where. First-change times
for the landed runs were 169, 186, 208, 219, 221, 229, 248, 314, 315, 374, 432,
440, 442, 467, 481, 529, 531 ms: a bimodal split, with seven runs at or under
the limit and ten roughly twice that. A press that Windows has already consumed
cannot explain a 500 ms frame, so the remaining cost is in presentation, not in
input delivery. This matches the ~442 ms first post-button blit recorded on
2026-08-23 above.

The three non-landing runs are three different refusals, and none of them is a
lost click:

- run 10 produced no `bv-pointer-target-ready.log` at all, so the parser
  returned `invalid-target` — the target window never reported itself;
- runs 9 and 19 reached a valid ready line, an interactive desktop and
  `BVPTR_READY` at the button centre, but the watcher then refused with
  `active target baseline not presented`: the white button never held still on
  the active IOSurface for the 120 s convergence window. The gate therefore
  never injected the click (their `input.ctl` ends at `POINTER move`, and
  `dci5_emission` count is zero), and `BVPTR summary presses=0 releases=0`
  records an absent click rather than a discarded one.

That is the fail-closed design working: a run whose target was never stably on
screen produces a refusal, not a latency. It also means 3/20 of the campaign
was spent on target-presentation flakiness that has nothing to do with B4's
question.

B4 stays **OPEN** with the criterion unchanged. Two separate problems are now
measured rather than assumed: the guest-side target must present and hold
deterministically, and the post-click presentation path must stop costing
roughly twice the budget in half the runs. Neither is fixed by this batch, and
no part of this receipt closes the criterion.

### 2026-08-23 latency decomposition: the 500 ms runs are not slow blits

The 17 landed runs of job `20260823-054152-13053-2622` carry enough host-clock
detail to split `first_changed_ms` without running anything new: the DCI5
emission trace stamps each of move/button/release with `host_unix_ns`,
`scanout_blit` now carries the same clock, and `visible.env` records the
control-write and pixel-change times on it too. Four segments, averaged over
the seven runs at or under the limit versus the ten above it:

| segment | fast (<=250 ms, n=7) | slow (>250 ms, n=10) |
| --- | --- | --- |
| control write -> button report emitted | 31 ms | 55 ms |
| button -> release emitted (configured 200 ms hold) | 237 ms | 279 ms |
| release -> first scanout blit | -58 ms | +100 ms |
| blit -> observed pixel change | ~0 ms | ~0 ms |

Three things follow, and none of them is "the blit is slow".

**The blit is not the cost.** Individual blits take 1.0-2.5 ms, and the gap
between a blit and the pixel change the watcher hashes is about zero in both
groups. Presentation, once started, is immediate.

**The dominant term is when Windows decides to repaint, not how long it
paints.** Charging the GPU trace against the window between the button report
and the next blit, the fast runs are fully explained by guest GPU work (188 ms
of command duration inside a 179 ms window), while the slow runs leave 221 ms
that no command accounts for. The slow runs also issue *fewer* commands and
less command time (158 ms) than the fast ones, so the guest is not doing more
work — it is doing nothing and waiting.

**What separates the groups is whether the repaint waits for release.** In 5 of
7 fast runs the pixel change happens *before* the release report is even
emitted: `MouseDown` repaints the button and the frame is already on screen. In
10 of 10 slow runs the change happens *after* release, i.e. the frame that the
watcher sees is the `Click` handler's, not `MouseDown`'s. That is the whole
bimodality: two different repaints are being timed.

The measured hold explains why the second repaint can be so late. The
configured interval is 200 ms, but the button-to-release times measured on the
host clock are 200, 202, 204, 205, 207, 216, 224, 224, 234, 249, 263, 274, 315,
315, 332, 392 and 394 ms — a tail reaching nearly double the setting.
`corr(hold, first_changed_ms) = 0.45`, and runs with a hold over 250 ms average
380 ms to first change against 314 ms for the rest. So the exact one-shot wake
fixed the systematic drift but not this tail, and any run whose visible
reaction is the post-click repaint inherits the overshoot directly.

Two consequences for B4, both requiring more measurement before any change.
First, the criterion currently times whichever repaint happens to be observed,
so the same guest behaviour can score 186 ms or 531 ms; a click is a press and
a release, and the gate should say which transition it is timing. Second, the
remaining hold overshoot is a real host-side defect and is not explained by the
periodic-wake bug already fixed. B4 stays **OPEN**.

### 2026-08-23 second complete batch: the hold overshoot is a missed host wake

Job `20260823-113151-99719-9275` at `main` `8e1f38791008da6b3fa20c437ca0a42065a076c5`
(gate log SHA-256 `b28ec56e47110987db51527c41b757f1b20ea85d6fb4247b841e701d11169a94`,
summary SHA-256 `93b3098947db40c175528d03c176327ddfc2819285bac2d7df615bee7adc7d8e`)
also completed and also **failed**: `landed 13/20`, p95 660 ms. It carried the
print-only deadline observation, and that instrument answers the question the
previous decomposition could not.

In every run where a DCI5 report was still queued when the arming pass ran, the
printed lateness equals the hold overshoot measured independently from emission
timestamps:

| run | hold overshoot | printed `late_us` | delta |
| --- | --- | --- | --- |
| 1 | 94.8 ms | 95.0 ms | 0.2 ms |
| 14 | 169.5 ms | 169.0 ms | 0.5 ms |
| 17 | 43.4 ms | 43.6 ms | 0.2 ms |

The report was eligible and waiting, and the drain that should have emitted it
ran late. That is a **missed host wake**, not a guest TD that was not ready:
an unready TD would leave the report ineligible, and the emission would not be
overdue at all. The competing explanation is therefore falsified for these
runs, without changing any behaviour to test it.

Ten further runs overshot (0.3 to 150.3 ms) without printing anything. That is
the instrument's own blind spot rather than a contradiction: it only prints
from the automation pass that arms the next deadline, so an overshoot resolved
before that pass runs is invisible to it. Closing that hole is a measurement
task, not a fix.

The target-presentation failure is also reproducible and got worse: 6 runs
refused with `invalid-click-count` and 1 with `invalid-target`, against 2 and 1
in the previous batch. In all six the probe reported a valid interactive
desktop and its cursor at the button centre (`session=1 input_desktop_open=1
cursor_x=800 cursor_y=450`) while `presses=0 releases=0 moves=0`, i.e. the
white target never converged and the gate refused before injecting, exactly as
designed.

Two batches now agree: input delivery works when the target is up, the residual
latency is presentation, and within that the hold overshoot has a measured
cause. B4 stays **OPEN**.

### 2026-08-24 third complete batch: stable black begins in renderer command submission

Job `20260824-003132-73288-13833` at `main`
`697477f93b14b1b1317319784d6490ae19954924` completed and **failed**:
`landed 15/20`, p95 709 ms (gate log SHA-256
`bdea989a58b7db5eb30586a700744ab88115ad79e586a300d5e01cbfb29d89a3`,
summary SHA-256
`191137cc0e0644c2580859cf849170ecbcf6e5bb0de42c5c8b3905828d74efd6`).
The unchanged limit is 250 ms, so neither 15/20 nor the 709 ms p95 is partial
credit.

The target-baseline evidence closes its coarse ambiguity. Runs 5, 6, 8 and 20
all wrote a valid `BVTARGET ready` record at 1600x900, but the active IOSurface
remained stable and completely black for the full 120-second convergence
window: `peak_white_px=0`, `final_white_px=0`, and 567 to 580 settled samples.
This is not target-window flicker and not a briefly visible target. Run 12 is a
different refusal: it never wrote a target-ready record (`invalid-target`).

The black frame is not a missing CGL publication or failed IOSurface copy.
Run 5 completed 179 `scanout_blit` events and 94 paced CPU readbacks, while its
90-second and 120-second 1600x900 CPU checkpoints both contain 1,440,000 black
pixels, zero nonzero pixels and one unique colour. The renderer scanout source
itself was black.

Existing JSONL then locates the first divergence before the 1600x900 target
transition. All four stable-black lanes enter a persistent ctx-7 renderer
failure:

| run | ctx-7 failed submits | first command | status | resource | backing before failure |
| --- | ---: | ---: | ---: | ---: | --- |
| 5 | 322 | `VIRGL_CCMD_TRANSFER3D` (43) | 22 | 140 | none |
| 6 | 301 | `VIRGL_CCMD_TRANSFER3D` (43) | 22 | 137 | none |
| 8 | 203 | `VIRGL_CCMD_TRANSFER3D` (43) | 22 | 308 | none |
| 20 | 277 | `VIRGL_CCMD_TRANSFER3D` (43) | 22 | 143 | none |

Each resource exists and was attached to ctx 7, but no successful
`RESOURCE_ATTACH_BACKING` precedes the first transfer. virglrenderer reports
`Illegal resource` and `Illegal command buffer`; the ctx-7 errors then cascade.
The first failure occurs at JSONL sequence 1,535 to 3,250, before the first
successful 1600x900 `SET_SCANOUT`/IOSurface blit in each run, so launching the
pointer target does not trigger the poison.

This separator is persistence- and context-specific, not “any renderer error”.
Fourteen of the fifteen landed lanes have zero failed submits. Landed run 10
has seven transient failures on ctx 5, interleaved with later successful
submits, and still presents normally. The earlier unqualified statement that
successful runs had zero failed submits was wrong and is retracted.

One boundary remained unobserved by this batch. BridgeVM consumed a malformed
virtqueue descriptor chain at used length zero and advanced `last_avail_idx`
without recording the rejection. Therefore the absent backing attach could
mean either that the guest never posted it, or that BridgeVM consumed its
malformed descriptor chain. PR #69 adds a sampled, structure-only
`descriptor_chain_rejected` event to decide that question without changing
queue-consumption behaviour; it logs no guest address or payload. The event is
bounded to the existing first-64/each-1024 trace policy and descriptor-table GPA
overflow now refuses instead of panicking. B4 remains **OPEN**.

## Batch 5 — 2026-08-24, first batch with renderer containment

t8 job `20260824-115428-72165-14984` at main `fcb95a59c5c63527564c0e54f479c3d05950ef0e`,
the first batch to run with the unbacked-`TRANSFER3D` preflight merged.
Honest **FAIL**: `landed 12/20 p95_first_changed_ms=688 (limit 250)`.

Failure classes: six `invalid-click-count`, two `invalid-target`. All six
stable-black lanes (runs 1, 3, 5, 7, 10, 17) refused with
`result=baseline-not-presented`, `peak_white_px=0` and `final_white_px=0`
(settled samples 195–548) — the target was never painted, not painted-then-lost.

The ctx-7 poison still separates black from landed perfectly and exclusively:
181–240 failed ctx-7 `SUBMIT_3D` in each black lane, **zero** in all fourteen
non-black lanes. `SET_SCANOUT` errors were tested as a candidate separator and
rejected — they occur in all twenty lanes (224–269 each), including every
landed run, so they are background noise.

### What the containment did and did not do

It works where it applies, and that is not enough to fix this.

In run 3 the offending buffer is resource 153: `RESOURCE_CREATE_3D target=0
bind=64` at seq 1576, `CTX_ATTACH_RESOURCE` to ctx 7 at seq 1577, and **no**
`RESOURCE_ATTACH_BACKING` ever. The preflight refused it at seq 1641 and the
next ctx-7 submit (seq 1647) returned `OK_NODATA` — the context was not poisoned
at that point. The same single-rejection-then-continue shape appears in runs 5,
10 and 17.

The lanes still went black because a *second, different* failure follows:
in run 3, seq 2198 fails `renderer_command_id=8` (`VIRGL_CCMD_DRAW_VBO`) with
`renderer_status=104` (`ENOTRECOVERABLE`), after which 238 more submits fail.
Two of the six black lanes (runs 1 and 7) never trip the preflight at all; their
first failure is `renderer_command_id=45` (`VIRGL_CCMD_COPY_TRANSFER3D`), which
the deliberately narrow rule does not cover.

Per-lane split of preflight rejections versus renderer rejections:

| run | preflight (first seq) | renderer (first seq) |
| --- | --- | --- |
| 3 | 1 (1641) | 239 (2198) |
| 5 | 1 (2990) | 180 (3023) |
| 10 | 1 (3016) | 198 (3030) |
| 17 | 1 (2916) | 199 (2936) |
| 1 | 0 | 186 (3195) |
| 7 | 0 | 185 (2987) |

Conclusion, stated plainly: containing `TRANSFER3D` removes one poison source
but is **not sufficient** for stable-black, and B4 must not be reported as
improved by it. The guest is issuing draws and copies against buffers it never
backed; the host can bound the damage but cannot supply the missing backing.
Next measurement is `COPY_TRANSFER3D` (command 45) and the `DRAW_VBO`
`ENOTRECOVERABLE` path, not another pointer-timing change.

A fail-open in the preflight itself was found while reading this trace and
fixed separately: an earlier transfer naming a resource the device never
registered aborted the entire command walk, hiding later unbacked transfers.

B4 remains **OPEN**.

### Batch 5 follow-up — four structural hypotheses tested and falsified

Re-derived from the retained `virtio-gpu.jsonl` of job
`20260824-115428-72165-14984`. Recorded because each was a plausible cause of
the ctx poison and each is now excluded by measurement.

**1. "The guest never posts `RESOURCE_ATTACH_BACKING`."** False as a general
claim. Backing attaches are plentiful and all succeed: run 3 has 525 and run 1
has 543, every one `OK_NODATA`. Only *specific* resources are skipped.

**2. "Black lanes accumulate more unbacked context resources."** False, and this
was my own first reading of the data. Measuring the peak count of
context-attached-but-unbacked resources over the whole run does separate the
classes (black 237–250, landed 86–124), but that peak is reached *after* the
poison. Measured strictly **before** each lane's first failed submit, the peak
is 112–116 in black lanes against 86–124 in landed lanes — overlapping ranges,
so it is a consequence, not a cause.

**3. "Black lanes have more never-backed constant buffers."** False, and in fact
inverted. Counting resources created as `target=0 bind=64`, context-attached and
never backed: black lanes 21–23, landed lanes 16–32. Landed run 13 carries 32
and presents normally. The guest routinely keeps unbacked constant buffers
alive; that alone is harmless.

**4. "The backing burst skips an id, and skipping is the defect."** False. The
guest does post backing in contiguous id bursts and does skip ids inside them
(the run-3 failure is exactly this shape: 154–159 backed at seq 1635–1640 while
153 is skipped, then seq 1641 submits against 153). But skipped ids inside
bursts are ubiquitous — 127 to 749 per lane — and landed run 19 has 740.

**Host-side counters are clean.** `descriptor_chain_rejected` is **0** in every
lane, so BridgeVM is not silently consuming the missing attach. Queue notifies
(69–70), device status reads, and driver feature negotiation are identical
across classes.

**One exact host-side asymmetry exists and is also a consequence.**
`fence_create` is **65** in all six black lanes and **64** in all fourteen
landed lanes — perfectly and exclusively separating. It is not a cause: the
65th fence lands at seq 5393–5759, always *after* that lane's first failed
submit (1641–3195), on a context created after the poison (run 3: ctx 37
created at seq 5228, fence 2718 at seq 5463, `outcome=parked`). It is the
guest's recovery attempt, and it is useful only as a post-hoc marker.

What remains unexplained is narrow and specific: why the guest submits a draw
or copy naming one particular resource whose backing it never posted, when it
posts backing for that resource's immediate id-neighbours in the same burst and
does so successfully in fourteen other lanes of the same image. Nothing measured
so far indicts the host. The next step is guest-side, not another host-side
counter, and B4 remains **OPEN**.

## Batch 6 — 2026-08-24, widened containment: still FAIL, and it exposed the real gap

t8 job `20260824-154650-73552-15846` at main `c3aa70244968377eb9d4116311bb86cde482ea30`,
the first batch with containment widened from constant buffers to any
`PIPE_BUFFER`. Honest **FAIL**: `landed 11/20 p95_first_changed_ms=540
(limit 250)`. Gate log SHA-256 `4c96a7ee…`, summary SHA-256 `8652c8a1…`.
Six lanes stable black (runs 4, 5, 8, 16, 17, 20), three `invalid-target`,
six `invalid-click-count`.

The widening did what it was written to do — the preflight now fires in every
black lane (exactly one rejection each, against resources 305, 155, 308, 318,
296, and one lane with none) where the previous rule matched only two of four.
**It did not reduce the black-lane count**, which stayed at 6/20. B4 is not
improved.

### The measurement that matters: the poison predates the preflight

In four of the six black lanes the first renderer-side failure is
`VIRGL_CCMD_DRAW_VBO` with `renderer_status=104`. That is `ENOTRECOVERABLE`,
and `vrend_draw_vbo` returns it from a single place — `if (ctx->in_error)` at
`vrend_renderer.c:6027` — *before* validating anything. It means the context was
already erroneous when the draw arrived.

Run 4 settles where that error came from. Between the preflight rejection at
seq 3087 and the `ENOTRECOVERABLE` at seq 3107 there are **zero** ctx-7 submits,
and the preflight never calls the renderer. Across the whole lane, ctx 7's first
non-OK submit *is* seq 3087. So `ctx->in_error` was set by a command inside an
**earlier submit that BridgeVM recorded as `OK_NODATA`**.

That is possible because `virgl_submit_cmd` reports success for the buffer as a
whole; `vrend_report_context_error` sets `ctx->in_error` from 85 call sites
(77 in `vrend_renderer.c`, 8 in `vrend_decode.c`) without failing the submit.
`dispatch_submit` (`submit_dispatch.rs:9-16`) therefore stores a diagnostic only
when `accepted` is false, and the true first fault is invisible.

**Consequence for the containment story:** the unbacked `TRANSFER3D` is not the
originating fault in these lanes — it is an early *visible* symptom of a context
that was already poisoned. Widening the rule could never have fixed the black
lanes, and the earlier framing of this work as "one poison source removed" was
too generous to it. What it does buy is a fail-closed refusal that no longer
depends on the binding, and a sharper question.

Run 20 is the one lane where the renderer failure genuinely precedes the
preflight (`COPY_TRANSFER3D`, seq 2218, 886 seqs earlier), so command 45 remains
uncontained by a rule that only walks command 43.

### Next measurement

Surface the first context error, not the first failed submit: the host needs to
know when `in_error` is set inside an accepted buffer. Until then any further
containment rule is guesswork about a fault we cannot see. B4 remains **OPEN**.

## Batch 7 — 2026-08-24: the containment feature is measured harmful and reverted

t8 job `20260824-182810-45208-23584` at main `3293e5d`, the first batch with the
prefix-preservation fix. **`landed 6/20 p95_first_changed_ms=778 (limit 250)`** —
the worst result of the campaign. Gate log SHA-256 `69c10b01…`, summary
`66168eaf…`. Ten lanes stable black.

### The trend the batch completes

| batch | containment | landed | black lanes | ctx errors |
|---|---|---|---|---|
| `20260823-113151` | none | 13/20 | 0 | 10 |
| `20260824-003132` | none | 15/20 | 4 | 10 |
| `20260824-115428` | constant buffers | 12/20 | 6 | 10 |
| `20260824-154650` | any `PIPE_BUFFER` | 11/20 | 6 | 23 |
| `20260824-182810` | + prefix preserved | **6/20** | **10** | 19 |

Landed counts fall monotonically as the feature is introduced and extended, and
in batch 7 the correlation is total: **every one of the 10 lanes with a preflight
rejection went black, and every one of the 10 lanes without a rejection landed.**

### Verdict

The prefix fix was correct in itself — refusing a whole buffer *did* destroy
object state, and that is now proven — but it did not recover the lanes, so the
premise underneath it is wrong. Refusing the transfer at all is worse than
letting virglrenderer refuse it, and no variant of the refusal has ever improved
a batch. Three measured attempts (narrow, wide, wide+prefix) all made it worse.

The whole containment feature is therefore **reverted**: the preflight, the walk,
the refusal, the backing-state bookkeeping and their tests are removed, restoring
the renderer as the sole judge of an unbacked transfer. This is a revert of my
own work on measured evidence, not a rollback of someone else's regression.

What survives is the knowledge the attempt bought: the originating fault is
`vrend_set_single_sampler_view` reporting `Illegal handle`, visible in each black
lane's `run.log` immediately before the `DRAW_VBO`/`ENOTRECOVERABLE` cascade, and
present in black lanes *before* any containment existed (batch `20260824-003132`,
2 context errors in each black lane, 0 in every landed lane). That is the real
B4 defect and it is untouched by anything in this feature.

B4 remains **OPEN**.

### Batch 8 — the revert is confirmed by measurement

t8 job `20260824-204245-4732-52` at main `36e03ba`, the first batch after the
revert: **`landed 12/20 p95_first_changed_ms=778 (limit 250)`**, seven black
lanes, gate log SHA-256 `7a7a5876…`. The landed count **doubles** against the
6/20 the same code base produced with containment enabled, restoring the
12–15/20 band every pre-containment batch sat in.

| batch | containment | landed | black |
|---|---|---|---|
| `20260823-113151` | none | 13/20 | 0 |
| `20260824-003132` | none | 15/20 | 4 |
| `20260824-115428` | narrow | 12/20 | 6 |
| `20260824-154650` | wide | 11/20 | 6 |
| `20260824-182810` | wide+prefix | 6/20 | 10 |
| `20260824-204245` | **none (reverted)** | **12/20** | 7 |

The revert was argued from a trend; this batch tests it prospectively and the
prediction holds. It does not make B4 pass — 12/20 against a 20/20 gate, p95 778
against 250 — but it removes a regression I introduced and returns the tree to
the honest pre-containment baseline.

### Batch 8 root-cause analysis — the separator is the *context*, not the resource

Analysing the post-revert batch (`20260824-204245-4732-52`, gate log SHA-256
`7a7a5876…`, summary `1e0a78b5…`) against `run.log`, which carries the renderer
context errors the JSONL cannot show.

**Exact 20/20 separator: a lane is black if and only if it has failed submits on
context 7.**

| | lanes | failed submits |
|---|---|---|
| black | 7 | 183–309, **all on ctx 7** |
| landed, no failures | 11 | 0 |
| landed, failures elsewhere | 2 | run 14: 49 on ctx 23; run 15: 3 on ctx 33 |

Runs 14 and 15 are the control that makes this conclusive: they failed submits —
49 of them in run 14 — and still landed, because the failures were not on ctx 7.
Context 7 is the busiest context in every lane (~500 submits, created at seq 99
in all 20) and is the one the desktop composites through.

**The originating fault, from `run.log`:** every black lane's first context error
is `vrend_renderer_transfer_iov: Illegal resource N` on ctx 7. That is the
`VIRGL_CCMD_TRANSFER3D` path inside a submit (`vrend_decode.c:1528`), and the
`Illegal resource` comes from the `check_transfer_iovec` branch
(`vrend_renderer.c:10305-10310`) — the resource exists but has no `iov`.

**The shape is uniform across all seven black lanes:**

```
RESOURCE_CREATE_3D (target=0)  ->  CTX_ATTACH_RESOURCE (ctx 7)  ->  44-62 seqs  ->  TRANSFER3D fails
```

with **no `RESOURCE_ATTACH_BACKING` in between**. Five of the seven resources are
never backed at all; two are backed only later, after the failure.

**What this rules out.** Never-backed `PIPE_BUFFER`s are not by themselves the
defect: landed lanes create them in the same proportion (black 22, landed 21 of
~105 constant buffers per lane), and the count attached to ctx 7 does not
separate the classes either (landed run 19 has nine; several black lanes have
one). Host integrity is clean and identical in both classes:
`descriptor_chain_rejected=0`, zero failed `RESOURCE_ATTACH_BACKING`, and
create/backing ratios overlapping at 0.83–0.88.

So the defect is the guest submitting a transfer against a resource it has not
yet backed, and it only becomes visible when it lands on ctx 7. This is the same
guest-side omission recorded earlier, now localised to the context that matters
and separated from the many harmless occurrences elsewhere.

**Why this does not justify another host-side refusal.** The reverted containment
targeted exactly this shape and made every batch worse (see above). The failing
transfer is a symptom of guest state the host cannot reconstruct; refusing it
does not give the guest its backing. B4 remains **OPEN** and the next step is
guest-side.

### Batch 8, second pass — the guest omits backing for exactly the resource it then uses

Continuing on `20260824-204245-4732-52`. Looking at the commands immediately
before each black lane's first failing submit shows the defect directly. Run 3,
seqs 1528–1540:

```
1528..1539  RESOURCE_ATTACH_BACKING  147 148 149 150 151 152 153 154 156 157 158   all OK_NODATA
1540        SUBMIT_3D ctx=7  first_command=43 (TRANSFER3D)                          ERR_UNSPEC
```

The guest backs a contiguous burst and **skips exactly 155** — the resource the
very next submit transfers against. The same shape appears in five of the seven
black lanes:

| lane | target | contiguous burst backed just before the failure | target in burst |
|---|---|---|---|
| run 3 | 155 | 147–154, 156–158 | no |
| run 5 | 150 | 142–149, 151–155 | no |
| run 10 | 315 | 307–314 | no |
| run 17 | 311 | 303–310 | no |
| run 20 | 301 | 293–300, 302–308 | no |

And the omission is permanent, not merely late: resources 155, 150, 311 and 301
are **never** given backing anywhere in their session, while their immediate
id-neighbours are backed normally (156 at seq 1537, 151 at 1648, 312 at 5051,
302 at 2966).

**This is a guest-driver omission, and it is not a host-side race.** The
create→backing latency distributions are identical across classes (p50 21–23
seqs, p95 554–646, max 4.2–5.1k in both), and the number of ctx-7 `PIPE_BUFFER`s
sitting unbacked more than 40 seqs after creation fully overlaps (black 33–42,
landed 10–41). Landed lanes are exposed to the same windows; black lanes are the
ones where the guest additionally *skipped* a backing and then used the
resource. Host integrity is clean in both classes:
`descriptor_chain_rejected=0`, zero failed `RESOURCE_ATTACH_BACKING`, one
virtqueue, no reordering.

Two black lanes (16, 18) do not show the burst pattern; there the resource had
been backed for earlier incarnations of the same id and the final
create→attach cycle simply never received one.

**The host is exonerated by construction.** Every lane's trace carries a strictly
contiguous sequence numbering with **zero gaps** — run 3 spans seq 1..6715 with
no break, and all 20 lanes are contiguous — so BridgeVM demonstrably processed
every command the guest posted and dropped none. Combined with
`descriptor_chain_rejected=0` and zero failed `RESOURCE_ATTACH_BACKING`, there is
no host-side path by which the missing backing could have been lost in transit:
the command was never sent.

The next step is therefore squarely guest-side: identify why the Windows VirGL
driver omits `RESOURCE_ATTACH_BACKING` for one resource in a burst it otherwise
backs completely. Nothing on the host can supply that backing — the reverted
containment proved that refusing the transfer only makes matters worse.
B4 remains **OPEN**.
