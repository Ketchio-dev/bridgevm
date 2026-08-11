# A1: Apple Feedback draft — vtimer PPI lost/undelivered under in-kernel GICv3

Status: draft, ready to file. The owner files it (requires an Apple account;
this workspace deliberately has none). Everything below is reproducible from
this repository at `3c900a4` with the referenced job receipts.

## One-paragraph summary

On macOS 26.5.2 (Mac16,9, M4 Max), a Hypervisor.framework VMM using the
in-kernel GICv3 (`hv_gic_create`) and the per-vCPU virtual timer
(`hv_vcpu_set_vtimer_mask` / EXIT_VTIMER) loses the guest's timer wake
during Windows 11 ARM64 boot in ~60% of boots. Two observable end states,
both captured with owning-thread register reads at the stall:

1. **Pending, never taken.** `CNTV_CTL=0x5` (ENABLE+ISTATUS), GICR
   `ISPENDR0` bit 27 set, `IGROUPR0` bit 27 = group 1, `IGRPEN1=1`,
   `PMR=0xf8`, `ISACTIVER0=0`, `RPR=0xff`, `AP0R0/AP1R0=0`, guest
   `PSTATE.I=0`, PC parked on a WFI in UEFI. Every architecturally readable
   gate is open; the PPI is never delivered through tens of thousands of
   subsequent `hv_vcpu_run` entries.
2. **Fire never forms.** `CNTV_CTL=0x1` (ENABLE, IMASK=0) with `CVAL`
   millions of ticks in the past and **ISTATUS=0** — on the architecture
   ISTATUS is a comparator (`CVAL <= CNTVCT` with ENABLE must read 1) — and
   `ISPENDR0` bit 27 clear. The interrupt is never generated.

## Why we believe the defect is inside HVF

Every host-side lever was tried at the stall, each measured over a 5-boot
gate (receipts in `~/BridgeVM/live-queue/done/`):

| lever | result |
|---|---|
| `hv_vcpu_set_vtimer_mask(false)` on canceled exits | helps the swallowed-fire race; gate stays ~2/5 |
| rewrite `CVAL` to guest-now | no effect on either shape |
| rewrite `CVAL` to guest-now + 10µs | no effect |
| pulse `hv_vcpu_set_vtimer_mask(true→false)` when ISTATUS=1 | no effect |
| force `GICR_ISPENDR0` bit 27 via `hv_gic_set_redistributor_reg` | **delivers** (first `ISACTIVER0` capture) — guest takes a spurious timer interrupt whose own `CNTV_CTL.ISTATUS=0`, finds no work, and the boot dies anyway |

The last row is the decisive one: delivery machinery works when handed a
latched PPI, so the loss is upstream — between HVF's vtimer expiry logic
and the redistributor latch — and correlates with `hv_vcpus_exit`
cancellation racing an in-flight fire (measured 21/21 correlation between
surplus canceled exits and this stall class).

## Reproduction

- VMM: `crates/bridgevm-hvf/examples/hvf_gic_boot_probe` at `3c900a4`
  (Apple in-kernel GIC, 4 vCPUs, Windows 11 ARM64 24H2 guest).
- Driver: `scripts/p1-boot-gate.sh --boots 5` (or the t4-soak queue tier).
- Measured across five 5-boot instrumented campaigns: 10/25 boots reach
  stage4; every failure is one of the two shapes above.
- Full register captures: `docs/windows-arm/evidence/a1-gic-irq-state-20260806.md`;
  job IDs `20260806-014404`, `-043632`, `-071633`, `-085841`, `-111404`.

## Additional mechanism found after filing draft (2026-08-06, soaks 6-8)

The ICC running-priority read exposed `RPR=0x10`, `AP1R0=0x4` while
`GICR_ISACTIVER0=0`. This draft originally called that a running priority with
no interrupt in service and attributed it to a lost EOI. **Correction
(2026-08-11): the predicate was incomplete.** `GICR_ISACTIVER0` covers only
banked SGIs/PPIs, not distributor shared SPI/MSI active state. Zeroing
AP0R0/AP1R0 did verifiably change the readback to idle and did not resume the
pending PPI, but it was not proven safe. In exact run
`20260811-140847-85685-23616`, after such a clear, Hypervisor.framework
intentionally raised SIGTRAP because guest `EOIR1 0x80` did not name its
highest-priority active interrupt (`report_fixme_and_trap` →
`EmulatedVgic::write_end_of_interrupt_register`). The APR mutation is removed;
the historical observation remains, but the lost-EOI mechanism claim is
retracted. The still-supported conclusion is that the in-kernel GIC/vtimer path
has measured forwarding failures under cancellation and no demonstrated public
re-arm mechanism.

## Avenues we measured and closed (2026-08-07)

- **EL2**: creating the VM with `hv_vm_config_set_el2_enabled(true)` and
  booting at EL2h is strictly worse -- the same firmware parks before its
  first serial byte with zero exits per window (vs EL1 reaching Windows most
  boots). The EL2 timer surface has no EXIT_VTIMER-equivalent lever.
- **Wake relocation onto an owned SPI**: closed by two prior measurements.
  Waking the CPU is not the gap (stalls persist through 69k guest re-entries;
  every host cancel already wakes the vCPU), and even *delivered* timer
  interrupts did not save the boot (hand-set pending latch: PPI taken,
  ISACTIVER0 bit 27 observed, guest died anyway). Time advances in the timer
  ISR; an unrelated SPI wake leaves the tick counter frozen.
- **Cancel-rate reduction**: an 8x lower host-wake cadence during the failure
  window changed nothing (3/3 identical parks) -- the loss is armed within
  seconds by guest activity, not accumulated by host cancel volume.
- **Stale-RPR values are load-dependent**: with the virgl GPU active the lost
  EOI leaves `RPR=0x60` (11 captures) vs `0x10` on the venus/headless path
  (6 captures) -- different interrupt in service when the write is lost, same
  mechanism.

## What we ask

Either (a) a fix for the lost vtimer fire under `hv_vcpus_exit`
cancellation pressure, or (b) documentation of an API contract that makes
the loss avoidable (ordering constraints between `hv_vcpus_exit`,
EXIT_VTIMER and the in-kernel GIC's PPI latch), or (c) a supported way to
re-assert the timer PPI that does not require forging state the guest can
observe as inconsistent.
