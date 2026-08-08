# A1 control experiment: same host, same image, userspace GIC — 10/10 boots

## Question

Is the A1 boot-park a property of this host's Hypervisor.framework, of the
guest image, or specifically of Apple's **in-kernel GICv3** (`hv_gic_*`)?

QEMU 11.0.2's hvf accelerator on this exact machine answers it: QEMU links
against 37 `hv_*` symbols and **zero** `hv_gic_*` symbols (verified with
`nm -u`). It emulates the entire GICv3 (distributor, redistributor, CPU
interface) in userspace and tells HVF only "an IRQ line is (de)asserted" via
`hv_vcpu_set_pending_interrupt` before each `hv_vcpu_run`. The vtimer is
consumed as plain vtimer exits + `hv_vcpu_set_vtimer_mask` — the kernel GIC's
trapped-EOI path, the lost-priority-drop path, and the swallowed-fire latch
simply do not exist in that configuration.

## Experiment

`scripts/qemu-a1-control.py` (committed): boots a `cp -c` clone of
`canonical-attach-resident-20260731` under
`qemu-system-aarch64 -M virt,accel=hvf,gic-version=3 -cpu host -smp 4 -m 6144`
with NVMe, ramfb, xhci kbd/tablet and a virtio-serial port wired to the
image's own bvagent. Success requires the guest's OWN agent to reach
`READY` (same oracle as our stack), then `RUN shutdown /p /f` for a clean
power-off between boots. 10 sequential cold boots.

## Result (run `/Users/insighton/BridgeVM/runs/qemu-a1-control-021802`)

    boot 1..10: READY at 18-24 s, every boot, no exception
    RESULT: 10/10 READY under QEMU-hvf userspace-GIC

Against our stack's measured baseline on the same image class: **14/40**
(35%) with every failure in the two in-kernel-GIC stall shapes (stale
RPR gating all delivery / fire never forming).

## Conclusion

- The host kernel's HVF, the vtimer surface, and the guest image are all
  **exonerated**: 4 vCPUs, EL1, same Windows, same firmware family boot
  10/10 when the GIC lives in userspace.
- A1 is therefore a defect (or an unusable-for-us contract) **specific to
  the in-kernel GICv3** — consistent with every register capture
  (architecturally impossible RPR=0x10/0x60-with-empty-active-set states
  inside `hv_gic`).
- This gives A1 a host-side exit that does not wait on Apple: **emulate the
  GICv3 in userspace and drive vCPU IRQ lines with
  `hv_vcpu_set_pending_interrupt`**, exactly like QEMU. The codebase
  already contains a userspace GICv3 device model used by the firmware
  bringup path (`probe_mmio/gic_distributor.rs`, `gic_redistributor.rs`,
  `gic_cpu_interface.rs`, `platform/apple/firmware_irq.rs`) — the probe's
  full-VM path is what standardized on `hv_gic_create`.
- Scope of the swap for the full device stack: SGI/IPI handling for 4 vCPUs,
  PPI (vtimer intid 27) latching, SPI level sources, and MSI-X (currently
  `hv_gic_send_msi`) all have to route through the userspace distributor,
  and every vCPU needs the pending-line refresh before `hv_vcpu_run`.
  Feedback to Apple stays filed regardless.
