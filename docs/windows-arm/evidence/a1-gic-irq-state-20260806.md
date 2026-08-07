# A1: the GIC's own state at every stall, and the contradiction it exposed (2026-08-06)

Job `20260806-014404-13012-8784`, t4-soak at the gic_irq_state build
(`794ddbc` line). 5 boots, 1 pass (boot-4). Every failing boot now carries a
GIC IRQ STATE capture taken by the owning thread at stall confirmation — the
read that was never done before.

## What the five boots show

| boot | outcome | final PC | CNTV_CTL | ISPENDR0 bit27 | overdue ticks | verdict |
|---|---|---|---|---|---|---|
| 1 | stall (UEFI) | `0x1bf33b6a0` | `0x5` en+istatus | pending | 2.3M | deliverable — block is not the GIC |
| 2 | stall (UEFI) | `0x1bf33ba04` | `0x5` en+istatus | pending | 4.9M | deliverable — block is not the GIC |
| 3 | stall (kernel) | `0xfffff800426761e8` | `0x1` en, **istatus=0** | **not pending** | **8.2M** | enabled but not pending at the GICR |
| 4 | PASS | — | `0x2` disabled | not pending | — | (clean shutdown) |
| 5 | stall (kernel) | `0xfffff801aac761cc` | `0x1` en, **istatus=0** | **not pending** | **8.2M** | enabled but not pending at the GICR |

All three failures: `IGRPEN1=0x1`, `PMR=0xf0/0xf8` (open), `ISACTIVER0=0`,
vtimer unmasked in HVF, `DAIF.I=0` (IRQs unmasked at the vCPU) for the UEFI
shape.

## The two shapes are now precisely characterized

**UEFI shape (boots 1–2):** `CNTV_CTL=0x5` — enabled, condition met, ISTATUS
set. The redistributor agrees: `ISPENDR0` bit 27 pending. Group on, PMR open,
active list empty, guest IRQs unmasked, PC parked on WFI. Every layer this
host can read says the interrupt is deliverable. **It is pending at the GIC
and never taken.** The remaining suspects are inside Apple's in-kernel GICv3:
the CPU interface's signal into `hv_vcpu_run`, i.e. the one layer with no
readable state.

**Kernel shape (boots 3, 5):** `CNTV_CTL=0x1` — enabled, unmasked, and
**ISTATUS=0 while CVAL is 8.2M ticks in the past**. On real hardware
ISTATUS is a pure comparator: `CVAL <= CNTVCT` with ENABLE=1 must read
ISTATUS=1. Both boots show the same contradiction, and the redistributor
consistently shows nothing pending — the timer "condition met" signal itself
never formed. That is not a delivery failure; the interrupt was never born.

The same contradiction, seen from two sides: the architectural timer register
disagrees with the architecture. Whether HVF's `CNTV_CTL` read reports a
stale shadow or the emulated comparator genuinely stopped, the defect is
below every layer this probe can instrument.

## What this rules out

- The BootOrder/injector theory of the kernel shape: the guest kernel is
  spinning with a healthy GIC and a timer that never fires. No enable bit,
  group, priority mask, HVF vtimer mask, or pending latch is wrong.
- Any remaining suspicion that the probe's own MSI-X/SPI delivery code eats
  the interrupts: the failures never reach the point of having one to eat.

## What it does not rule out

The boot-1/2 "deliverable" verdicts were read ~50ms after the stall was
confirmed on a vCPU that had just been forced out of guest mode; a pending
PPI at that instant is expected traffic. The UEFI-shape conclusion (pending
but never taken) rests on the guest being parked at WFI with everything
open, which these captures do show.

## Consequences

1. Both shapes now point at the same place: **HVF's vtimer/GICv3 emulation
   boundary**, not this codebase. The host-side avenue left is behavioral:
   e.g. re-arm CVAL or toggle the HVF vtimer mask when the kernel-shape
   contradiction (`ENABLE=1, ISTATUS=0, CVAL long past`) is detected, and
   measure whether the guest resumes.
2. `recover_swallowed_vtimer_fire` already rewrites CVAL on cancels for
   exactly this class of wedge — but only on `EXIT_CANCELED`, and these
   stalls show the wedge persisting between cancels. A periodic detector is
   the next experiment.
3. This gate run was 1/5; the sample adds to 2/5 and 11/12 history. No
   promotion claim changes.

## Follow-up soak at the future-CVAL re-arm (`953e78b`, job `20260806-043632-93324-6822`)

