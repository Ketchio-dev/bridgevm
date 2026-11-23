//! The DSDT: the AML devices the guest enumerates through ACPI.

use super::*;

use crate::machine;

pub(crate) fn build_pl011_dsdt_device() -> Vec<u8> {
    let mut crs = Vec::new();
    crs.extend(resource_memory32_fixed(
        machine::UART.base,
        machine::UART.size,
    ));
    crs.extend(resource_interrupt(machine::spi_to_intid(machine::SPI_UART)));
    crs.extend(resource_end_tag());

    let mut body = Vec::new();
    body.extend(aml_name_string(b"_HID", "ARMH0011"));
    body.extend(aml_name_simple(b"_UID", AML_ZERO_OP));
    body.extend(aml_name_simple(b"_CCA", AML_ONE_OP));
    body.extend(aml_name_buffer(b"_CRS", &crs));
    aml_device(b"COM0", &body)
}

pub(crate) fn build_cpu_dsdt_device(cpu: u64) -> Vec<u8> {
    let name = format!("C{cpu:03X}");
    assert!(
        name.len() == 4,
        "ACPI CPU device name requires a three-hex-digit UID: {cpu}",
    );
    let mut device_name = [0u8; 4];
    device_name.copy_from_slice(name.as_bytes());
    let uid = u8::try_from(cpu).expect("ACPI CPU UID exceeds one-byte AML encoding");

    let mut body = Vec::new();
    body.extend(aml_name_string(b"_HID", "ACPI0007"));
    body.extend(aml_name_byte(b"_UID", uid));
    aml_device(&device_name, &body)
}

pub(crate) fn build_pci_root_dsdt_device() -> Vec<u8> {
    let mut crs = Vec::new();
    crs.extend(resource_word_bus_number(0, 0x00FF));
    crs.extend(resource_dword_memory(
        machine::PCIE_MMIO_32.base,
        machine::PCIE_MMIO_32.size,
    ));
    crs.extend(resource_qword_memory(
        machine::PCIE_MMIO_64_NON_PREFETCH.base,
        machine::PCIE_MMIO_64_NON_PREFETCH.size,
    ));
    crs.extend(resource_qword_prefetchable_memory(
        machine::PCIE_MMIO_64_PREFETCH.base,
        machine::PCIE_MMIO_64_PREFETCH.size,
    ));
    crs.extend(resource_dword_io(
        machine::PCIE_PIO.base,
        machine::PCIE_PIO.size,
    ));
    crs.extend(resource_end_tag());

    let mut body = Vec::new();
    body.extend(aml_name_eisa(b"_HID", EISA_PNP0A08));
    body.extend(aml_name_eisa(b"_CID", EISA_PNP0A03));
    body.extend(aml_name_simple(b"_SEG", AML_ZERO_OP));
    body.extend(aml_name_simple(b"_BBN", AML_ZERO_OP));
    body.extend(aml_name_simple(b"_UID", AML_ZERO_OP));
    body.extend(aml_name_simple(b"_CCA", AML_ONE_OP));
    body.extend(aml_name_buffer(b"_CRS", &crs));
    body.extend(build_pci_root_osc_method());
    aml_device(b"PCI0", &body)
}

pub(crate) fn build_pci_root_osc_method() -> Vec<u8> {
    let cdw1 = aml_name_ref(b"CDW1");
    let cdw3 = aml_name_ref(b"CDW3");
    let local0 = aml_local0();

    let mut known_uuid_body = Vec::new();
    known_uuid_body.extend(aml_create_dword_field(&aml_arg(3), 4, b"CDW2"));
    known_uuid_body.extend(aml_create_dword_field(&aml_arg(3), 8, b"CDW3"));
    known_uuid_body.extend(aml_store(&cdw3, &local0));
    // Grant OS control for PCIeHotplug, SHPCHotplug, PME, AER, and
    // PCIeCapability, but not the unimplemented LTR feature.
    known_uuid_body.extend(aml_binary_op(AML_AND_OP, &local0, &aml_byte(0x1F), &local0));
    known_uuid_body.extend(aml_if(
        &aml_not_equal(&aml_arg(1), &aml_byte(1)),
        &aml_binary_op(AML_OR_OP, &cdw1, &aml_byte(0x08), &cdw1),
    ));
    known_uuid_body.extend(aml_if(
        &aml_not_equal(&cdw3, &local0),
        &aml_binary_op(AML_OR_OP, &cdw1, &aml_byte(0x10), &cdw1),
    ));
    known_uuid_body.extend(aml_store(&local0, &cdw3));

    let mut unknown_uuid_body = Vec::new();
    unknown_uuid_body.extend(aml_binary_op(AML_OR_OP, &cdw1, &aml_byte(0x04), &cdw1));

    let mut body = Vec::new();
    body.extend(aml_create_dword_field(&aml_arg(3), 0, b"CDW1"));
    body.extend(aml_if(
        &aml_equal(&aml_arg(0), &aml_uuid_buffer(&PCI_HOST_BRIDGE_OSC_UUID)),
        &known_uuid_body,
    ));
    body.extend(aml_else(&unknown_uuid_body));
    body.extend(aml_return(&aml_arg(3)));
    aml_method(b"_OSC", 4, false, &body)
}

