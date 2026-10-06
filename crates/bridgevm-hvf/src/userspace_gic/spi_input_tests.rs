//! Electrical input history is distinct from the pending latch.
//! Arm IHI 0069G sections 1.2.1 and 4.1.2 require a new rising edge
//! before an acknowledged edge-triggered SPI becomes pending again.

use super::*;

fn configured(intid: u32, edge: bool) -> UserspaceGic {
    let mut gic = UserspaceGic::new(2);
    wake_cpu(&mut gic, 1);
    gic.mmio(
        machine::GIC_DIST.base + GICD_ICFGR + u64::from(intid / 16) * 4,
        4,
        Some(if edge { 2 << ((intid % 16) * 2) } else { 0 }),
    );
    enable_spi(&mut gic, intid, 1);
    gic
}

fn pending(gic: &mut UserspaceGic, intid: u32, expected: bool) {
    for alias in [GICD_ISPENDR, GICD_ICPENDR] {
        let result = gic.mmio(
            machine::GIC_DIST.base + alias + u64::from(intid / 32) * 4,
            4,
            None,
        );
        assert_eq!(result.kick_mask, 0);
        assert_eq!(result.value & (1 << (intid % 32)) != 0, expected);
    }
}

fn acknowledge(gic: &mut UserspaceGic) -> u64 {
    gic.sysreg(1, ICC_IAR1_EL1, true, 0).unwrap().value
}

fn eoi(gic: &mut UserspaceGic, intid: u32) {
    gic.sysreg(1, ICC_EOIR1_EL1, false, u64::from(intid))
        .unwrap();
}

#[test]
fn held_high_repetition_during_active_does_not_create_another_edge() {
    for intid in [32, 40, 63, 64, 255] {
        let mut gic = configured(intid, true);
        assert_eq!(gic.set_spi(intid, true), 2);
        assert_eq!(acknowledge(&mut gic), u64::from(intid));
        pending(&mut gic, intid, false);
        assert_eq!(gic.set_spi(intid, true), 0);
        pending(&mut gic, intid, false);
        eoi(&mut gic, intid);
        assert!(!gic.line_asserted(1));
        assert_eq!(acknowledge(&mut gic), u64::from(SPURIOUS_INTID));
    }
}

#[test]
fn held_high_repetition_after_eoi_does_not_raise_or_kick() {
    let mut gic = configured(40, true);
    gic.set_spi(40, true);
    assert_eq!(acknowledge(&mut gic), 40);
    eoi(&mut gic, 40);
    assert!(!gic.line_asserted(1));
    assert_eq!(gic.set_spi(40, true), 0);
    pending(&mut gic, 40, false);
    assert_eq!(acknowledge(&mut gic), u64::from(SPURIOUS_INTID));
}

#[test]
fn clearing_pending_does_not_rearm_a_still_high_input() {
    let mut gic = configured(40, true);
    gic.set_spi(40, true);
    gic.mmio(machine::GIC_DIST.base + GICD_ICPENDR + 4, 4, Some(1 << 8));
    pending(&mut gic, 40, false);
    assert_eq!(gic.set_spi(40, true), 0);
    pending(&mut gic, 40, false);
    assert_eq!(gic.set_spi(40, false), 0);
    assert_eq!(gic.set_spi(40, true), 2);
    assert_eq!(acknowledge(&mut gic), 40);
}

#[test]
fn real_second_edge_while_active_is_delivered_once_after_eoi() {
    let mut gic = configured(40, true);
    gic.set_spi(40, true);
    assert_eq!(acknowledge(&mut gic), 40);
    assert_eq!(gic.set_spi(40, false), 0);
    assert_eq!(gic.set_spi(40, true), 0);
    pending(&mut gic, 40, true);
    assert!(!gic.line_asserted(1));
    eoi(&mut gic, 40);
    assert!(gic.line_asserted(1));
    assert_eq!(acknowledge(&mut gic), 40);
    pending(&mut gic, 40, false);
    eoi(&mut gic, 40);
    assert!(!gic.line_asserted(1));
}

#[test]
fn falling_edge_preserves_pending_until_acknowledgement() {
    let mut gic = configured(40, true);
    gic.set_spi(40, true);
    assert_eq!(gic.set_spi(40, false), 0);
    pending(&mut gic, 40, true);
    assert_eq!(acknowledge(&mut gic), 40);
    pending(&mut gic, 40, false);
    eoi(&mut gic, 40);
    assert!(!gic.line_asserted(1));
}

#[test]
fn level_input_stays_pending_until_low_even_after_ack_or_clear() {
    let mut gic = configured(40, false);
    gic.set_spi(40, true);
    assert_eq!(acknowledge(&mut gic), 40);
    pending(&mut gic, 40, true);
    gic.mmio(machine::GIC_DIST.base + GICD_ICPENDR + 4, 4, Some(1 << 8));
    assert_eq!(gic.set_spi(40, true), 0);
    pending(&mut gic, 40, true);
    eoi(&mut gic, 40);
    assert!(gic.line_asserted(1));
    assert_eq!(gic.set_spi(40, false), 2);
    pending(&mut gic, 40, false);
    assert!(!gic.line_asserted(1));
}

#[test]
fn each_msi_message_relatches_without_electrical_input_transition() {
    let intid = machine::GIC_MSI_INTID_BASE;
    let mut gic = configured(intid, true);
    let address = machine::GIC_MSI_FRAME.base + GICM_SET_SPI_NSR;
    assert_eq!(gic.send_msi(address, intid), 2);
    assert_eq!(acknowledge(&mut gic), u64::from(intid));
    assert_eq!(gic.send_msi(address, intid), 0);
    pending(&mut gic, intid, true);
    eoi(&mut gic, intid);
    assert!(gic.line_asserted(1));
    assert_eq!(acknowledge(&mut gic), u64::from(intid));
    pending(&mut gic, intid, false);
    eoi(&mut gic, intid);
    assert!(!gic.line_asserted(1));
}

#[test]
fn clone_preserves_high_input_history_and_fresh_controller_starts_low() {
    let mut gic = configured(40, true);
    gic.set_spi(40, true);
    assert_eq!(acknowledge(&mut gic), 40);
    eoi(&mut gic, 40);
    let mut cloned = gic.clone();
    assert_eq!(cloned.set_spi(40, true), 0);
    pending(&mut cloned, 40, false);
    assert_eq!(cloned.set_spi(40, false), 0);
    assert_eq!(cloned.set_spi(40, true), 2);
    pending(&mut gic, 40, false);
    let mut fresh = configured(40, true);
    assert_eq!(fresh.set_spi(40, true), 2);
}
