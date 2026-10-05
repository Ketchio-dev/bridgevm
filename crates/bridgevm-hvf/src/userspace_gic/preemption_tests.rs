//! Binary-point grouping gates preemption, while PMR and ordering use full priority.

use super::*;

fn configured(bpr: u64, first: u64, second: u64) -> UserspaceGic {
    let mut gic = UserspaceGic::new(2);
    gic.sysreg(1, ICC_BPR1_EL1, false, bpr);
    assert_eq!(
        gic.sysreg(1, ICC_BPR1_EL1, true, 0).unwrap().value,
        bpr.max(1)
    );
    assert_eq!(gic.sysreg(1, ICC_CTLR_EL1, true, 0).unwrap().value & 1, 0);
    assert_eq!(
        (gic.sysreg(1, ICC_CTLR_EL1, true, 0).unwrap().value >> 8) & 7,
        7
    );
    assert_ne!(
        gic.mmio(machine::GIC_DIST.base + GICD_CTLR, 4, None).value & u64::from(GICD_CTLR_DS),
        0
    );
    wake_cpu(&mut gic, 1);
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

fn acknowledge(gic: &mut UserspaceGic) -> u64 {
    gic.sysreg(1, ICC_IAR1_EL1, true, 0).unwrap().value
}

fn activate_first(gic: &mut UserspaceGic) {
    assert_eq!(gic.set_spi(60, true), 2);
    assert_eq!(acknowledge(gic), 60);
    gic.set_spi(60, false);
    assert!(!gic.line_asserted(1));
}

fn same_group_waits(bpr: u64, first: u64, second: u64, split: bool) {
    let mut gic = configured(bpr, first, second);
    gic.sysreg(
        1,
        ICC_CTLR_EL1,
        false,
        if split { ICC_CTLR_EOIMODE } else { 0 },
    );
    activate_first(&mut gic);
    assert_eq!(
        gic.set_spi(61, true),
        0,
        "subpriority alone cannot raise the IRQ line"
    );
    assert!(!gic.line_asserted(1));
    assert!(!gic.line_asserted(0));
    assert_eq!(acknowledge(&mut gic), u64::from(SPURIOUS_INTID));
    assert_eq!(
        gic.mmio(machine::GIC_DIST.base + GICD_ISPENDR + 4, 4, None)
            .value,
        1 << 29
    );
    gic.sysreg(1, ICC_EOIR1_EL1, false, 60);
    assert!(
        gic.line_asserted(1),
        "priority drop releases the waiting group"
    );
    assert_eq!(acknowledge(&mut gic), 61);
    gic.set_spi(61, false);
    gic.sysreg(1, ICC_EOIR1_EL1, false, 61);
    if split {
        gic.sysreg(1, ICC_DIR_EL1, false, 61);
        gic.sysreg(1, ICC_DIR_EL1, false, 60);
    }
    assert!(!gic.line_asserted(1));
    assert_eq!(gic.sysreg(1, ICC_RPR_EL1, true, 0).unwrap().value, 0xff);
}

#[test]
fn minimum_bpr1_prevents_subpriority_only_preemption() {
    for bpr in [0, 1] {
        same_group_waits(bpr, 0x41, 0x40, false);
    }
}

#[test]
fn coarse_bpr1_prevents_subpriority_only_preemption() {
    same_group_waits(4, 0x4f, 0x40, false);
}

#[test]
fn maximum_bpr1_preserves_both_priority_groups() {
    same_group_waits(7, 0xfe, 0x80, false);
    same_group_waits(7, 0x7f, 0x00, false);
}

#[test]
fn split_priority_drop_releases_same_group_before_deactivation() {
    same_group_waits(4, 0x4f, 0x40, true);
}

#[test]
fn strictly_higher_priority_group_preempts_and_restores_outer_interrupt() {
    for (bpr, first, second) in [(1, 0x41, 0x3f), (4, 0x4f, 0x3f), (7, 0x81, 0x7f)] {
        let mut gic = configured(bpr, first, second);
        activate_first(&mut gic);
        assert_eq!(gic.set_spi(61, true), 2);
        assert_eq!(acknowledge(&mut gic), 61);
        gic.set_spi(61, false);
        gic.sysreg(1, ICC_EOIR1_EL1, false, 61);
        assert!(!gic.line_asserted(1));
        assert_eq!(
            gic.mmio(machine::GIC_DIST.base + GICD_ISACTIVER + 4, 4, None)
                .value,
            1 << 28
        );
        gic.sysreg(1, ICC_EOIR1_EL1, false, 60);
        assert_eq!(gic.sysreg(1, ICC_RPR_EL1, true, 0).unwrap().value, 0xff);
    }
}

#[test]
fn pmr_compares_full_pending_priority_not_its_group_boundary() {
    let mut gic = configured(4, 0x4f, 0x3f);
    activate_first(&mut gic);
    gic.sysreg(1, ICC_PMR_EL1, false, 0x31);
    assert_eq!(gic.set_spi(61, true), 0);
    assert!(!gic.line_asserted(1));
    assert_eq!(acknowledge(&mut gic), u64::from(SPURIOUS_INTID));
    gic.sysreg(1, ICC_PMR_EL1, false, 0x3f);
    assert!(!gic.line_asserted(1), "equal to PMR is still masked");
    gic.sysreg(1, ICC_PMR_EL1, false, 0x40);
    assert!(gic.line_asserted(1));
    assert_eq!(acknowledge(&mut gic), 61);
}

#[test]
fn idle_priority_is_not_masked_to_the_maximum_bpr1_boundary() {
    let mut gic = configured(7, 0xfe, 0xff);
    assert_eq!(gic.sysreg(1, ICC_RPR_EL1, true, 0).unwrap().value, 0xff);
    assert_eq!(gic.set_spi(61, true), 0, "idle priority is always masked");
    assert!(!gic.line_asserted(1));
    assert_eq!(gic.set_spi(60, true), 2, "0xfe remains eligible while idle");
    assert_eq!(acknowledge(&mut gic), 60);
}

#[test]
fn pending_subpriority_order_survives_coarse_grouping() {
    let mut gic = configured(7, 0x8f, 0x80);
    gic.set_spi(60, true);
    gic.set_spi(61, true);
    assert_eq!(
        acknowledge(&mut gic),
        61,
        "full subpriority orders pending interrupts"
    );
    assert!(!gic.line_asserted(1));
    gic.set_spi(61, false);
    gic.sysreg(1, ICC_EOIR1_EL1, false, 61);
    assert_eq!(acknowledge(&mut gic), 60);
}
