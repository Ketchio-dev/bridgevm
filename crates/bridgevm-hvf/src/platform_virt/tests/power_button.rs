//! The ACPI power button reaches the guest through the GED selector and SPI 9.

use super::super::*;
use super::helpers::platform;
use crate::machine;

const GED_INTID: u32 = 41;

#[test]
fn power_button_press_raises_spi_9_until_the_guest_reads_the_selector() {
    let mut p = platform();
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
    p.press_power_button();
    assert_eq!(p.take_pending_spi_levels(), [(GED_INTID, true)]);

    let read = MmioOp::Read { size: 4 };
    assert_eq!(
        p.on_mmio(machine::ACPI_GED.base, read, &mut mem),
        MmioOutcome::ReadValue(2)
    );
    assert_eq!(p.take_pending_spi_levels(), [(GED_INTID, false)]);
    assert_eq!(
        p.on_mmio(machine::ACPI_GED.base, read, &mut mem),
        MmioOutcome::ReadValue(0)
    );
    assert!(p.take_pending_spi_levels().is_empty());
}

#[test]
fn guest_reset_lowers_an_unread_power_button_line() {
    let mut p = platform();
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
    p.press_power_button();
    p.reset().unwrap();
    assert_eq!(p.take_pending_spi_levels(), [(GED_INTID, false)]);
    assert_eq!(
        p.on_mmio(machine::ACPI_GED.base, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::ReadValue(0)
    );
}
