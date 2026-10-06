//! Active-priority readback follows acknowledgement and priority drop.
//! Arm IHI0069G §§12.2.2, 12.20 and B.7; APR bit encoding is implementation-defined.

use super::*;

fn read_group1_priorities(gic: &mut UserspaceGic) -> [u64; 4] {
    [ICC_AP1R0_EL1, ICC_AP1R1_EL1, ICC_AP1R2_EL1, ICC_AP1R3_EL1]
        .map(|reg| gic.sysreg(0, reg, true, 0).unwrap().value)
}

#[test]
fn iar_and_eoir_are_visible_in_group1_active_priorities() {
    let mut gic = UserspaceGic::new(1);
    wake_cpu(&mut gic, 0);
    enable_spi(&mut gic, 40, 0);
    gic.sysreg(0, ICC_BPR1_EL1, false, 1).unwrap();
    gic.mmio(machine::GIC_DIST.base + GICD_IPRIORITYR + 40, 1, Some(0x4e));
    assert_eq!(read_group1_priorities(&mut gic), [0; 4]);

    gic.set_spi(40, true);
    assert_eq!(gic.sysreg(0, ICC_IAR1_EL1, true, 0).unwrap().value, 40);
    gic.set_spi(40, false);
    assert_eq!(gic.sysreg(0, ICC_RPR_EL1, true, 0).unwrap().value, 0x4e);
    let active = read_group1_priorities(&mut gic);
    assert!(active.iter().all(|value| value >> 32 == 0));
    assert_ne!(
        active, [0; 4],
        "IAR must be reflected by Group 1 active-priority state"
    );

    gic.sysreg(0, ICC_EOIR1_EL1, false, 40).unwrap();
    assert_eq!(read_group1_priorities(&mut gic), [0; 4]);
    assert_eq!(gic.sysreg(0, ICC_RPR_EL1, true, 0).unwrap().value, 0xff);
}

fn activate(gic: &mut UserspaceGic, intid: u32, priority: u64) {
    enable_spi(gic, intid, 0);
    gic.mmio(
        machine::GIC_DIST.base + GICD_IPRIORITYR + u64::from(intid),
        1,
        Some(priority),
    );
    gic.set_spi(intid, true);
    assert_eq!(
        gic.sysreg(0, ICC_IAR1_EL1, true, 0).unwrap().value,
        u64::from(intid)
    );
    gic.set_spi(intid, false);
}

fn lowest_active_priority(active: [u64; 4]) -> u64 {
    let n = active.iter().position(|&word| word != 0).unwrap();
    (n as u64 * 32 + u64::from(active[n].trailing_zeros())) << 1
}

fn idle_cpu() -> UserspaceGic {
    let mut gic = UserspaceGic::new(1);
    wake_cpu(&mut gic, 0);
    gic.sysreg(0, ICC_BPR1_EL1, false, 1).unwrap();
    gic
}

#[test]
fn nested_active_priorities_track_running_priority_and_group0_stays_idle() {
    let mut gic = idle_cpu();
    activate(&mut gic, 60, 0xc1);
    activate(&mut gic, 61, 0x21);
    let active = read_group1_priorities(&mut gic);
    assert_eq!(active.iter().filter(|&&w| w != 0).count(), 2);
    assert_eq!(lowest_active_priority(active), 0x20);
    assert_eq!(gic.sysreg(0, ICC_RPR_EL1, true, 0).unwrap().value, 0x20);
    for reg in [ICC_AP0R0_EL1, ICC_AP0R1_EL1, ICC_AP0R2_EL1, ICC_AP0R3_EL1] {
        assert_eq!(gic.sysreg(0, reg, true, 0).unwrap().value, 0);
    }
    gic.sysreg(0, ICC_EOIR1_EL1, false, 61).unwrap();
    assert_eq!(
        lowest_active_priority(read_group1_priorities(&mut gic)),
        0xc0
    );
    gic.sysreg(0, ICC_EOIR1_EL1, false, 60).unwrap();
    assert_eq!(read_group1_priorities(&mut gic), [0; 4]);
}

#[test]
fn split_eoi_clears_active_priority_before_deactivation() {
    let mut gic = idle_cpu();
    gic.sysreg(0, ICC_CTLR_EL1, false, ICC_CTLR_EOIMODE)
        .unwrap();
    activate(&mut gic, 60, 0x81);
    assert_ne!(read_group1_priorities(&mut gic), [0; 4]);
    gic.sysreg(0, ICC_EOIR1_EL1, false, 60).unwrap();
    assert_eq!(read_group1_priorities(&mut gic), [0; 4]);
    gic.sysreg(0, ICC_DIR_EL1, false, 60).unwrap();
    assert_eq!(read_group1_priorities(&mut gic), [0; 4]);
}

#[test]
fn writing_back_the_read_value_preserves_priority_and_zero_drops_it() {
    let mut gic = idle_cpu();
    activate(&mut gic, 60, 0x40);
    let active = read_group1_priorities(&mut gic);
    let regs = [ICC_AP1R0_EL1, ICC_AP1R1_EL1, ICC_AP1R2_EL1, ICC_AP1R3_EL1];
    for (reg, value) in regs.into_iter().zip(active) {
        assert_eq!(gic.sysreg(0, reg, false, value).unwrap().kick_mask, 0);
    }
    gic.sysreg(0, ICC_AP0R0_EL1, false, u64::from(u32::MAX))
        .unwrap();
    gic.sysreg(0, ICC_AP1R3_EL1, false, u64::from(u32::MAX))
        .unwrap();
    assert_eq!(read_group1_priorities(&mut gic), active);
    assert_eq!(gic.sysreg(0, ICC_RPR_EL1, true, 0).unwrap().value, 0x40);

    // A pending lower-priority SPI is masked by the running priority until
    // the guest clears the active-priority state.
    enable_spi(&mut gic, 61, 0);
    gic.set_spi(61, true);
    assert!(!gic.line_asserted(0));
    for (reg, value) in regs.into_iter().zip(active) {
        let kick = gic.sysreg(0, reg, false, 0).unwrap().kick_mask;
        assert_eq!(kick, u64::from(value != 0));
    }
    assert_eq!(gic.sysreg(0, ICC_RPR_EL1, true, 0).unwrap().value, 0xff);
    assert!(gic.line_asserted(0));
    // The interrupt itself stays active until deactivated.
    let active_spis = gic.mmio(machine::GIC_DIST.base + GICD_ISACTIVER + 4, 4, None);
    assert_eq!(active_spis.value, 1 << 28);
}
