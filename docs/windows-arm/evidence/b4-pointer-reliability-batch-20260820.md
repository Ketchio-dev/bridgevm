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
MSI-X path. B4 remains OPEN until a fresh fixed 20-run t8 receipt passes 20/20
with p95 <=250 ms.
