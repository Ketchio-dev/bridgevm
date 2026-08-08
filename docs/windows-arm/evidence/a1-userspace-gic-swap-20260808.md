# A1 fix: userspace GICv3 swap — 10/10 installed boots, READY + clean SYSTEM_OFF

## What was built

`crates/bridgevm-hvf/src/userspace_gic/` — a complete userspace GICv3
(distributor with 256 INTIDs + affinity routing, per-CPU redistributors and
CPU interfaces, SGI1R, GICv2m MSI frame, level/edge SPI latching, EOImode,
preemption, 15 unit tests) — wired into the boot probe behind
`BRIDGEVM_USERSPACE_GIC=1` (`usgic_bridge.rs`). In this mode
`hv_gic_create` is never called; vCPU IRQ lines are driven with
`hv_vcpu_set_pending_interrupt` before every `hv_vcpu_run` — exactly the
configuration QEMU-hvf uses (a1-qemu-userspace-gic-control-20260808.md).

## Guest-contract findings (each one parked the boot until fixed)

1. **PFR0.GIC must be advertised**: Apple silicon reports GIC=0 and guests
   then never use ICC_*; setting GIC=1 makes HVF trap the unallocated ICC
   encodings to userspace (QEMU's contract).
2. **GICR_WAKER is storage-only**: EDK2 never programs it; a reset value
   with ProcessorSleep gated all PPIs and parked BdsDxe.
3. **Single security state**: IGROUPR resets all-Group-1 (DS=1). A zero
   reset made every interrupt Group 0 and nothing delivered.
4. **winload calibrates with PMCCNTR_EL0** and `__fastfail`s (BRK #0xf004)
   on a zero delta — return scaled host-timebase ticks.
5. **vtimer PPI is edge-latched at EXIT_VTIMER**: Windows' timer ISR masks
   CNTV_CTL.IMASK *before* reading IAR; modelling the PPI as level tied to
   CNTV_CTL raced that window (ack found nothing, timer stayed masked
   forever). Latch on fire; unmask the host vtimer on EOI/DIR of INTID 27.
6. **CNTPCT_EL0 traps** in this configuration: mach_absolute_time ticks at
   the advertised 24 MHz CNTFRQ, so the host timebase IS the counter.
7. **ntoskrnl brings up SMP through the same paths** — secondary vCPUs
   need the identical GIC MMIO/sysreg/WFI routing.

## Result: 10/10 soak (/Users/insighton/BridgeVM/runs/usgic-soak-135206)

    boot 1..10: BVAGENT READY at 82-87 s, agent-driven shutdown /p /f,
    PSCI SYSTEM_OFF, clean probe exit — every boot, no exception

Baseline on the same image class with Apple's in-kernel GIC: **14/40**,
every failure in the two hv_gic stall shapes. The A1 mechanism (lost
trapped-EOI / swallowed fire inside hv_gic) is **absent by construction**:
EOI is handled in our own code and pending state cannot be lost.

## Scope

- 2D/agent/NVMe/xHCI/serial path proven end to end (this soak).
- 3D (venus MSI-X under load), A15 reset cycles, and the A1 criterion's
  fresh-firstboot gate run next — tracked in the registry.
- In-kernel-GIC remains the default until those complete; the swap is one
  env flag away and involves zero Apple-side dependencies.
