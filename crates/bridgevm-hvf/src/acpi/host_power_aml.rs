//! DSDT devices for the Mac's power source ([`crate::host_power`]): the AC
//! adapter `\_SB.ADP0` (`ACPI0003`) and the control-method battery
//! `\_SB.BAT0` (`PNP0C0A`) that Windows' cmbatt binds, plus the GED `_EVT`
//! branches that notify them. The methods only copy page registers into their
//! results; the VMM computes every value.

use super::*;

use crate::acpi_ged::{GED_EVENT_AC_ADAPTER, GED_EVENT_BATTERY_INFO, GED_EVENT_BATTERY_STATUS};
use crate::host_power;
use crate::machine;

pub(crate) const EISA_PNP0C0A: [u8; 4] = [0x41, 0xD0, 0x0C, 0x0A];

/// The page's AML field names in register order: field `i` reads the 32-bit
/// register at offset `4 * i`.
pub(crate) const HOST_POWER_FIELDS: [&[u8; 4]; host_power::REGISTERS] = [
    b"PFLG", // flags: bit 0 AC online, bit 1 battery present
    b"BSTT", // _BST battery state
    b"BRAT", // _BST present rate, mW
    b"BRCP", // _BST remaining capacity, mWh
    b"BPVT", // _BST present voltage, mV
    b"BDCP", // _BIF design capacity, mWh
    b"BFCP", // _BIF last full charge capacity, mWh
    b"BDVT", // _BIF design voltage, mV
    b"BWRN", // _BIF design capacity of warning, mWh
    b"BLOW", // _BIF design capacity of low, mWh
];

/// `OperationRegion (BATR, SystemMemory, ...)`, its fields, `ADP0` and `BAT0`,
/// for `\_SB`.
pub(crate) fn build_host_power_dsdt_devices() -> Vec<u8> {
    let base = u32::try_from(machine::HOST_POWER.base).expect("host power page below 4 GiB");
    let length = u8::try_from(host_power::PAGE_BYTES).expect("small host power page");
    let fields: Vec<(&[u8; 4], usize)> = HOST_POWER_FIELDS.iter().map(|name| (*name, 32)).collect();
    let mut out = aml_operation_region(b"BATR", &aml_dword(base), &aml_byte(length));
    out.extend(aml_field(
        b"BATR", 0x03, // DWordAcc, NoLock, Preserve
        &fields,
    ));
    out.extend(build_ac_adapter_device());
    out.extend(build_battery_device());
    out
}

/// `Name (_PCL, Package () { \_SB })`: the power source feeds the whole system.
fn power_consumer_list() -> Vec<u8> {
    aml_name_package(b"_PCL", &[b"\\_SB_".to_vec()])
}

/// `PFLG & flag`.
fn flag(flag: u32) -> Vec<u8> {
    let flag = u8::try_from(flag).expect("flag in the low byte");
    aml_binary_op(
        AML_AND_OP,
        &aml_name_ref(b"PFLG"),
        &aml_byte(flag),
        &[AML_ZERO_OP],
    )
}

fn build_ac_adapter_device() -> Vec<u8> {
    let mut body = aml_name_string(b"_HID", "ACPI0003");
    body.extend(power_consumer_list());
    body.extend(aml_method(
        b"_PSR",
        0,
        false,
        &aml_return(&flag(host_power::FLAG_AC_ONLINE)),
    ));
    aml_device(b"ADP0", &body)
}

/// `Store (field, Index (package, index))` for each pair, then `Return (package)`.
fn fill_and_return(package: &[u8; 4], slots: &[(u8, &[u8; 4])]) -> Vec<u8> {
    let mut out = Vec::new();
    for (index, field) in slots {
        out.extend(aml_store(
            &aml_name_ref(field),
            &aml_index(&aml_name_ref(package), &aml_byte(*index), &[AML_ZERO_OP]),
        ));
    }
    out.extend(aml_return(&aml_name_ref(package)));
    out
}

fn build_battery_device() -> Vec<u8> {
    // Present, enabled, shown, functioning, and a battery in the slot.
    let mut sta = aml_if(
        &flag(host_power::FLAG_BATTERY_PRESENT),
        &aml_return(&aml_byte(0x1F)),
    );
    sta.extend(aml_return(&aml_byte(0x0F)));

    let zero = vec![AML_ZERO_OP];
    let one = vec![AML_ONE_OP];
    let bif_template = [
        zero.clone(),              // power unit: mW / mWh
        zero.clone(),              // design capacity (BDCP)
        zero.clone(),              // last full charge capacity (BFCP)
        one.clone(),               // battery technology: rechargeable
        zero.clone(),              // design voltage (BDVT)
        zero.clone(),              // design capacity of warning (BWRN)
        zero.clone(),              // design capacity of low (BLOW)
        one.clone(),               // capacity granularity 1, mWh
        one,                       // capacity granularity 2, mWh
        aml_string("Mac Battery"), // model number
        aml_string(""),            // serial number
        aml_string("LION"),        // battery type
        aml_string("BridgeVM"),    // OEM information
    ];
    let bif = fill_and_return(
        b"PBIF",
        &[
            (1, b"BDCP"),
            (2, b"BFCP"),
            (4, b"BDVT"),
            (5, b"BWRN"),
            (6, b"BLOW"),
        ],
    );
    let bst = fill_and_return(
        b"PBST",
        &[(0, b"BSTT"), (1, b"BRAT"), (2, b"BRCP"), (3, b"BPVT")],
    );

    let mut body = aml_name_eisa(b"_HID", EISA_PNP0C0A);
    body.extend(aml_name_simple(b"_UID", AML_ONE_OP));
    body.extend(power_consumer_list());
    body.extend(aml_method(b"_STA", 0, false, &sta));
    body.extend(aml_name_package(b"PBIF", &bif_template));
    body.extend(aml_method(b"_BIF", 0, true, &bif));
    body.extend(aml_name_package(
        b"PBST",
        &[zero.clone(), zero.clone(), zero.clone(), zero],
    ));
    body.extend(aml_method(b"_BST", 0, true, &bst));
    aml_device(b"BAT0", &body)
}

/// `If (((Local0 & event) == event)) { body }`.
fn ged_branch(local0: &[u8], event: u32, body: &[u8]) -> Vec<u8> {
    let event = aml_dword(event);
    aml_if(
        &aml_equal(
            &aml_binary_op(AML_AND_OP, local0, &event, &[AML_ZERO_OP]),
            &event,
        ),
        body,
    )
}

/// The GED `_EVT` branches for the power-source events; `local0` holds the
/// selector `_EVT` read.
pub(crate) fn build_host_power_ged_events(local0: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for (event, device, value) in [
        (GED_EVENT_AC_ADAPTER, b"ADP0", 0x80), // status change: _PSR
        (GED_EVENT_BATTERY_STATUS, b"BAT0", 0x80), // status change: _BST
        (GED_EVENT_BATTERY_INFO, b"BAT0", 0x81), // information change: _STA, _BIF
    ] {
        out.extend(ged_branch(
            local0,
            event,
            &aml_notify(&aml_name_ref(device), &aml_byte(value)),
        ));
    }
    out
}
