//! Global Group 1 disable masks pending private interrupts without losing them.

use super::*;

fn redist(cpu: usize) -> u64 {
    machine::GIC_REDIST.base + machine::GICV3_REDIST_STRIDE * cpu as u64
}

fn read_reg(gic: &mut UserspaceGic, cpu: usize, reg: u16) -> u64 {
    gic.sysreg(cpu, reg, true, 0).unwrap().value
}

fn pending_private(intid: u32, cpu: usize) -> UserspaceGic {
    let mut gic = UserspaceGic::new(2);
    wake_cpu(&mut gic, cpu);
    gic.mmio(redist(cpu) + GICR_ISENABLER0, 4, Some(1 << intid));
    gic.mmio(
        redist(cpu) + GICR_IPRIORITYR + u64::from(intid),
        1,
        Some(0x80),
    );
    if intid == VTIMER_INTID {
        assert_eq!(gic.set_vtimer_ppi(cpu, true), 1 << cpu);
    } else if intid < 16 {
        assert_eq!(
            gic.sysreg(
                0,
                ICC_SGI1R_EL1,
                false,
                (u64::from(intid) << 24) | (1 << cpu)
            )
            .unwrap()
            .kick_mask,
            1 << cpu
        );
    } else {
        gic.mmio(redist(cpu) + GICR_ISPENDR0, 4, Some(1 << intid));
    }
    assert!(gic.line_asserted(cpu));
    assert_eq!(read_reg(&mut gic, cpu, ICC_HPPIR1_EL1), u64::from(intid));
    gic
}

fn disable_preserves_pending(intid: u32, cpu: usize, disabled_ctlr: u64) {
    let mut gic = pending_private(intid, cpu);
    assert_eq!(
        gic.mmio(machine::GIC_DIST.base + GICD_CTLR, 4, Some(disabled_ctlr))
            .kick_mask,
        3
    );
    assert!(
        !gic.line_asserted(cpu),
        "globally disabled INTID {intid} still signals"
    );
    assert_eq!(
        read_reg(&mut gic, cpu, ICC_HPPIR1_EL1),
        u64::from(SPURIOUS_INTID)
    );
    assert_eq!(
        read_reg(&mut gic, cpu, ICC_IAR1_EL1),
        u64::from(SPURIOUS_INTID)
    );
    for alias in [GICR_ISPENDR0, GICR_ICPENDR0] {
        assert_eq!(gic.mmio(redist(cpu) + alias, 4, None).value, 1 << intid);
    }
    assert_eq!(gic.mmio(redist(cpu) + GICR_ISACTIVER0, 4, None).value, 0);
    assert_eq!(
        gic.mmio(machine::GIC_DIST.base + GICD_CTLR, 4, Some(2))
            .kick_mask,
        3
    );
    assert!(gic.line_asserted(cpu));
    assert_eq!(read_reg(&mut gic, cpu, ICC_IAR1_EL1), u64::from(intid));
    assert_eq!(gic.mmio(redist(cpu) + GICR_ISPENDR0, 4, None).value, 0);
    gic.sysreg(cpu, ICC_EOIR1_EL1, false, u64::from(intid));
    assert!(!gic.line_asserted(cpu));
}

#[test]
fn global_disable_retrieves_sgi_and_reenable_delivers_it() {
    for cpu in 0..2 {
        for ctlr in [0, 1] {
            disable_preserves_pending(5, cpu, ctlr);
        }
    }
}

#[test]
fn global_disable_retrieves_timer_ppi_and_reenable_delivers_it() {
    for cpu in 0..2 {
        for ctlr in [0, 1] {
            disable_preserves_pending(VTIMER_INTID, cpu, ctlr);
        }
    }
}

#[test]
fn global_disable_retrieves_software_ppi_and_reenable_delivers_it() {
    for cpu in 0..2 {
        disable_preserves_pending(31, cpu, 0);
    }
}

