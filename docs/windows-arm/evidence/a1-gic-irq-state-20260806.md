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

