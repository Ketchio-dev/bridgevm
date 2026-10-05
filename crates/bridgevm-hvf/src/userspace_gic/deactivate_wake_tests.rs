//! SPI deactivation must report a newly asserted remote IRQ line.
//! Arm IHI 0069G section 4.1.1 permits cross-PE SPI deactivation;
//! section 12.9.20 permits routing changes without losing pending delivery.

use super::*;

const SPI: u32 = 40;

fn active_level_spi(split_eoi: bool) -> UserspaceGic {
    let mut gic = UserspaceGic::new(2);
    for cpu in 0..2 {
        wake_cpu(&mut gic, cpu);
        if split_eoi {
            gic.sysreg(cpu, ICC_CTLR_EL1, false, ICC_CTLR_EOIMODE);
        }
    }
    enable_spi(&mut gic, SPI, 0);
    assert_eq!(gic.set_spi(SPI, true), 1);
    assert_eq!(gic.sysreg(0, ICC_IAR1_EL1, true, 0).unwrap().value, 40);
    assert!(!gic.line_asserted(0));
    assert!(!gic.line_asserted(1));
    gic
}

fn retarget_to_cpu1(gic: &mut UserspaceGic) {
    let base = machine::GIC_DIST.base;
    let bank = u64::from(SPI / 32) * 4;
    let bit = 1 << (SPI % 32);
    gic.mmio(base + GICD_ICENABLER + bank, 4, Some(bit));
    gic.mmio(base + GICD_IROUTER + u64::from(SPI) * 8, 8, Some(1));
    gic.mmio(base + GICD_ISENABLER + bank, 4, Some(bit));
    // The earlier MMIO kicks may already have been consumed by CPU1.
    // While SPI40 remains active, its refreshed IRQ pin is still low.
    assert!(!gic.line_asserted(0));
    assert!(!gic.line_asserted(1));
}

#[test]
fn cross_cpu_dir_wakes_the_fixed_route_target() {
    let mut gic = active_level_spi(true);
    let eoi = gic.sysreg(0, ICC_EOIR1_EL1, false, u64::from(SPI)).unwrap();
    assert_eq!(eoi.kick_mask & 2, 0);
    assert!(!gic.line_asserted(0), "priority drop keeps the SPI active");
    assert_eq!(gic.sysreg(0, ICC_RPR_EL1, true, 0).unwrap().value, 0xff);

    let dir = gic.sysreg(1, ICC_DIR_EL1, false, u64::from(SPI)).unwrap();
    assert!(gic.line_asserted(0));
    assert!(!gic.line_asserted(1));
    assert_eq!(dir.kick_mask & 1, 1, "CPU0 needs a fresh IRQ pin");
    assert_eq!(gic.sysreg(0, ICC_IAR1_EL1, true, 0).unwrap().value, 40);
}

#[test]
fn combined_eoi_wakes_the_retargeted_active_spi() {
    let mut gic = active_level_spi(false);
    retarget_to_cpu1(&mut gic);
    let eoi = gic.sysreg(0, ICC_EOIR1_EL1, false, u64::from(SPI)).unwrap();
    assert!(!gic.line_asserted(0));
    assert!(gic.line_asserted(1));
    assert_eq!(eoi.kick_mask & 2, 2, "CPU1 needs a fresh IRQ pin");
    assert_eq!(gic.sysreg(1, ICC_IAR1_EL1, true, 0).unwrap().value, 40);
}

#[test]
fn split_dir_wakes_the_retargeted_active_spi() {
    let mut gic = active_level_spi(true);
    retarget_to_cpu1(&mut gic);
    let eoi = gic.sysreg(0, ICC_EOIR1_EL1, false, u64::from(SPI)).unwrap();
    assert_eq!(eoi.kick_mask & 2, 0);
    assert!(!gic.line_asserted(1), "EOIR alone must not deactivate");
    let dir = gic.sysreg(0, ICC_DIR_EL1, false, u64::from(SPI)).unwrap();
    assert!(!gic.line_asserted(0));
    assert!(gic.line_asserted(1));
    assert_eq!(dir.kick_mask & 2, 2, "CPU1 needs a fresh IRQ pin");
    assert_eq!(gic.sysreg(1, ICC_IAR1_EL1, true, 0).unwrap().value, 40);
}

#[test]
fn priority_drop_without_deactivation_does_not_wake_remote_cpu() {
    let mut gic = active_level_spi(true);
    retarget_to_cpu1(&mut gic);
    let eoi = gic.sysreg(0, ICC_EOIR1_EL1, false, u64::from(SPI)).unwrap();
    assert_eq!(eoi.kick_mask & 2, 0);
    assert!(!gic.line_asserted(0));
    assert!(!gic.line_asserted(1));
    let active = gic.mmio(machine::GIC_DIST.base + GICD_ISACTIVER + 4, 4, None);
    assert_ne!(active.value & (1 << (SPI % 32)), 0);
}

#[test]
fn deactivation_without_remote_line_change_does_not_add_remote_kick() {
    for case in ["no pending", "masked", "already asserted"] {
        let mut gic = active_level_spi(false);
        retarget_to_cpu1(&mut gic);
        match case {
            "no pending" => {
                gic.set_spi(SPI, false);
            }
            "masked" => {
                gic.sysreg(1, ICC_PMR_EL1, false, 0x80);
            }
            "already asserted" => {
                enable_spi(&mut gic, SPI + 1, 1);
                gic.set_spi(SPI + 1, true);
            }
            _ => unreachable!(),
        }
        let before = gic.line_asserted(1);
        let eoi = gic.sysreg(0, ICC_EOIR1_EL1, false, u64::from(SPI)).unwrap();
        assert_eq!(gic.line_asserted(1), before, "{case}");
        assert_eq!(eoi.kick_mask & 2, 0, "{case}");
    }
}

#[test]
fn private_interrupt_deactivation_does_not_wake_other_cpu() {
    let mut gic = UserspaceGic::new(2);
    for cpu in 0..2 {
        wake_cpu(&mut gic, cpu);
    }
    enable_vtimer_ppi(&mut gic, 0);
    gic.set_vtimer_ppi(0, true);
    let iar = gic.sysreg(0, ICC_IAR1_EL1, true, 0).unwrap();
    assert_eq!(iar.value, u64::from(VTIMER_INTID));
    let eoi = gic.sysreg(0, ICC_EOIR1_EL1, false, iar.value).unwrap();
    assert_eq!(eoi.kick_mask & 2, 0);
    assert!(!gic.vtimer_in_service(0));
    assert!(!gic.line_asserted(0));
    assert!(!gic.line_asserted(1));
}
