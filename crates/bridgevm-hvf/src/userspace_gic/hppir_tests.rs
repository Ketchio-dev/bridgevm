//! Pending visibility is independent of priority masking and ability to preempt.

use super::*;

fn read(gic: &mut UserspaceGic, cpu: usize, reg: u16) -> u64 {
    gic.sysreg(cpu, reg, true, 0).unwrap().value
}

fn configured(first: u64, second: u64) -> UserspaceGic {
    let mut gic = UserspaceGic::new(2);
    gic.sysreg(1, ICC_BPR1_EL1, false, 4);
    for cpu in 0..2 {
        wake_cpu(&mut gic, cpu);
    }
    assert_eq!(read(&mut gic, 1, ICC_CTLR_EL1) & ((1 << 6) | 1), 0);
    for (intid, priority) in [(60, first), (61, second)] {
        enable_spi(&mut gic, intid, 1);
        gic.mmio(
            machine::GIC_DIST.base + GICD_IPRIORITYR + u64::from(intid),
            1,
            Some(priority),
        );
    }
    gic
}

fn hppir(gic: &mut UserspaceGic, cpu: usize) -> u64 {
    read(gic, cpu, ICC_HPPIR1_EL1)
}

#[test]
fn pmr_masked_spi_remains_visible_and_hppir_does_not_acknowledge() {
    let mut gic = configured(0x80, 0xa0);
    gic.sysreg(1, ICC_PMR_EL1, false, 0x80);
    gic.set_spi(60, true);
    assert!(!gic.line_asserted(1));
    for _ in 0..3 {
        assert_eq!(hppir(&mut gic, 1), 60);
        assert_eq!(read(&mut gic, 1, ICC_IAR1_EL1), u64::from(SPURIOUS_INTID));
    }
    assert_eq!(
        gic.mmio(machine::GIC_DIST.base + GICD_ISACTIVER + 4, 4, None)
            .value,
        0
    );
    assert_eq!(
        gic.mmio(machine::GIC_DIST.base + GICD_ISPENDR + 4, 4, None)
            .value,
        1 << 28
    );
    gic.sysreg(1, ICC_PMR_EL1, false, 0x81);
    assert_eq!(read(&mut gic, 1, ICC_IAR1_EL1), 60);
}

#[test]
fn pmr_masked_sgi_and_ppi_remain_visible() {
    for intid in [5u32, VTIMER_INTID] {
        let mut gic = configured(0x80, 0xa0);
        let base = machine::GIC_REDIST.base + machine::GICV3_REDIST_STRIDE;
        gic.mmio(base + GICR_ISENABLER0, 4, Some(1 << intid));
        gic.mmio(base + GICR_IPRIORITYR + u64::from(intid), 1, Some(0x80));
        if intid == VTIMER_INTID {
            gic.set_vtimer_ppi(1, true);
        } else {
            gic.sysreg(0, ICC_SGI1R_EL1, false, (u64::from(intid) << 24) | 2);
        }
        gic.sysreg(1, ICC_PMR_EL1, false, 0);
        assert_eq!(hppir(&mut gic, 1), u64::from(intid));
        assert!(!gic.line_asserted(1));
        assert_eq!(read(&mut gic, 1, ICC_IAR1_EL1), u64::from(SPURIOUS_INTID));
        assert_eq!(gic.mmio(base + GICR_ISACTIVER0, 4, None).value, 0);
        assert_eq!(gic.mmio(base + GICR_ISPENDR0, 4, None).value, 1 << intid);
    }
}

fn pending_during_active(first: u64, second: u64) {
    let mut gic = configured(first, second);
    gic.set_spi(60, true);
    assert_eq!(read(&mut gic, 1, ICC_IAR1_EL1), 60);
    gic.set_spi(60, false);
    gic.set_spi(61, true);
    let line_before = gic.line_asserted(1);
    let running_before = read(&mut gic, 1, ICC_RPR_EL1);
    for _ in 0..3 {
        assert_eq!(hppir(&mut gic, 1), 61);
    }
    assert_eq!(gic.line_asserted(1), line_before);
    assert_eq!(read(&mut gic, 1, ICC_RPR_EL1), running_before);
    assert_eq!(
        gic.mmio(machine::GIC_DIST.base + GICD_ISACTIVER + 4, 4, None)
            .value,
        1 << 28
    );
    assert_eq!(
        gic.mmio(machine::GIC_DIST.base + GICD_ISPENDR + 4, 4, None)
            .value,
        1 << 29
    );
    gic.sysreg(1, ICC_EOIR1_EL1, false, 60);
    assert_eq!(read(&mut gic, 1, ICC_IAR1_EL1), 61);
}