/// Generic Event Device that turns a host power-button press into
/// `Notify (PWRB, 0x80)`, with the name, address, SPI and `_EVT` shape of the
/// GED that QEMU virt describes. The register model is [`crate::acpi_ged`].
/// With `host_power`, `_EVT` also notifies `ADP0`/`BAT0` on the power-source
/// events.
pub(crate) fn build_ged_dsdt_device(host_power: bool) -> Vec<u8> {
    let mut crs = resource_edge_interrupt(machine::spi_to_intid(machine::SPI_ACPI_GED));
    crs.extend(resource_end_tag());

    // Local0 = ESEL; If ((Local0 & 2) == 2) { Notify (PWRB, 0x80) }
    let local0 = aml_local0();
    let event = aml_byte(crate::acpi_ged::GED_EVENT_POWER_BUTTON as u8);
    let mut evt = aml_store(&aml_name_ref(b"ESEL"), &local0);
    evt.extend(aml_if(
        &aml_equal(
            &aml_binary_op(AML_AND_OP, &local0, &event, &[AML_ZERO_OP]),
            &event,
        ),
        &aml_notify(&aml_name_ref(b"PWRB"), &aml_byte(0x80)),
    ));
    if host_power {
        evt.extend(build_host_power_ged_events(&local0));
    }

    let mut body = Vec::new();
    body.extend(aml_name_string(b"_HID", "ACPI0013"));
    body.extend(aml_name_string(b"_UID", "GED"));
    body.extend(aml_name_buffer(b"_CRS", &crs));
    body.extend(aml_operation_region(
        b"EREG",
        &aml_dword(machine::ACPI_GED.base as u32),
        &aml_byte(machine::ACPI_GED.size as u8),
    ));
    body.extend(aml_field(
        b"EREG",
        0x43, // DWordAcc, NoLock, WriteAsZeros
        &[(b"ESEL", 32)],
    ));
    body.extend(aml_method(b"_EVT", 1, true, &evt));
    aml_device(b"GED_", &body)
}

pub(crate) fn build_power_button_dsdt_device() -> Vec<u8> {
    let mut body = Vec::new();
    body.extend(aml_name_eisa(b"_HID", EISA_PNP0C0C));
    aml_device(b"PWRB", &body)
}

pub(crate) fn build_ecam_reserved_dsdt_device() -> Vec<u8> {
    let mut crs = Vec::new();
    crs.extend(resource_qword_memory(
        machine::PCIE_ECAM.base,
        machine::PCIE_ECAM.size,
    ));
    crs.extend(resource_end_tag());

    let mut body = Vec::new();
    body.extend(aml_name_eisa(b"_HID", EISA_PNP0C02));
    body.extend(aml_name_simple(b"_UID", AML_ZERO_OP));
    body.extend(aml_name_buffer(b"_CRS", &crs));
    aml_device(b"RES0", &body)
}

pub(crate) fn build_tpm_tis_dsdt_device() -> Vec<u8> {
    let mut crs = Vec::new();
    crs.extend(resource_memory32_fixed(
        machine::TPM_TIS.base,
        machine::TPM_TIS.size,
    ));
    crs.extend(resource_end_tag());

    let mut body = Vec::new();
    body.extend(aml_name_string(b"_HID", "MSFT0101"));
    body.extend(aml_name_string(b"_STR", "TPM 2.0 Device"));
    body.extend(aml_name_simple(b"_UID", AML_ZERO_OP));
    body.extend(aml_name_byte(b"_STA", 0x0f));
    body.extend(aml_name_buffer(b"_CRS", &crs));
    body.extend(build_tpm_ppi_aml());
    aml_device(b"TPM0", &body)
}

