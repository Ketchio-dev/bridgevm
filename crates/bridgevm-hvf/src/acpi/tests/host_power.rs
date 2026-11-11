//! The Mac's AC adapter and battery in the DSDT.

use super::super::*;
use super::helpers::{contains_bytes, find, split_tables};
use crate::host_power;
use crate::machine;

fn dsdt(host_power_present: bool) -> Vec<u8> {
    let blobs = build_acpi_with_devices(
        1,
        AcpiDeviceConfig {
            host_power_present,
            ..AcpiDeviceConfig::default()
        },
    );
    find(&split_tables(&blobs.tables), "DSDT").to_vec()
}

#[test]
fn without_a_mac_battery_the_dsdt_has_no_power_source() {
    let dsdt = dsdt(false);
    assert_eq!(dsdt, find(&split_tables(&build_acpi(1).tables), "DSDT"));
    for absent in [
        b"ACPI0003".to_vec(),
        b"ADP0".to_vec(),
        b"BAT0".to_vec(),
        b"BATR".to_vec(),
        EISA_PNP0C0A.to_vec(),
        aml_dword(crate::acpi_ged::GED_EVENT_AC_ADAPTER),
    ] {
        assert!(!contains_bytes(&dsdt, &absent), "unexpected {absent:02x?}");
    }
}

#[test]
fn ac_adapter_and_battery_methods_read_the_host_power_page() {
    let dsdt = dsdt(true);
    let mut field_list = b"BATR\x03".to_vec(); // DWordAcc, NoLock, Preserve
    for name in HOST_POWER_FIELDS {
        field_list.extend_from_slice(name);
        field_list.push(32);
    }
    let needles = [
        // OperationRegion (BATR, SystemMemory, 0x090C0000, 0x28)
        b"\x5b\x80BATR\x00\x0c\x00\x00\x0c\x09\x0a\x28".to_vec(),
        field_list,
        // Device (ADP0) { Name (_HID, "ACPI0003") Name (_PCL, Package () { \_SB })
        b"ADP0\x08_HID\x0dACPI0003\x00\x08_PCL\x12\x07\x01\\_SB_".to_vec(),
        // Method (_PSR) { Return ((PFLG & One)) }
        b"_PSR\x00\xa4\x7bPFLG\x01\x00".to_vec(),
        // Device (BAT0) { Name (_HID, EisaId ("PNP0C0A")) Name (_UID, One)
        b"BAT0\x08_HID\x0c\x41\xd0\x0c\x0a\x08_UID\x01".to_vec(),
        // Method (_STA) { If ((PFLG & 2)) { Return (0x1F) } Return (0x0F) }
        b"_STA\x00\xa0\x0c\x7bPFLG\x0a\x02\x00\xa4\x0a\x1f\xa4\x0a\x0f".to_vec(),
        // Name (PBIF, Package (13) { mWh units, ..., rechargeable, ...
        b"PBIF\x12".to_vec(),
        b"\x0d\x00\x00\x00\x01\x00\x00\x00\x01\x01\x0dMac Battery\x00\x0d\x00\x0dLION\x00".to_vec(),
        // Method (_BIF, 0, Serialized) { PBIF [1] = BDCP ... Return (PBIF) }
        b"_BIF\x08\x70BDCP\x88PBIF\x01\x00".to_vec(),
        b"\x70BLOW\x88PBIF\x0a\x06\x00\xa4PBIF".to_vec(),
        // Method (_BST, 0, Serialized) { PBST [0] = BSTT ... Return (PBST) }
        b"_BST\x08\x70BSTT\x88PBST\x00\x00".to_vec(),
        b"\x70BPVT\x88PBST\x0a\x03\x00\xa4PBST".to_vec(),
    ];
    for needle in needles {
        assert!(contains_bytes(&dsdt, &needle), "DSDT missing {needle:02x?}");
    }
    assert_eq!(machine::HOST_POWER.base, 0x090C_0000);
    assert_eq!(host_power::PAGE_BYTES, 0x28);
    // The methods' fields are the page's registers.
    for (reg, name) in [
        (host_power::REG_FLAGS, b"PFLG"),
        (host_power::REG_STATE, b"BSTT"),
        (host_power::REG_RATE_MW, b"BRAT"),
        (host_power::REG_REMAINING_MWH, b"BRCP"),
        (host_power::REG_VOLTAGE_MV, b"BPVT"),
        (host_power::REG_DESIGN_CAPACITY_MWH, b"BDCP"),
        (host_power::REG_FULL_CAPACITY_MWH, b"BFCP"),
        (host_power::REG_DESIGN_VOLTAGE_MV, b"BDVT"),
        (host_power::REG_WARNING_CAPACITY_MWH, b"BWRN"),
        (host_power::REG_LOW_CAPACITY_MWH, b"BLOW"),
    ] {
        assert_eq!(HOST_POWER_FIELDS[reg], name);
    }
}

#[test]
fn ged_notifies_the_adapter_and_battery_and_keeps_the_power_button() {
    let dsdt = dsdt(true);
    for needle in [
        // If (((Local0 & 2) == 2)) { Notify (PWRB, 0x80) }
        b"\x93\x7b\x60\x0a\x02\x00\x0a\x02\x86PWRB\x0a\x80".to_vec(),
        // If (((Local0 & 0x00010000) == 0x00010000)) { Notify (ADP0, 0x80) }
        b"\x93\x7b\x60\x0c\x00\x00\x01\x00\x00\x0c\x00\x00\x01\x00\x86ADP0\x0a\x80".to_vec(),
        // If (((Local0 & 0x00020000) == 0x00020000)) { Notify (BAT0, 0x80) }
        b"\x93\x7b\x60\x0c\x00\x00\x02\x00\x00\x0c\x00\x00\x02\x00\x86BAT0\x0a\x80".to_vec(),
        // If (((Local0 & 0x00040000) == 0x00040000)) { Notify (BAT0, 0x81) }
        b"\x93\x7b\x60\x0c\x00\x00\x04\x00\x00\x0c\x00\x00\x04\x00\x86BAT0\x0a\x81".to_vec(),
    ] {
        assert!(
            contains_bytes(&dsdt, &needle),
            "GED _EVT missing {needle:02x?}"
        );
    }
}
