//! The Mac's power source as the guest sees it: an ACPI AC adapter
//! (`ACPI0003`, `\_SB.ADP0`) and a control-method battery (`PNP0C0A`,
//! `\_SB.BAT0`) whose `_PSR`, `_STA`, `_BIF` and `_BST` read this register page
//! through a SystemMemory OperationRegion (the AML is in `acpi`).
//!
//! The VMM fills the page from a host snapshot ([`HostPower`]). A change
//! returns GED events ([`HostPowerPage::update`]); the GED's `_EVT` turns them
//! into `Notify` so Windows re-reads the devices. The page is read-only: the
//! platform ignores guest writes, and a read outside the registers or of an odd
//! size reads zero.
//!
//! macOS reports the battery as a percentage, so the page presents a battery
//! with a fixed design and full-charge capacity in mWh (Windows' cmbatt needs
//! consistent units and a non-zero design capacity) and derives the
//! charge/discharge rate from macOS' time estimates, which makes Windows'
//! time-remaining agree with the Mac's.

use crate::acpi_ged::{GED_EVENT_AC_ADAPTER, GED_EVENT_BATTERY_INFO, GED_EVENT_BATTERY_STATUS};

/// One reading of the Mac's power source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostPower {
    /// The Mac runs from its power adapter.
    pub ac_online: bool,
    /// The internal battery; `None` on a Mac without one.
    pub battery: Option<HostBattery>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostBattery {
    /// Charge, 0..=100 (larger values read as 100).
    pub percent: u8,
    /// macOS is charging the battery.
    pub charging: bool,
    /// macOS' estimate while on battery, in minutes.
    pub minutes_to_empty: Option<u32>,
    /// macOS' estimate while charging, in minutes.
    pub minutes_to_full: Option<u32>,
}

/// 60 Wh, between a MacBook Air's and a 14-inch MacBook Pro's battery.
pub const DESIGN_CAPACITY_MWH: u32 = 60_000;
/// A three-cell lithium-ion pack's nominal voltage.
pub const DESIGN_VOLTAGE_MV: u32 = 11_400;
/// ACPI's "unknown" for `_BST` rate, capacity and voltage.
pub const UNKNOWN: u32 = 0xFFFF_FFFF;

/// Flags register: the Mac runs from its adapter (`_PSR`).
pub const FLAG_AC_ONLINE: u32 = 1 << 0;
/// Flags register: the battery is present (`_STA` 0x1F instead of 0x0F).
pub const FLAG_BATTERY_PRESENT: u32 = 1 << 1;
/// `_BST` state bit 0.
pub const STATE_DISCHARGING: u32 = 1 << 0;
/// `_BST` state bit 1.
pub const STATE_CHARGING: u32 = 1 << 1;

/// Number of 32-bit registers; register `i` is at byte offset `4 * i`.
pub const REGISTERS: usize = 10;
/// Bytes the DSDT's OperationRegion covers.
pub const PAGE_BYTES: usize = REGISTERS * 4;

// Register indexes, in page order.
pub const REG_FLAGS: usize = 0;
pub const REG_STATE: usize = 1;
pub const REG_RATE_MW: usize = 2;
pub const REG_REMAINING_MWH: usize = 3;
pub const REG_VOLTAGE_MV: usize = 4;
pub const REG_DESIGN_CAPACITY_MWH: usize = 5;
pub const REG_FULL_CAPACITY_MWH: usize = 6;
pub const REG_DESIGN_VOLTAGE_MV: usize = 7;
pub const REG_WARNING_CAPACITY_MWH: usize = 8;
pub const REG_LOW_CAPACITY_MWH: usize = 9;