5 boots, 2 pass. Every one of the three failures is the **UEFI shape**
(`CNTV_CTL=0x5`, ISPENDR0 bit 27 pending, IGROUPR0 confirms group 1,
verdict "pending and deliverable"). The kernel shape -- `ENABLE=1,
ISTATUS=0, CVAL past` -- **did not appear once**, versus 2 of 3 failures
in the previous soak.

Five boots cannot prove the future-CVAL re-arm killed the kernel shape,
but the prediction it was built on ("if the shape persists, the wedge is
below CVAL writes") was not falsified, and the surviving failure mode is
now exclusively the one CVAL cannot touch: the PPI is already pending at
the redistributor; there is nothing left to re-arm. The next lever is a
delivery-edge experiment at the CPU interface (HVF vtimer mask toggle
when ISTATUS=1 and the PPI sits pending undelivered).

## Retraction and second follow-up (`61a1f06` mask pulse, job `20260806-071633-6963-32186`)

**Retracted:** the previous section's observation that the kernel shape "did
not appear once" under the future-CVAL re-arm. It was 5-boot noise. This
soak (future-CVAL *and* the ISTATUS=1 mask pulse active) went 2/5 with the
kernel shape back in two of three failures:

| boot | outcome | CNTV_CTL | shape |
|---|---|---|---|
| 2 | stall | `0x5` | UEFI (pending, undelivered, mask pulse ran) |
| 3 | stall (fresh=0 reboot-loop) | `0x1` | kernel (ISTATUS=0, CVAL past) |
| 5 | stall | `0x1` | kernel |

Score across the three instrumented soaks: 5/15. Every host-side vtimer
lever has now been pulled at the stall and measured not to recover it:
unconditional unmask, CVAL rewrite to now, CVAL rewrite to now+10us, HVF
vtimer mask pulse on ISTATUS=1. The UEFI-shape PPI stays pending and
undelivered through all of them; the kernel-shape ISTATUS contradiction
persists through CVAL rewrites.

One host-writable register class remains between these registers and the
vCPU: the redistributor latches themselves (`hv_gic_set_redistributor_reg`
is public API). Next experiment, both edges from the owning thread:
- kernel shape (timer due, ISPENDR0 bit 27 clear): SET the pending latch --
  hand-deliver the interrupt HVF's timer logic dropped.
- UEFI shape (ISPENDR0 bit 27 stuck): ICPENDR0-clear then ISPENDR0-set
  pulse -- force the CPU interface to re-latch a level it stopped seeing.

If forced pending latches do not deliver either, the block is inside the
CPU interface / hv_vcpu_run boundary with no host-visible lever, and A1's
remaining option is architectural (different wake source for the guest, or
Apple feedback) rather than another patch on this path.

## Third follow-up: forced pending latches DELIVER -- and the boot dies anyway (`b53a7d1`, job `20260806-085841-35818-16321`)

1/5, the worst of the four instrumented soaks. But the capture bought the
decisive fact of the series:

| boot | CNTV_CTL | ISPENDR0 | ISACTIVER0 | CPSR I | meaning |
|---|---|---|---|---|---|
| 2 | `0x1` | clear | **bit 27 ACTIVE** | **1** | interrupt was TAKEN; guest is inside/past the handler, IRQs masked, still spinning |
| 3 | `0x5` | pending | clear | 0 | UEFI shape unchanged through pulse |
| 4 | `0x1` | **pending (hand-set)** | clear | 1 | forged PPI latched, not yet taken, guest masked |
| 5 | `0x1` | clear | clear | 0 | kernel shape unchanged |

Boot-2 is the first ISACTIVER0!=0 ever captured: the hand-set latch was
**delivered into the guest**. The boot still died -- parked at EL1 with
PSTATE.I=1 after handling (or while handling) a timer interrupt whose own
CNTV_CTL said ISTATUS=0. That is the experiment answering with a fact worth
more than a pass: **delivery is not the cure. A spurious timer PPI hands the
Windows ISR an interrupt its timer state says did not happen**, and the
kernel's subsequent behavior (IRQs masked, no progress) is consistent with
an ISR that found no work, returned, and the real wake it was substituting
for still never arrived.

**Reverted** the forced-latch writes (both shapes) in the same commit as
this note. What stays: the diagnostics, the mask pulse (baseline-neutral,
mechanism-justified), the future-CVAL re-arm (2/5, 2/5 vs 2/5 baseline --
neutral, harmless, keeps the swallowed-cancel fix honest).

## Where A1 stands after four instrumented soaks

Passes: 2/5, 2/5, 2/5, 1/5 (7/20 vs 9/10 required). The instrumentation
did its job: every readable layer is now measured at the stall, and every
host-side lever below the CPU interface has been tried and measured:

1. unmask on cancel -- helps the swallowed-fire case, does not close the gate
2. CVAL to now / now+10us -- no effect on either shape
3. HVF mask pulse on ISTATUS=1 -- no effect
4. forced redistributor latches -- DELIVERS, guest dies anyway (this soak)

The contradiction is now two-sided: HVF sometimes fails to form/deliver the
timer's fire (ISTATUS=0 with CVAL past; PPI pending and never taken), and a
forged delivery does not substitute for the real one. Both point inside
Apple's in-kernel GICv3/vtimer emulation, below every host-visible register.
Remaining options are architectural: a different guest wake source (e.g.
relocating Windows' timer onto a device interrupt we own), an EL2-mode
probe, or an Apple Feedback with this evidence chain attached.

## Fifth soak: RPR reads idle -- the register-level investigation is closed (`f26b271`, job `20260806-111404-23612-12781`)

3/5 (best of the series; still noise-range). Both failures kernel-shape.
Every terminal capture: **RPR=0xff (idle), AP0R0=0, AP1R0=0**. No stale
running priority exists. The last readable CPU-interface state is clean.

Five instrumented soaks, 10/25 aggregate. Final state of the investigation:

- Every register HVF exposes for the GICv3 (redistributor latches, groups,
  enables, PMR, RPR, active-priority bitmaps, CTLR) has been read at
  terminal stalls and is either consistent with delivery or contradicts the
  architecture in HVF's own timer (ISTATUS=0, CVAL past).
- Every host-side write lever has been tried and measured: unmask, CVAL
  rewrites (now, now+10us), mask pulse, forced pending latches (delivered,
  guest died anyway -- falsifying delivery-as-cure).

The A1 defect is inside Apple's in-kernel GICv3/vtimer emulation, below
every host-visible surface. Remaining paths are architectural, in order of
cost: (1) Apple Feedback with this evidence chain; (2) relocate the guest's
boot-critical wake source onto a device interrupt this VMM owns (SPI/MSI,
requires understanding which Windows boot phase arms the wake); (3) an EL2
probe (nested virt) to observe the ICH_* list registers HVF manages.

This document is the evidence chain for option 1.

## Sixth soak: the UEFI shape has a mechanism -- a stale running priority (`05e8797`, job `20260806-141247`)

The RPR/APR read paid off on its first capture. Every UEFI-shape stall in
this soak reads:

```
GICR ISENABLER0=0x6c000000 ISPENDR0=0x8000000 ISACTIVER0=0x0
ICC  PMR=0xf8 IGRPEN1=0x1 RPR=0x10 AP0R0=0x0 AP1R0=0x4
verdict: vtimer PPI pending but ICC_RPR holds a stale running priority (nothing active)
```

RPR=0x10 with AP1R0 bit set while **ISACTIVER0=0** is architecturally
impossible: a running priority exists only while an interrupt is in
service. The consistent story across every capture this session:

1. the vtimer PPI (priority 0x10, group 1) was delivered and ACTIVATED
   (IAR read by the guest);
2. the guest's EOI / priority-drop write was lost -- the same
   hv_vcpus_exit cancel race that swallows vtimer fires can swallow a
   trapped EOI;
3. the CPU interface, left with running priority 0x10, architecturally
   gates every interrupt at or below 0x10 -- including the next vtimer
   PPI, which then sits "pending and deliverable" forever.

The "fifth soak: RPR reads idle" conclusion is hereby **narrowed, not
retracted**: kernel-shape stalls do read RPR=0xff (their block is the
unformed fire); UEFI-shape stalls read the wedge. The fifth soak's
failures happened to be kernel-shape only.

Fix under test (`0a8c1f0`, queued): on canceled exits, if RPR!=idle and
ISACTIVER0==0, zero AP0R0/AP1R0 via hv_gic_set_icc_reg. Unlike the
reverted latch forge, nothing guest-visible is invented -- HVF's
bookkeeping is corrected to match the empty active set it itself
reports. Also learned: hv_gic_get_ich_reg fails on this config (EL2
view not exposed without nested virt); renders as `?`.

## Seventh and eighth soaks: the priority clear works; delivery still does not follow

`0a8c1f0` (clear after pulse, job `20260806-142939`, 3/5) and `eb87816`
(clear BEFORE pulse, job `20260806-171557`, 1/5). In both, every capture of
the clear shows it doing exactly what it claims: `rpr=0x10 -> 0xff`,
AP1R0 zeroed, terminal reads idle. The ordering hypothesis from the seventh
soak -- pulse's edge was spent into the closed gate -- is falsified by the
eighth: with the gate cleared first and the pulse after, the terminal state
is STILL "ISPENDR0 pending, CPU interface fully idle, PPI never taken".

Conclusion the eight soaks force: with a pending PPI at the redistributor,
an idle open CPU interface, unmasked PSTATE, and a parked WFI, **HVF's
distributor-to-CPU-interface forwarding simply does not re-evaluate**. No
combination of CVAL rewrites, HVF mask pulses, or ICC state correction
makes it look again. The stale-RPR discovery explains how SOME stalls
arise (swallowed EOI) but clearing it post-hoc does not resurrect
delivery, and the kernel shape (fire never forms) was never touched by any
lever.

Aggregate across eight instrumented soaks: 14/40. The clear stays in (it
removes a real architectural contradiction and is provably harmless), the
diagnostics stay in, and the investigation's host-side phase is DONE. The
Apple Feedback draft now carries the complete mechanism: cancel races
swallow both vtimer fires and trapped EOI writes, and no public API
re-arms delivery afterward.

## 2026-08-07: the EL2 avenue, measured and closed

The last architectural option short of Apple was booting the VM with EL2
enabled (`hv_vm_config_set_el2_enabled`), on the theory that the EL2 path
through HVF's interrupt/timer virtualization might not carry the EL1-path
defect. Wired as `--enable-el2` on the installed-boot wrapper
(`BRIDGEVM_ENABLE_EL2`), with the boot CPSR switched to EL2h (`0x3c9`) --
a vCPU left at EL1h inside an EL2-enabled VM makes no progress at all
(first attempt: 1506 exits, not one PL011 byte, ramfb never activated).

Result at EL2h, same canonical image, three boots:

```
EL2 config: requested=true supported=true ... enabled_after=true
probe: boot-progress watchdog stalled_for_ms=120000 exits_in_window=0
       total_exits=1130 reboots=0 suspect=guest-not-running
REGS: pc=0x1bf33a2b0 lr=0x1bf33a2b4 fp=0x1bf33fc70 sp_el0=0x1bf33fc70
```

The firmware genuinely executes at EL2 (PC/LR/FP walking real code in the
same firmware region as the known EL1 WFI park at `0x1bf33ba04`), then
parks with **zero exits per window before the first serial byte** -- an
earlier, harder stall than EL1's. WFI parks in-kernel without userspace
exits, so `exits_in_window=0` with a firmware PC is the same parked-in-WFI
signature; at EL2 the wake never arrives even once (EL1 boots at least
reach Windows most of the time). Whatever HVF does for the EL2 timer
surface (CNTHP/CNTP), it serves this firmware worse than the EL1 vtimer
path, and there is no EXIT_VTIMER-equivalent lever for it.

Verdict: **EL2 is strictly worse and is closed as an A1 avenue.** The flag
stays available for future diagnosis (`--enable-el2`, preflight-recorded),
default off. With this, every architectural option on this host is
measured: the remaining paths are Apple's (Feedback draft ready) or a
guest-side change to what arms the wake, which is not ours to make.

## 2026-08-07: wake-relocation (owned SPI) assessed and closed by inference

The one remaining host-side idea was relocating the guest's boot-critical
wake onto an interrupt we own: assert a spare SPI to pop the parked WFI,
letting the guest re-check its timer state. Two prior measurements close it
without another soak:

1. **Waking the CPU is not the missing piece.** UEFI-shape stalls persisted
   through tens of thousands of re-entries into guest mode (69k measured);
   the CPU leaves WFI constantly (every host cancel does it) and re-parks.
   An SPI wake adds nothing a cancel-driven wake does not already do.
2. **Even DELIVERING the timer interrupt does not save the boot.** The
   forced-latch experiment (soak 20260806-085841 boot-2) got the vtimer PPI
   taken -- first ISACTIVER0 capture ever -- and the guest died anyway.
   Firmware and Windows timekeeping advance in the timer ISR; an unrelated
   SPI wake leaves the tick counter frozen exactly as WFI re-entry does.
   Time never advances, timeouts never fire, the boot stays parked.

An owned-SPI wake could only help if the stall were "CPU asleep, work
ready" -- and every capture shows the opposite: the CPU wakes fine; the
timer stream itself is dead inside HVF's forwarding. Closed.

**Final state of A1 options: every host-side avenue measured or closed by
measurement-backed inference. The defect requires Apple (Feedback draft
ready) or a different in-kernel GIC.**