#[test]
fn global_disable_keeps_existing_spi_gate_and_asserted_input() {
    let mut gic = UserspaceGic::new(2);
    wake_cpu(&mut gic, 1);
    enable_spi(&mut gic, 40, 1);
    assert_eq!(gic.set_spi(40, true), 2);
    gic.mmio(machine::GIC_DIST.base + GICD_CTLR, 4, Some(0));
    assert!(!gic.line_asserted(1));
    assert_eq!(
        read_reg(&mut gic, 1, ICC_IAR1_EL1),
        u64::from(SPURIOUS_INTID)
    );
    assert_eq!(
        gic.mmio(machine::GIC_DIST.base + GICD_ISPENDR + 4, 4, None)
            .value,
        1 << 8
    );
    gic.mmio(machine::GIC_DIST.base + GICD_CTLR, 4, Some(2));
    assert_eq!(read_reg(&mut gic, 1, ICC_IAR1_EL1), 40);
    gic.set_spi(40, false);
    gic.sysreg(1, ICC_EOIR1_EL1, false, 40);
    assert!(!gic.line_asserted(1));
}

#[test]
fn global_reenable_preserves_individual_and_cpu_interface_masks() {
    for cpu_mask in [false, true] {
        let mut gic = pending_private(5, 1);
        if cpu_mask {
            gic.sysreg(1, ICC_IGRPEN1_EL1, false, 0);
        } else {
            gic.mmio(redist(1) + GICR_ICENABLER0, 4, Some(1 << 5));
        }
        gic.mmio(machine::GIC_DIST.base + GICD_CTLR, 4, Some(0));
        gic.mmio(machine::GIC_DIST.base + GICD_CTLR, 4, Some(2));
        assert!(!gic.line_asserted(1));
        assert_eq!(
            read_reg(&mut gic, 1, ICC_IAR1_EL1),
            u64::from(SPURIOUS_INTID)
        );
        assert_eq!(gic.mmio(redist(1) + GICR_ISPENDR0, 4, None).value, 1 << 5);
        if cpu_mask {
            gic.sysreg(1, ICC_IGRPEN1_EL1, false, 1);
        } else {
            gic.mmio(redist(1) + GICR_ISENABLER0, 4, Some(1 << 5));
        }
        assert_eq!(read_reg(&mut gic, 1, ICC_IAR1_EL1), 5);
    }
}

#[test]
fn global_disable_leaves_active_interrupt_available_for_split_eoi() {
    let mut gic = pending_private(5, 1);
    gic.sysreg(1, ICC_CTLR_EL1, false, ICC_CTLR_EOIMODE);
    assert_eq!(read_reg(&mut gic, 1, ICC_IAR1_EL1), 5);
    // A second SGI while enabled is pending behind the active first one.
    gic.sysreg(0, ICC_SGI1R_EL1, false, (5 << 24) | 2);
    gic.mmio(machine::GIC_DIST.base + GICD_CTLR, 4, Some(0));
    assert_eq!(read_reg(&mut gic, 1, ICC_RPR_EL1), 0x80);
    assert_eq!(gic.mmio(redist(1) + GICR_ISACTIVER0, 4, None).value, 1 << 5);
    gic.sysreg(1, ICC_EOIR1_EL1, false, 5);
    assert_eq!(read_reg(&mut gic, 1, ICC_RPR_EL1), 0xff);
    assert_eq!(gic.mmio(redist(1) + GICR_ISACTIVER0, 4, None).value, 1 << 5);
    gic.sysreg(1, ICC_DIR_EL1, false, 5);
    assert!(!gic.line_asserted(1));
    assert_eq!(gic.mmio(redist(1) + GICR_ISACTIVER0, 4, None).value, 0);
    assert_eq!(gic.mmio(redist(1) + GICR_ISPENDR0, 4, None).value, 1 << 5);
    gic.mmio(machine::GIC_DIST.base + GICD_CTLR, 4, Some(2));
    assert_eq!(read_reg(&mut gic, 1, ICC_IAR1_EL1), 5);
}
