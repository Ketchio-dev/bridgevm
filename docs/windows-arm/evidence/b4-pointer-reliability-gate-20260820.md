# B4: the click-reliability acceptance gate, fixed before any fix (2026-08-20)

## Why this document exists

The 2026-08-17 batch (`b4-pointer-batch-20260817.md`) measured the defect —
HID fired 10/10, guest reacted 1/10 — and killed the pacing hypothesis. It
also proved something structural: **every host-visible counter is identical
between a landed and a lost click**. Doorbells, transfer rings, event rings,
interrupts — no separator. Any host-side code change made now would be a guess,
and per `AGENTS.md` a guess may not precede measurement. So this document does
the two things that can be done honestly today: it fixes the acceptance gate
before any fix attempt, and it lands the guest-side instrument the next
measurement needs.

## The gate (threshold, fixed 2026-08-20)

`scripts/verify-pointer-click-reliability.sh`, wrapped by Studio tier
`t8-pointer-reliability` (`scripts/live-gates/run-pointer-reliability-tier.sh`),
closes B4 only when all of the following hold on one sealed batch:

- N=20 independent APFS clones of the sealed image, each with its own vars;
- host injection fires 20/20 (`xHCI pointer-input injection ... fired`);
- guest input-stack consumption 20/20: the in-guest probe reports at least one
  `BVPTR press` **and** one `BVPTR release` per run;
- zero stuck buttons (`BVPTR summary ... stuck=0` in every run);
- visible reaction 20/20: the pointer-input framebuffer checkpoint changes
  from its pre-click baseline;
- first-visible-change p95 <= 250 ms.

Threshold direction is one-way: N may rise, the latency limit may fall, and
neither may move the other way. A local run of the script is a candidate
filter; only a queue receipt at the stated N closes the criterion. After the
core gate passes, the regression set (right click, drag, wheel, coordinate
stability across resolutions and DPI scales, keyboard+pointer concurrency)
gets its own gates; they are out of scope for the core criterion and listed in
the audit backlog.

## The instrument

`scripts/win-assets/bv-pointer-capture.ps1` (CRLF, delivered over the agent
share, launched via `powershell -File`) polls `GetAsyncKeyState(VK_LBUTTON)`
and prints one `BVPTR press/release` line per button transition with cursor
position and foreground window, then a `BVPTR summary` line. It is
deliberately callback-free: native enumeration/hook callbacks hang the PS
5.1/ARM64 agent (measured live, job 20260820-043633, WINLIST).

This splits the loss into the two classes the 2026-08-17 batch could not
distinguish:

| observation | meaning |
| --- | --- |
| `BVPTR press` seen, no framebuffer change | the input stack consumed the report; the loss is foreground routing or application handling |
| no `BVPTR` transition at all | the loss is at or below the HID class driver; next instrument is the xHCI transfer-ring completion state for the exact TRB the button report rode |

Either outcome falsifies about half of the open hypothesis list
(transfer-ring dequeue/cycle state, event TRB completion, interrupter routing,
press/release coalescing, guest same-state suppression, endpoint interval
semantics, ordering expectations of the Windows HID stack); the batch that
runs this probe is the first measurement that can separate them.

## Parser correctness

The gate's run-log parser is self-testable offline
(`verify-pointer-click-reliability.sh --selftest`): one fixture reproduces a
landed click (CRLF guest lines included, per the repository's guest-log rule)
and one reproduces the 2026-08-17 lost-click shape; the parser must classify
both correctly or the script refuses to run a batch.

## Status

B4 stays **OPEN**. Two complete batches have run against this fixed gate and
both failed 0/20: job `20260820-134138-21205-18560` first localized the loss
below the posted host event, and correlated job
`20260821-003915-18894-18703` proved 60/60 DCI5 reports were 6/6 Success while
ERDP remained at the first event in every run. Diagnostic job
`20260821-040346-74328-24747` then proved the DCI5 MSI-X messages reached the
host GIC successfully. The resulting EHB repeat-notification model defect is
fixed and unit-tested, but none of those failed or cancelled jobs is closure
evidence. Only a fresh exact-SHA 20/20 receipt with p95 <=250 ms closes B4.
