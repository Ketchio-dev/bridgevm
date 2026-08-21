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
