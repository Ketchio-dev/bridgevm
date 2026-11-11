//! ACPI Generic Event Device (`ACPI0013`): the host's power button and power
//! source.
//!
//! Register model of the GED that QEMU virt places at the same address
//! ([`crate::machine::ACPI_GED`]) and SPI ([`crate::machine::SPI_ACPI_GED`]):
//! one 32-bit event selector. The DSDT's `_EVT` method reads it to learn which
//! events fired, and the read clears them. Writes are ignored. `_EVT` turns the
//! power-button event into `Notify (PWRB, 0x80)`, and, when the VM has the
//! Mac's battery ([`crate::host_power`]), the power-source events into
//! notifications of the AC adapter and battery.
//!
//! The interrupt is declared edge-triggered, as QEMU declares it. Unlike QEMU's
//! pulse, the line stays high until the guest reads the selector, and a press
//! while it is still high lowers and raises it again. Every press is therefore
//! a fresh rising edge, and the event still arrives if the GIC treats the line
//! as level-triggered.

use crate::machine;

/// Selector bit of the power-button event (QEMU's `ACPI_GED_PWR_DOWN_EVT`).
pub const GED_EVENT_POWER_BUTTON: u32 = 1 << 1;
/// The Mac's power status changed (`_PSR`/`_BST`): `Notify (ADP0, 0x80)` and
/// `Notify (BAT0, 0x80)`. BridgeVM's own events use bits 16 and up, clear of
/// QEMU's.
pub const GED_EVENT_POWER_STATUS: u32 = 1 << 16;
/// The Mac's battery information changed (`_STA`/`_BIF`): `Notify (BAT0, 0x81)`.
pub const GED_EVENT_BATTERY_INFO: u32 = 1 << 17;

/// Absolute GIC INTID of the GED interrupt line.
pub const GED_INTID: u32 = machine::spi_to_intid(machine::SPI_ACPI_GED);

#[derive(Debug, Default)]
pub struct AcpiGed {
    selector: u32,
    line_high: bool,
}

impl AcpiGed {
    pub fn new() -> Self {
        Self::default()
    }

    /// Latch a power-button press and queue the rising edge in `spi_levels`.
    pub fn press_power_button(&mut self, spi_levels: &mut Vec<(u32, bool)>) {
        self.raise(GED_EVENT_POWER_BUTTON, spi_levels);
    }

    /// Latch `events` and queue a rising edge in `spi_levels`.
    pub fn raise(&mut self, events: u32, spi_levels: &mut Vec<(u32, bool)>) {
        debug_assert_ne!(events, 0);
        self.selector |= events;
        if self.line_high {
            spi_levels.push((GED_INTID, false));
        }
        spi_levels.push((GED_INTID, true));
        self.line_high = true;
    }

    /// A 32-bit read at offset 0 returns the pending events and clears them,
    /// which also lowers the line. Other accesses read as zero and change
    /// nothing.
    pub fn mmio_read(&mut self, offset: u64, size: u8, spi_levels: &mut Vec<(u32, bool)>) -> u64 {
        if offset != 0 || size != 4 {
            return 0;
        }
        self.lower_line(spi_levels);
        u64::from(std::mem::take(&mut self.selector))
    }

    /// Guest reset: drop unread events and lower the line.
    pub fn reset(&mut self, spi_levels: &mut Vec<(u32, bool)>) {
        self.selector = 0;
        self.lower_line(spi_levels);
    }

    fn lower_line(&mut self, spi_levels: &mut Vec<(u32, bool)>) {
        if std::mem::take(&mut self.line_high) {
            spi_levels.push((GED_INTID, false));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn press_raises_the_line_and_the_selector_read_returns_and_clears_it() {
        let mut ged = AcpiGed::new();
        let mut levels = Vec::new();
        ged.press_power_button(&mut levels);
        assert_eq!(levels, [(41, true)]);

        levels.clear();
        assert_eq!(ged.mmio_read(0, 4, &mut levels), 2);
        assert_eq!(levels, [(41, false)]);

        levels.clear();
        assert_eq!(ged.mmio_read(0, 4, &mut levels), 0);
        assert!(levels.is_empty());
    }

    #[test]
    fn a_press_before_the_read_is_a_fresh_edge() {
        let mut ged = AcpiGed::new();
        let mut levels = Vec::new();
        ged.press_power_button(&mut levels);
        ged.press_power_button(&mut levels);
        assert_eq!(levels, [(41, true), (41, false), (41, true)]);
        assert_eq!(ged.mmio_read(0, 4, &mut Vec::new()), 2);
    }

    #[test]
    fn other_accesses_read_zero_and_keep_the_event() {
        let mut ged = AcpiGed::new();
        let mut levels = Vec::new();
        ged.press_power_button(&mut levels);
        levels.clear();
        assert_eq!(ged.mmio_read(0, 1, &mut levels), 0);
        assert_eq!(ged.mmio_read(2, 2, &mut levels), 0);
        assert!(levels.is_empty());
        assert_eq!(ged.mmio_read(0, 4, &mut levels), 2);
    }

    #[test]
    fn events_raised_before_the_read_arrive_together() {
        let mut ged = AcpiGed::new();
        let mut levels = Vec::new();
        ged.raise(GED_EVENT_POWER_STATUS, &mut levels);
        ged.press_power_button(&mut levels);
        assert_eq!(levels, [(41, true), (41, false), (41, true)]);
        assert_eq!(
            ged.mmio_read(0, 4, &mut Vec::new()),
            u64::from(GED_EVENT_POWER_STATUS | GED_EVENT_POWER_BUTTON)
        );
    }

    #[test]
    fn reset_drops_the_event_and_lowers_the_line() {
        let mut ged = AcpiGed::new();
        let mut levels = Vec::new();
        ged.press_power_button(&mut levels);
        levels.clear();
        ged.reset(&mut levels);
        assert_eq!(levels, [(41, false)]);
        assert_eq!(ged.mmio_read(0, 4, &mut levels), 0);
    }
}