/// TCG PPI 1.3 `_DSM` and reset-attack-mitigation `_DSM`. The methods exchange
/// state through the adjacent 0x400-byte
/// [`crate::tpm_ppi`] shared-memory window.
pub(crate) fn build_tpm_ppi_aml() -> Vec<u8> {
    let zero = vec![AML_ZERO_OP];
    let one = vec![AML_ONE_OP];
    let pprq = aml_name_ref(b"PPRQ");
    let pprm = aml_name_ref(b"PPRM");
    let tpm2 = aml_name_ref(b"TPM2");
    let tpm3 = aml_name_ref(b"TPM3");
    let op = aml_local(0);
    let op_flags = aml_local(1);

    let mut out = Vec::new();
    out.extend(aml_operation_region(
        b"TPP2",
        &aml_dword((machine::TPM_PPI.base + 0x100) as u32),
        &aml_byte(0x5a),
    ));
    out.extend(aml_field(
        b"TPP2",
        0x00, // AnyAcc, NoLock, Preserve
        &[
            (b"PPIN", 8),
            (b"PPIP", 32),
            (b"PPRP", 32),
            (b"PPRQ", 32),
            (b"PPRM", 32),
            (b"LPPR", 32),
        ],
    ));
    out.extend(aml_operation_region(
        b"TPP3",
        &aml_dword((machine::TPM_PPI.base + crate::tpm_ppi::MOVV_OFFSET as u64) as u32),
        &one,
    ));
    out.extend(aml_field(
        b"TPP3",
        0x01, // ByteAcc, NoLock, Preserve
        &[(b"MOVV", 8)],
    ));

    // Windows cannot reliably DerefOf an indexed SystemMemory field, so create
    // a one-byte dynamic OperationRegion for FUNC[operation].
    let mut tpfn = Vec::new();
    let upper_bits = aml_binary_op(AML_AND_OP, &aml_arg(0), &aml_dword(0xffff_ff00), &zero);
    tpfn.extend(aml_if(
        &aml_not_equal(&upper_bits, &zero),
        &aml_return(&zero),
    ));
    let func_addr = aml_binary_op(
        AML_ADD_OP,
        &aml_dword(machine::TPM_PPI.base as u32),
        &aml_arg(0),
        &zero,
    );
    tpfn.extend(aml_operation_region(b"TPP1", &func_addr, &one));
    tpfn.extend(aml_field(
        b"TPP1",
        0x01, // ByteAcc, NoLock, Preserve
        &[(b"TPPF", 8)],
    ));
    tpfn.extend(aml_return(&aml_name_ref(b"TPPF")));
    out.extend(aml_method(b"TPFN", 1, true, &tpfn));

    out.extend(aml_name_package(b"TPM2", &[zero.clone(), zero.clone()]));
    out.extend(aml_name_package(
        b"TPM3",
        &[zero.clone(), zero.clone(), zero.clone()],
    ));

    let arguments = aml_arg(3);
    let argument0 = aml_deref_of(&aml_index(&arguments, &zero, &zero));
    let mut ppi = Vec::new();
    ppi.extend(aml_if(
        &aml_equal(&aml_arg(2), &zero),
        &aml_return(&aml_buffer(&[0xff, 0x01])),
    ));
    ppi.extend(aml_if(
        &aml_equal(&aml_arg(2), &one),
        &aml_return(&aml_string("1.3")),
    ));

    let mut submit = Vec::new();
    submit.extend(aml_store(&argument0, &op));
    submit.extend(aml_store(&aml_call1(b"TPFN", &op), &op_flags));
    let masked_flags = aml_binary_op(
        AML_AND_OP,
        &op_flags,
        &aml_byte(crate::tpm_ppi::FUNC_MASK),
        &zero,
    );
    submit.extend(aml_if(&aml_equal(&masked_flags, &zero), &aml_return(&one)));
    submit.extend(aml_store(&op, &pprq));
    submit.extend(aml_store(&zero, &pprm));
    submit.extend(aml_return(&zero));
    ppi.extend(aml_if(&aml_equal(&aml_arg(2), &aml_byte(2)), &submit));

    let mut pending = Vec::new();
    let tpm2_request = aml_index(&tpm2, &one, &zero);
    let mut revision1 = aml_store(&pprq, &tpm2_request);
    revision1.extend(aml_return(&tpm2));
    pending.extend(aml_if(&aml_equal(&aml_arg(1), &one), &revision1));
    let tpm3_request = aml_index(&tpm3, &one, &zero);
    let tpm3_parameter = aml_index(&tpm3, &aml_byte(2), &zero);
    let mut revision2 = aml_store(&pprq, &tpm3_request);
    revision2.extend(aml_store(&pprm, &tpm3_parameter));
    revision2.extend(aml_return(&tpm3));
    pending.extend(aml_if(&aml_equal(&aml_arg(1), &aml_byte(2)), &revision2));
    ppi.extend(aml_if(&aml_equal(&aml_arg(2), &aml_byte(3)), &pending));
    ppi.extend(aml_if(
        &aml_equal(&aml_arg(2), &aml_byte(4)),
        &aml_return(&aml_byte(2)),
    ));

    let mut response = aml_store(&aml_name_ref(b"LPPR"), &tpm3_request);
    response.extend(aml_store(&aml_name_ref(b"PPRP"), &tpm3_parameter));
    response.extend(aml_return(&tpm3));
    ppi.extend(aml_if(&aml_equal(&aml_arg(2), &aml_byte(5)), &response));
    ppi.extend(aml_if(
        &aml_equal(&aml_arg(2), &aml_byte(6)),
        &aml_return(&aml_byte(3)),
    ));

    let mut submit2 = Vec::new();
    submit2.extend(aml_store(&argument0, &op));
    submit2.extend(aml_store(&aml_call1(b"TPFN", &op), &op_flags));
    let masked_flags = aml_binary_op(
        AML_AND_OP,
        &op_flags,
        &aml_byte(crate::tpm_ppi::FUNC_MASK),
        &zero,
    );
    submit2.extend(aml_if(&aml_equal(&masked_flags, &zero), &aml_return(&one)));
    submit2.extend(aml_if(
        &aml_equal(&masked_flags, &aml_byte(crate::tpm_ppi::FUNC_BLOCKED)),
        &aml_return(&aml_byte(3)),
    ));
    submit2.extend(aml_store(&op, &pprq));
    let mut revision1 = aml_store(&zero, &pprm);
    revision1.extend(aml_return(&zero));
    submit2.extend(aml_if(&aml_equal(&aml_arg(1), &one), &revision1));
    let argument1 = aml_deref_of(&aml_index(&arguments, &one, &zero));
    let mut revision2 = aml_store(&argument1, &pprm);
    revision2.extend(aml_return(&zero));
    submit2.extend(aml_if(&aml_equal(&aml_arg(1), &aml_byte(2)), &revision2));
    submit2.extend(aml_return(&zero));
    ppi.extend(aml_if(&aml_equal(&aml_arg(2), &aml_byte(7)), &submit2));

    let mut confirmation = aml_store(&argument0, &op);
    confirmation.extend(aml_store(&aml_call1(b"TPFN", &op), &op_flags));
    confirmation.extend(aml_return(&aml_binary_op(
        AML_AND_OP,
        &op_flags,
        &aml_byte(crate::tpm_ppi::FUNC_MASK),
        &zero,
    )));
    ppi.extend(aml_if(&aml_equal(&aml_arg(2), &aml_byte(8)), &confirmation));
    ppi.extend(aml_return(&aml_buffer(&[0])));

    let mut dsm = aml_if(
        &aml_equal(&aml_arg(0), &aml_uuid_buffer(&TPM_PPI_DSM_UUID)),
        &ppi,
    );

    let mut reset_attack = Vec::new();
    reset_attack.extend(aml_if(
        &aml_equal(&aml_arg(2), &zero),
        &aml_return(&aml_buffer(&[0x03])),
    ));
    let mut set_movv = aml_store(&argument0, &aml_name_ref(b"MOVV"));
    set_movv.extend(aml_return(&zero));
    reset_attack.extend(aml_if(&aml_equal(&aml_arg(2), &one), &set_movv));
    dsm.extend(aml_if(
        &aml_equal(&aml_arg(0), &aml_uuid_buffer(&TPM_RESET_ATTACK_DSM_UUID)),
        &reset_attack,
    ));
    dsm.extend(aml_return(&aml_buffer(&[0])));
    out.extend(aml_method(b"_DSM", 4, true, &dsm));
    out
}