#[test]
fn same_group_pending_visibility_survives_corrected_preemption() {
    pending_during_active(0x4f, 0x40);
}

#[test]
fn lower_priority_pending_is_visible_behind_active_interrupt() {
    pending_during_active(0x40, 0x50);
}

#[test]
fn cpu_interface_group_disable_hides_pending_interrupt() {
    let mut gic = configured(0x80, 0xa0);
    gic.set_spi(60, true);
    gic.sysreg(1, ICC_IGRPEN1_EL1, false, 0);
    assert_eq!(hppir(&mut gic, 1), u64::from(SPURIOUS_INTID));
    assert!(!gic.line_asserted(1));
    assert_eq!(
        gic.mmio(machine::GIC_DIST.base + GICD_ISPENDR + 4, 4, None)
            .value,
        1 << 28
    );
    gic.sysreg(1, ICC_IGRPEN1_EL1, false, 1);
    assert_eq!(hppir(&mut gic, 1), 60);
}

#[test]
fn distributor_individual_and_interrupt_group_gates_still_apply() {
    for gate in [GICD_CTLR, GICD_ICENABLER + 4, GICD_IGROUPR + 4] {
        let mut gic = configured(0x80, 0xa0);
        gic.set_spi(60, true);
        let value = if gate == GICD_ICENABLER + 4 {
            1 << 28
        } else {
            0
        };
        gic.mmio(machine::GIC_DIST.base + gate, 4, Some(value));
        assert_eq!(hppir(&mut gic, 1), u64::from(SPURIOUS_INTID));
        assert!(!gic.line_asserted(1));
    }
}

#[test]
fn routing_and_active_state_limit_pending_visibility() {
    let mut gic = configured(0x80, 0xa0);
    gic.set_spi(60, true);
    assert_eq!(hppir(&mut gic, 0), u64::from(SPURIOUS_INTID));
    assert_eq!(hppir(&mut gic, 1), 60);
    assert_eq!(read(&mut gic, 1, ICC_IAR1_EL1), 60);
    assert_eq!(hppir(&mut gic, 1), u64::from(SPURIOUS_INTID));
    assert_eq!(
        gic.mmio(machine::GIC_DIST.base + GICD_ISPENDR + 4, 4, None)
            .value,
        1 << 28
    );
    gic.set_spi(60, false);
    gic.sysreg(1, ICC_EOIR1_EL1, false, 60);
    assert_eq!(hppir(&mut gic, 1), u64::from(SPURIOUS_INTID));
}

#[test]
fn masked_pending_order_uses_full_priority_without_side_effects() {
    let mut gic = configured(0x4f, 0x40);
    gic.sysreg(1, ICC_PMR_EL1, false, 0);
    gic.set_spi(60, true);
    gic.set_spi(61, true);
    assert_eq!(hppir(&mut gic, 1), 61);
    assert!(!gic.line_asserted(1));
    assert_eq!(
        gic.mmio(machine::GIC_DIST.base + GICD_ISACTIVER + 4, 4, None)
            .value,
        0
    );
    gic.sysreg(1, ICC_PMR_EL1, false, 0xff);
    assert_eq!(read(&mut gic, 1, ICC_IAR1_EL1), 61);
}

#[test]
fn priority_ff_retains_existing_spurious_readback_policy() {
    let mut gic = configured(0xff, 0xff);
    gic.set_spi(60, true);
    assert_eq!(
        gic.mmio(machine::GIC_DIST.base + GICD_IPRIORITYR + 60, 1, None)
            .value,
        0xff
    );
    assert_eq!(hppir(&mut gic, 1), u64::from(SPURIOUS_INTID));
    assert_eq!(read(&mut gic, 1, ICC_IAR1_EL1), u64::from(SPURIOUS_INTID));
    assert!(!gic.line_asserted(1));
    assert_eq!(
        gic.mmio(machine::GIC_DIST.base + GICD_ISPENDR + 4, 4, None)
            .value,
        1 << 28
    );
}
