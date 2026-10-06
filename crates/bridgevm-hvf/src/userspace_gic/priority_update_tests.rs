use super::*;

#[test]
fn distributor_priority_updates_kick_only_changed_remote_lines() {
    let mut gic = UserspaceGic::new(3);
    for cpu in 1..3 {
        wake_cpu(&mut gic, cpu);
        gic.sysreg(cpu, ICC_PMR_EL1, false, 0x80);
        enable_spi(&mut gic, 39 + cpu as u32, cpu as u64);
        assert_eq!(gic.set_spi(39 + cpu as u32, true), 0);
        assert!(!gic.line_asserted(cpu));
    }
    let address = machine::GIC_DIST.base + GICD_IPRIORITYR + 40;
    let promoted = gic.mmio(address, 2, Some(0x4040));
    assert_eq!(promoted.kick_mask, 0b110);
    assert!(gic.line_asserted(1));
    assert!(gic.line_asserted(2));
    assert!(!gic.line_asserted(0));
    assert_eq!(gic.mmio(address, 2, None).value, 0x4040);
    assert_eq!(gic.mmio(address, 2, None).kick_mask, 0);
    assert_eq!(gic.mmio(address, 2, Some(0x4040)).kick_mask, 0);
    let masked = gic.mmio(address, 1, Some(0xa0));
    assert_eq!(masked.kick_mask, 1 << 1);
    assert!(!gic.line_asserted(1));
    assert!(gic.line_asserted(2));
    assert_eq!(gic.mmio(address, 1, Some(0xb0)).kick_mask, 0);
    assert_eq!(gic.sysreg(2, ICC_IAR1_EL1, true, 0).unwrap().value, 41);
    assert_eq!(gic.mmio(address + 1, 1, Some(0x20)).kick_mask, 0);
    assert!(!gic.line_asserted(2), "active SPI stays in service");
}

#[test]
fn redistributor_priority_updates_kick_remote_private_interrupt() {
    let mut gic = UserspaceGic::new(2);
    wake_cpu(&mut gic, 1);
    gic.sysreg(1, ICC_PMR_EL1, false, 0x80);
    let base = machine::GIC_REDIST.base + machine::GICV3_REDIST_STRIDE;
    gic.mmio(base + GICR_ISENABLER0, 4, Some(1 << 3));
    let address = base + GICR_IPRIORITYR + 3;
    gic.mmio(address, 1, Some(0xa0));
    assert_eq!(
        gic.sysreg(0, ICC_SGI1R_EL1, false, (3 << 24) | 2)
            .unwrap()
            .kick_mask,
        0
    );
    assert!(!gic.line_asserted(1));
    assert_eq!(gic.mmio(address, 1, Some(0x40)).kick_mask, 1 << 1);
    assert!(gic.line_asserted(1));
    assert!(!gic.line_asserted(0));
    assert_eq!(gic.mmio(address, 1, Some(0x40)).kick_mask, 0);
    assert_eq!(gic.mmio(address, 1, None).value, 0x40);
    assert_eq!(gic.mmio(address, 1, None).kick_mask, 0);
    assert_eq!(gic.mmio(address, 1, Some(0xa0)).kick_mask, 1 << 1);
    assert!(!gic.line_asserted(1));
    assert_eq!(gic.mmio(address, 1, Some(0xb0)).kick_mask, 0);
    assert_eq!(gic.mmio(address, 1, Some(0x40)).kick_mask, 1 << 1);
    assert_eq!(gic.sysreg(1, ICC_IAR1_EL1, true, 0).unwrap().value, 3);
}

#[test]
fn priority_writes_without_deliverable_pending_interrupt_do_not_kick() {
    let mut gic = UserspaceGic::new(2);
    wake_cpu(&mut gic, 1);
    enable_spi(&mut gic, 40, 1);
    assert_eq!(
        gic.mmio(machine::GIC_DIST.base + GICD_IPRIORITYR + 40, 1, Some(0x40))
            .kick_mask,
        0
    );
    let base = machine::GIC_REDIST.base + machine::GICV3_REDIST_STRIDE;
    assert_eq!(
        gic.mmio(base + GICR_IPRIORITYR + 3, 1, Some(0x40))
            .kick_mask,
        0
    );
    assert_eq!(
        gic.mmio(base + GICR_IPRIORITYR + 31, 2, Some(0)).kick_mask,
        0
    );
    assert!(!gic.line_asserted(0));
    assert!(!gic.line_asserted(1));
}
