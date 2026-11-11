//! The Mac's battery reaches the guest through the host power page and GED.

use super::super::*;
use super::helpers::{platform, platform_with_devices};
use crate::acpi::ACPI_TABLE_FILE;
use crate::acpi_ged::GED_EVENT_BATTERY_STATUS;
use crate::host_power::{HostBattery, HostPower, REG_FLAGS, REG_REMAINING_MWH};
use crate::machine;

const GED_INTID: u32 = 41;

fn on_battery(percent: u8) -> HostPower {
    HostPower {
        ac_online: false,
        battery: Some(HostBattery {
            percent,
            charging: false,
            minutes_to_empty: None,
            minutes_to_full: None,
        }),
    }
}

fn with_battery(percent: u8) -> VirtPlatform {
    platform_with_devices(VirtPlatformDeviceConfig {
        host_power: Some(on_battery(percent)),
        ..VirtPlatformDeviceConfig::default()
    })
}

fn read_reg(p: &mut VirtPlatform, reg: usize) -> MmioOutcome {
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
    p.on_mmio(
        machine::HOST_POWER.base + 4 * reg as u64,
        MmioOp::Read { size: 4 },
        &mut mem,
    )
}

#[test]
fn the_page_serves_the_mac_reading_and_ignores_guest_writes() {
    let mut p = with_battery(80);
    assert_eq!(
        read_reg(&mut p, REG_REMAINING_MWH),
        MmioOutcome::ReadValue(48_000)
    );
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
    for offset in [0, 4 * REG_REMAINING_MWH as u64, 0xFFC] {
        let write = MmioOp::Write {
            size: 4,
            value: 0xFFFF_FFFF,
        };
        assert_eq!(
            p.on_mmio(machine::HOST_POWER.base + offset, write, &mut mem),
            MmioOutcome::WriteAck
        );
    }
    assert_eq!(read_reg(&mut p, REG_FLAGS), MmioOutcome::ReadValue(2));
    assert_eq!(
        read_reg(&mut p, REG_REMAINING_MWH),
        MmioOutcome::ReadValue(48_000)
    );
    assert!(p.take_pending_spi_levels().is_empty());
    // The ACPI tables carry the battery.
    let tables = p.fw_cfg.file_bytes(ACPI_TABLE_FILE).unwrap();
    assert!(tables.windows(4).any(|name| name == b"BAT0"));
}

#[test]
fn a_new_reading_raises_the_ged_event_and_survives_a_guest_reset() {
    let mut p = with_battery(80);
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
    p.set_host_power(on_battery(80));
    assert!(
        p.take_pending_spi_levels().is_empty(),
        "no change, no event"
    );

    p.set_host_power(on_battery(79));
    assert_eq!(p.take_pending_spi_levels(), [(GED_INTID, true)]);
    assert_eq!(
        p.on_mmio(machine::ACPI_GED.base, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::ReadValue(u64::from(GED_EVENT_BATTERY_STATUS))
    );
    assert_eq!(p.take_pending_spi_levels(), [(GED_INTID, false)]);
    assert_eq!(
        read_reg(&mut p, REG_REMAINING_MWH),
        MmioOutcome::ReadValue(47_400)
    );

    // The reading is the Mac's, not the boot's: a guest reboot keeps it.
    p.reset().unwrap();
    assert_eq!(
        read_reg(&mut p, REG_REMAINING_MWH),
        MmioOutcome::ReadValue(47_400)
    );
}

#[test]
fn without_a_battery_device_the_page_stays_a_hole() {
    let mut p = platform();
    assert_eq!(read_reg(&mut p, REG_FLAGS), MmioOutcome::Unmapped);
    p.set_host_power(on_battery(50));
    assert!(p.take_pending_spi_levels().is_empty());
    let tables = p.fw_cfg.file_bytes(ACPI_TABLE_FILE).unwrap();
    assert!(!tables.windows(4).any(|name| name == b"BAT0"));
}
