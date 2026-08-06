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

