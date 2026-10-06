//! Valid enabled-group setup shared by the userspace GIC fixtures.

use super::*;

pub(super) fn enable_spi(gic: &mut UserspaceGic, intid: u32, cpu_route: u64) {
    let intid = intid as usize;
    // GICD_CTLR: enable group1.
    gic.mmio(machine::GIC_DIST.base + GICD_CTLR, 4, Some(2));
    // Group1, enabled, priority 0xa0, routed.
    let (reg, _) = (intid / 32, intid % 32);
    gic.mmio(
        machine::GIC_DIST.base + GICD_IGROUPR + (reg as u64) * 4,
        4,
        Some(0xffff_ffff),
    );
    gic.mmio(
        machine::GIC_DIST.base + GICD_ISENABLER + (reg as u64) * 4,
        4,
        Some(1 << (intid % 32)),
    );
    gic.mmio(
        machine::GIC_DIST.base + GICD_IPRIORITYR + intid as u64,
        1,
        Some(0xa0),
    );
    gic.mmio(
        machine::GIC_DIST.base + GICD_IROUTER + (intid as u64) * 8,
        8,
        Some(cpu_route),
    );
}

pub(super) fn wake_cpu(gic: &mut UserspaceGic, cpu: usize) {
    let base = machine::GIC_REDIST.base + machine::GICV3_REDIST_STRIDE * cpu as u64;
    gic.mmio(base + GICR_WAKER, 4, Some(0));
    gic.mmio(machine::GIC_DIST.base + GICD_CTLR, 4, Some(2));
    // Unmask at the CPU interface.
    gic.sysreg(cpu, ICC_PMR_EL1, false, 0xff);
    gic.sysreg(cpu, ICC_IGRPEN1_EL1, false, 1);
}

pub(super) fn enable_vtimer_ppi(gic: &mut UserspaceGic, cpu: usize) {
    gic.mmio(machine::GIC_DIST.base + GICD_CTLR, 4, Some(2));
    let base = machine::GIC_REDIST.base + machine::GICV3_REDIST_STRIDE * cpu as u64;
    gic.mmio(base + GICR_IGROUPR0, 4, Some(0xffff_ffff));
    gic.mmio(base + GICR_ISENABLER0, 4, Some(1 << VTIMER_INTID));
    gic.mmio(
        base + GICR_IPRIORITYR + u64::from(VTIMER_INTID),
        1,
        Some(0x80),
    );
}