/// The register values for `power`.
pub fn encode(power: &HostPower) -> [u32; REGISTERS] {
    let full = DESIGN_CAPACITY_MWH;
    let mut regs = [0; REGISTERS];
    regs[REG_FLAGS] = if power.ac_online { FLAG_AC_ONLINE } else { 0 }
        | if power.battery.is_some() {
            FLAG_BATTERY_PRESENT
        } else {
            0
        };
    regs[REG_DESIGN_CAPACITY_MWH] = DESIGN_CAPACITY_MWH;
    regs[REG_FULL_CAPACITY_MWH] = full;
    regs[REG_DESIGN_VOLTAGE_MV] = DESIGN_VOLTAGE_MV;
    regs[REG_WARNING_CAPACITY_MWH] = full / 10;
    regs[REG_LOW_CAPACITY_MWH] = full / 20;
    let Some(battery) = power.battery else {
        regs[REG_RATE_MW] = UNKNOWN;
        regs[REG_REMAINING_MWH] = UNKNOWN;
        regs[REG_VOLTAGE_MV] = UNKNOWN;
        return regs;
    };
    let remaining = full / 100 * u32::from(battery.percent.min(100));
    let per_hour = |mwh: u32, minutes: Option<u32>| {
        minutes
            .filter(|minutes| *minutes > 0)
            .map_or(UNKNOWN, |minutes| mwh * 60 / minutes)
    };
    // On the adapter and not charging (full, or macOS holding the charge) is
    // neither state, at rate 0.
    let (state, rate) = if !power.ac_online {
        (
            STATE_DISCHARGING,
            per_hour(remaining, battery.minutes_to_empty),
        )
    } else if battery.charging {
        (
            STATE_CHARGING,
            per_hour(full - remaining, battery.minutes_to_full),
        )
    } else {
        (0, 0)
    };
    regs[REG_STATE] = state;
    regs[REG_RATE_MW] = rate;
    regs[REG_REMAINING_MWH] = remaining;
    regs[REG_VOLTAGE_MV] = DESIGN_VOLTAGE_MV;
    regs
}

/// The register page behind `\_SB.BATR`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostPowerPage {
    regs: [u32; REGISTERS],
}

impl HostPowerPage {
    pub fn new(power: HostPower) -> Self {
        Self {
            regs: encode(&power),
        }
    }

    /// Show `power`. Returns the GED events for what changed: the adapter for
    /// `_PSR`, battery status for `_BST`, battery information for `_STA` or
    /// `_BIF`; 0 when nothing did.
    pub fn update(&mut self, power: HostPower) -> u32 {
        let new = encode(&power);
        let old = std::mem::replace(&mut self.regs, new);
        let flags = old[REG_FLAGS] ^ new[REG_FLAGS];
        let mut events = 0;
        if flags & FLAG_AC_ONLINE != 0 {
            events |= GED_EVENT_AC_ADAPTER;
        }
        if old[REG_STATE..=REG_VOLTAGE_MV] != new[REG_STATE..=REG_VOLTAGE_MV] {
            events |= GED_EVENT_BATTERY_STATUS;
        }
        if flags & FLAG_BATTERY_PRESENT != 0
            || old[REG_DESIGN_CAPACITY_MWH..] != new[REG_DESIGN_CAPACITY_MWH..]
        {
            events |= GED_EVENT_BATTERY_INFO;
        }
        events
    }

    /// A guest read of `size` bytes at `offset`, little-endian. Bytes past the
    /// registers read zero, and so does a size other than 1, 2, 4 or 8.
    pub fn mmio_read(&self, offset: u64, size: u8) -> u64 {
        if !matches!(size, 1 | 2 | 4 | 8) {
            return 0;
        }
        (0..u64::from(size)).fold(0, |value, i| {
            let byte = offset
                .checked_add(i)
                .and_then(|at| usize::try_from(at).ok())
                .and_then(|at| Some(self.regs.get(at / 4)?.to_le_bytes()[at % 4]))
                .unwrap_or(0);
            value | u64::from(byte) << (8 * i)
        })
    }
}

#[cfg(test)]
#[path = "host_power_tests.rs"]
mod tests;
