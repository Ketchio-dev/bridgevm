//! A priority-dropped interrupt deactivated outside ICC_DIR_EL1 cannot grow
//! the CPU interface active list on each reacknowledgement.

use super::*;

const SPI: u32 = 40;

fn split_eoi_cpu() -> UserspaceGic {
    let mut gic = UserspaceGic::new(1);
    wake_cpu(&mut gic, 0);
    enable_spi(&mut gic, SPI, 0);
    gic.sysreg(0, ICC_CTLR_EL1, false, ICC_CTLR_EOIMODE)
        .unwrap();
    gic
}

fn acknowledge_and_drop(gic: &mut UserspaceGic) {
    gic.set_spi(SPI, true);
    assert_eq!(
        gic.sysreg(0, ICC_IAR1_EL1, true, 0).unwrap().value,
        u64::from(SPI)
    );
    gic.set_spi(SPI, false);
    gic.sysreg(0, ICC_EOIR1_EL1, false, u64::from(SPI)).unwrap();
}

#[test]
fn distributor_deactivation_does_not_accumulate_dropped_entries() {
    let mut gic = split_eoi_cpu();
    let icactiver = machine::GIC_DIST.base + GICD_ICACTIVER + u64::from(SPI / 32) * 4;
    for _ in 0..4096 {
        acknowledge_and_drop(&mut gic);
        gic.mmio(icactiver, 4, Some(1 << (SPI % 32)));
    }
    assert_eq!(gic.ifaces[0].active.len(), 1);
    assert_eq!(gic.sysreg(0, ICC_RPR_EL1, true, 0).unwrap().value, 0xff);

    gic.sysreg(0, ICC_DIR_EL1, false, u64::from(SPI)).unwrap();
    assert!(gic.ifaces[0].active.is_empty());
}

#[test]
fn reacknowledgement_keeps_an_undropped_preempted_priority() {
    let mut gic = split_eoi_cpu();
    let icactiver = machine::GIC_DIST.base + GICD_ICACTIVER + u64::from(SPI / 32) * 4;
    gic.sysreg(0, ICC_BPR1_EL1, false, 1).unwrap();
    gic.set_spi(SPI, true);
    assert_eq!(
        gic.sysreg(0, ICC_IAR1_EL1, true, 0).unwrap().value,
        u64::from(SPI)
    );
    gic.set_spi(SPI, false);
    gic.mmio(icactiver, 4, Some(1 << (SPI % 32)));
    gic.mmio(
        machine::GIC_DIST.base + GICD_IPRIORITYR + u64::from(SPI),
        1,
        Some(0x40),
    );
    gic.set_spi(SPI, true);
    assert_eq!(
        gic.sysreg(0, ICC_IAR1_EL1, true, 0).unwrap().value,
        u64::from(SPI)
    );

    // The first acknowledgement still holds its running priority until EOI.
    assert_eq!(gic.ifaces[0].active.len(), 2);
    gic.sysreg(0, ICC_EOIR1_EL1, false, u64::from(SPI)).unwrap();
    assert_eq!(gic.sysreg(0, ICC_RPR_EL1, true, 0).unwrap().value, 0xa0);
}