/// DSDT surface for devices Linux and Windows enumerate through ACPI.
/// MADT/GTDT/MCFG/SPCR/DBG2 carry the architectural tables, while this AML names
/// the platform devices the OS driver core expects to bind (`ACPI0007` CPU
/// devices, `ARMH0011` PL011, `PNP0A08` PCI root bridge, and a power button
/// with the `ACPI0013` Generic Event Device that signals it), and, when the Mac
/// has a battery, its `ACPI0003` AC adapter and `PNP0C0A` battery.
pub(crate) fn build_dsdt(cpu_count: u64, devices: AcpiDeviceConfig) -> Vec<u8> {
    let mut sb = Vec::new();
    for cpu in 0..cpu_count {
        sb.extend(build_cpu_dsdt_device(cpu));
    }
    sb.extend(build_pl011_dsdt_device());
    sb.extend(build_pci_root_dsdt_device());
    sb.extend(build_ecam_reserved_dsdt_device());
    sb.extend(build_ged_dsdt_device(devices.host_power_present));
    sb.extend(build_power_button_dsdt_device());
    if devices.host_power_present {
        sb.extend(build_host_power_dsdt_devices());
    }
    if devices.tpm_tis_present {
        sb.extend(build_tpm_tis_dsdt_device());
    }

    let mut t = Table::new(b"DSDT", 2);
    t.bytes.extend(aml_scope(b"_SB_", &sb));
    t.finish()
}
