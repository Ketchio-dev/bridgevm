//! Guest pending reads include the wire level, not just the software latch.
//! Arm IHI 0069F sections 11.9.11 and 11.9.26 define both read aliases;
//! section 4.1.2 distinguishes active from active-and-pending interrupts.

use super::*;

fn configured_spi(intid: u32, edge: bool) -> UserspaceGic {
    let mut gic = UserspaceGic::new(2);
    wake_cpu(&mut gic, 1);
    // Set the trigger before enabling the interrupt or asserting its input.
    let cfg = GICD_ICFGR + u64::from(intid / 16) * 4;
    let trigger = if edge { 2 << ((intid % 16) * 2) } else { 0 };
    gic.mmio(machine::GIC_DIST.base + cfg, 4, Some(trigger));
    enable_spi(&mut gic, intid, 1);
    gic
}

fn bank_access(gic: &mut UserspaceGic, base: u64, intid: u32, write: Option<u64>) -> u64 {
    gic.mmio(
        machine::GIC_DIST.base + base + u64::from(intid / 32) * 4,
        4,
        write,
    )
    .value
}

fn assert_pending(gic: &mut UserspaceGic, intid: u32, expected: bool) {
    for base in [GICD_ISPENDR, GICD_ICPENDR] {
        let result = gic.mmio(
            machine::GIC_DIST.base + base + u64::from(intid / 32) * 4,
            4,
            None,
        );
        assert_eq!(
            result.kick_mask, 0,
            "pending reads must not change IRQ state"
        );
        assert_eq!(result.value & (1 << (intid % 32)) != 0, expected);
    }
}

#[test]
fn level_pending_readback_tracks_the_input_across_spi_banks() {
    for intid in [32, 40, 63, 64, 255] {
        let mut gic = configured_spi(intid, false);
        assert_pending(&mut gic, intid, false);
        gic.set_spi(intid, true);
        assert!(gic.line_asserted(1));
        assert!(!gic.line_asserted(0));
        assert_pending(&mut gic, intid, true);
        gic.set_spi(intid, false);
        assert!(!gic.line_asserted(1));
        assert_pending(&mut gic, intid, false);
    }
}

#[test]
fn active_level_pending_readback_tracks_the_still_asserted_input() {
    let mut gic = configured_spi(40, false);
    gic.set_spi(40, true);
    assert_eq!(gic.sysreg(1, ICC_IAR1_EL1, true, 0).unwrap().value, 40);
    assert!(
        !gic.line_asserted(1),
        "active-and-pending is not delivered again"
    );
    assert_ne!(bank_access(&mut gic, GICD_ISACTIVER, 40, None) & 0x100, 0);
    assert_pending(&mut gic, 40, true);
    gic.set_spi(40, false);
    assert_pending(&mut gic, 40, false);
    assert_ne!(bank_access(&mut gic, GICD_ISACTIVER, 40, None) & 0x100, 0);
    gic.sysreg(1, ICC_EOIR1_EL1, false, 40).unwrap();
    assert!(!gic.line_asserted(1));
}

#[test]
fn clear_pending_write_does_not_hide_an_asserted_level_input() {
    let mut gic = configured_spi(40, false);
    gic.set_spi(40, true);
    bank_access(&mut gic, GICD_ISPENDR, 40, Some(0x100));
    assert_pending(&mut gic, 40, true);
    bank_access(&mut gic, GICD_ICPENDR, 40, Some(0x100));
    assert!(gic.line_asserted(1));
    assert_pending(&mut gic, 40, true);
    gic.set_spi(40, false);
    assert_pending(&mut gic, 40, false);
}

#[test]
fn disabled_level_input_is_pending_without_being_deliverable() {
    let mut gic = configured_spi(40, false);
    bank_access(&mut gic, GICD_ICENABLER, 40, Some(0x100));
    gic.set_spi(40, true);
    assert!(!gic.line_asserted(1));
    assert_pending(&mut gic, 40, true);
    bank_access(&mut gic, GICD_ISENABLER, 40, Some(0x100));
    assert!(gic.line_asserted(1));
    assert_pending(&mut gic, 40, true);
}

#[test]
fn software_pending_survives_lowered_input_until_explicit_clear() {
    let mut gic = configured_spi(40, false);
    gic.set_spi(40, true);
    bank_access(&mut gic, GICD_ISPENDR, 40, Some(0x100));
    gic.set_spi(40, false);
    assert!(gic.line_asserted(1));
    assert_pending(&mut gic, 40, true);
    bank_access(&mut gic, GICD_ICPENDR, 40, Some(0));
    assert_pending(&mut gic, 40, true);
    bank_access(&mut gic, GICD_ICPENDR, 40, Some(0x100));
    assert_pending(&mut gic, 40, false);
    assert!(!gic.line_asserted(1));
    bank_access(&mut gic, GICD_ISPENDR, 40, Some(0));
    assert_pending(&mut gic, 40, false);
}

#[test]
fn edge_pending_survives_lowered_input_then_clears_on_acknowledge() {
    let mut gic = configured_spi(40, true);
    gic.set_spi(40, true);
    gic.set_spi(40, false);
    assert!(gic.line_asserted(1));
    assert_pending(&mut gic, 40, true);
    assert_eq!(gic.sysreg(1, ICC_IAR1_EL1, true, 0).unwrap().value, 40);
    assert_pending(&mut gic, 40, false);
    assert_ne!(bank_access(&mut gic, GICD_ISACTIVER, 40, None) & 0x100, 0);
    assert!(!gic.line_asserted(1));
    gic.sysreg(1, ICC_EOIR1_EL1, false, 40).unwrap();
    assert!(!gic.line_asserted(1));
}
