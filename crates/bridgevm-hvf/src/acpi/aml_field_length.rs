//! Split out of acpi.rs to keep files under 850 lines.

use super::*;

use crate::machine;

pub(crate) fn aml_field_length(value: usize) -> Vec<u8> {
    if value <= 0x3f {
        return vec![value as u8];
    }
    assert!(value <= 0x0fff, "small AML field expected");
    vec![0x40 | (value as u8 & 0x0f), (value >> 4) as u8]
}

pub(crate) fn aml_scope(name: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_SCOPE_OP];
    out.extend(aml_pkg_length(name.len() + body.len()));
    out.extend_from_slice(name);
    out.extend_from_slice(body);
    out
}

pub(crate) fn aml_device(name: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = vec![AML_EXT_OP, AML_DEVICE_OP];
    out.extend(aml_pkg_length(name.len() + body.len()));
    out.extend_from_slice(name);
    out.extend_from_slice(body);
    out
}

pub(crate) fn aml_method(name: &[u8; 4], arg_count: u8, serialized: bool, body: &[u8]) -> Vec<u8> {
    assert!(
        arg_count <= 7,
        "AML methods support at most seven arguments"
    );
    let mut out = vec![AML_METHOD_OP];
    out.extend(aml_pkg_length(name.len() + 1 + body.len()));
    out.extend_from_slice(name);
    out.push(arg_count | if serialized { 0x08 } else { 0x00 });
    out.extend_from_slice(body);
    out
}

pub(crate) fn resource_memory32_fixed(base: u64, size: u64) -> Vec<u8> {
    let base = u32::try_from(base).expect("Memory32Fixed base exceeds 32 bits");
    let size = u32::try_from(size).expect("Memory32Fixed size exceeds 32 bits");
    let mut out = vec![0x86, 0x09, 0x00, 0x01]; // Memory32Fixed, ReadWrite
    out.extend_from_slice(&base.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out
}

pub(crate) fn resource_interrupt(gsiv: u32) -> Vec<u8> {
    let mut out = vec![0x89, 0x06, 0x00, 0x01, 0x01]; // Consumer, level, active-high, exclusive
    out.extend_from_slice(&gsiv.to_le_bytes());
    out
}

pub(crate) fn resource_word_bus_number(min: u16, max: u16) -> Vec<u8> {
    let len = max
        .checked_sub(min)
        .and_then(|v| v.checked_add(1))
        .expect("invalid PCI bus range");
    let mut out = vec![0x88, 0x0D, 0x00, 0x02, 0x0C, 0x00]; // Word address, bus, min/max fixed
    out.extend_from_slice(&0u16.to_le_bytes()); // granularity
    out.extend_from_slice(&min.to_le_bytes());
    out.extend_from_slice(&max.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // translation offset
    out.extend_from_slice(&len.to_le_bytes());
    out
}

pub(crate) fn resource_dword_memory(base: u64, size: u64) -> Vec<u8> {
    let base = u32::try_from(base).expect("DWordMemory base exceeds 32 bits");
    let size = u32::try_from(size).expect("DWordMemory size exceeds 32 bits");
    let max = base
        .checked_add(size)
        .and_then(|v| v.checked_sub(1))
        .expect("DWordMemory range overflow");
    let mut out = vec![0x87, 0x17, 0x00, 0x00, 0x0C, 0x01]; // Memory, min/max fixed, read-write
    out.extend_from_slice(&0u32.to_le_bytes()); // granularity
    out.extend_from_slice(&base.to_le_bytes());
    out.extend_from_slice(&max.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // translation offset
    out.extend_from_slice(&size.to_le_bytes());
    out
}

pub(crate) fn resource_dword_io(base: u64, size: u64) -> Vec<u8> {
    let base = u32::try_from(base).expect("DWordIO translation exceeds 32 bits");
    let size = u32::try_from(size).expect("DWordIO size exceeds 32 bits");
    let max = size.checked_sub(1).expect("DWordIO size must be non-zero");
    let mut out = vec![0x87, 0x17, 0x00, 0x01, 0x0C, 0x03]; // I/O, min/max fixed, entire range
    out.extend_from_slice(&0u32.to_le_bytes()); // granularity
    out.extend_from_slice(&0u32.to_le_bytes()); // host-visible I/O min
    out.extend_from_slice(&max.to_le_bytes()); // host-visible I/O max
    out.extend_from_slice(&base.to_le_bytes()); // translation to CPU MMIO window
    out.extend_from_slice(&size.to_le_bytes());
    out
}

pub(crate) fn resource_qword_memory_with_flags(base: u64, size: u64, type_flags: u8) -> Vec<u8> {
    let max = base
        .checked_add(size)
        .and_then(|v| v.checked_sub(1))
        .expect("QWordMemory range overflow");
    let mut out = vec![0x8A, 0x2B, 0x00, 0x00, 0x0C, type_flags];
    out.extend_from_slice(&0u64.to_le_bytes()); // granularity
    out.extend_from_slice(&base.to_le_bytes());
    out.extend_from_slice(&max.to_le_bytes());
    out.extend_from_slice(&0u64.to_le_bytes()); // translation offset
    out.extend_from_slice(&size.to_le_bytes());
    out
}

pub(crate) fn resource_qword_memory(base: u64, size: u64) -> Vec<u8> {
    // Memory, min/max fixed, non-cacheable, read-write.
    resource_qword_memory_with_flags(base, size, 0x01)
}

pub(crate) fn resource_qword_prefetchable_memory(base: u64, size: u64) -> Vec<u8> {
    // Memory, min/max fixed, prefetchable, read-write.  In an ACPI address-space
    // descriptor bits 2:1 of the memory-specific flags encode cacheability and
    // 0b11 means prefetchable; bit 0 selects read-write.
    resource_qword_memory_with_flags(base, size, 0x07)
}

pub(crate) fn resource_end_tag() -> [u8; 2] {
    [0x79, 0x00]
}

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
    // Match QEMU's generic PCI host bridge policy: grant OS control for
    // PCIeHotplug, SHPCHotplug, PME, AER and PCIeCapability, but not LTR.
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

/// TCG PPI 1.3 `_DSM` and the reset-attack-mitigation `_DSM`, following
/// QEMU's guest ABI. The methods exchange state through the adjacent 0x400-byte
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

    // Windows cannot reliably DerefOf an indexed SystemMemory field. Match
    // QEMU by creating a one-byte dynamic OperationRegion for FUNC[operation].
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

/// TPM2 ACPI table for a FIFO/TIS device, matching QEMU's revision-4 client
/// table and the TCG ACPI layout. The loader relocates LASA (offset 68) to the
/// separately allocated `etc/tpm/log` buffer.
pub(crate) fn build_tpm2() -> Vec<u8> {
    let mut t = Table::new(b"TPM2", 4);
    t.u16(0); // Platform Class = client
    t.u16(0); // Reserved
    t.u64(0); // Control Area address: unused for FIFO/TIS
    t.u32(6); // Start Method = MMIO
    t.pad(12); // Platform-specific start-method parameters
    t.u32(TPM_LOG_AREA_MINIMUM_SIZE as u32); // LAML
    t.u64(0); // LASA, relocated by the fw_cfg table loader
    let table = t.finish();
    debug_assert_eq!(table.len(), 76);
    table
}

/// QEMU-like DSDT surface for devices Linux/Windows enumerate through ACPI.
/// MADT/GTDT/MCFG/SPCR/DBG2 carry the architectural tables, while this AML names
/// the platform devices the OS driver core expects to bind (`ACPI0007` CPU
/// devices, `ARMH0011` PL011, `PNP0A08` PCI root bridge and a power button).
pub(crate) fn build_dsdt(cpu_count: u64, devices: AcpiDeviceConfig) -> Vec<u8> {
    let mut sb = Vec::new();
    for cpu in 0..cpu_count {
        sb.extend(build_cpu_dsdt_device(cpu));
    }
    sb.extend(build_pl011_dsdt_device());
    sb.extend(build_pci_root_dsdt_device());
    sb.extend(build_ecam_reserved_dsdt_device());
    sb.extend(build_power_button_dsdt_device());
    if devices.tpm_tis_present {
        sb.extend(build_tpm_tis_dsdt_device());
    }

    let mut t = Table::new(b"DSDT", 2);
    t.bytes.extend(aml_scope(b"_SB_", &sb));
    t.finish()
}

/// MADT (Multiple APIC Description Table) for GICv3: one GICC per CPU, a single
/// GICD, and a GICR covering the whole redistributor window.
pub(crate) fn build_madt(cpu_count: u64) -> Vec<u8> {
    let mut t = Table::new(b"APIC", 4); // MADT signature is "APIC"
    t.u32(0); // Local Interrupt Controller Address (unused on GICv3)
    t.u32(0); // Flags (no PC-AT 8259)

    // GIC Distributor (type 0x0C) — exactly one. QEMU emits the GICD before
    // per-CPU GICC structures.
    t.u8(0x0C); // Type = GICD
    t.u8(24); // Length
    t.u16(0); // reserved
    t.u32(0); // GIC ID
    t.u64(machine::GIC_DIST.base); // Physical Base Address
    t.u32(0); // System Vector Base (reserved, must be 0)
    t.u8(3); // GIC version = 3
    t.pad(3); // reserved

    // One GIC CPU Interface (type 0x0B) structure per CPU.
    for cpu in 0..cpu_count {
        t.u8(0x0B); // Type = GICC
        t.u8(80); // Length
        t.u16(0); // reserved
        t.u32(cpu as u32); // CPU Interface Number
        t.u32(cpu as u32); // ACPI Processor UID
        t.u32(1); // Flags = Enabled
        t.u32(0); // Parking Protocol Version
        t.u32(ppi_to_gsiv(machine::PPI_PMU)); // Performance Interrupt GSIV
        t.u64(0); // Parked Address
        t.u64(0); // Physical Base Address (0 on GICv3 — sysreg interface)
        t.u64(0); // GICV (virtual CPU interface)
        t.u64(0); // GICH (hypervisor interface)
        t.u32(0); // VGIC Maintenance interrupt
        t.u64(redistributor_base(cpu)); // GICR Base Address (per-CPU)
                                        // MPIDR: affinity packed Aff3[39:32] | Aff2[23:16] | Aff1[15:8] | Aff0[7:0].
        t.u64(mpidr_for(cpu));
        t.u8(0); // Processor Power Efficiency Class
        t.u8(0); // reserved
        t.u16(0); // SPE overflow Interrupt
    }

    // GIC Redistributor (type 0x0E) — discovery range covering all GICRs.
    t.u8(0x0E); // Type = GICR
    t.u8(16); // Length
    t.u16(0); // reserved
    t.u64(machine::GIC_REDIST.base); // Discovery Range Base Address
    t.u32(machine::GIC_REDIST.size as u32); // Discovery Range Length

    // Generic MSI Frame (type 0x0D) — Apple `hv_gic` exposes message-signalled
    // SPIs through a GICv2m-compatible frame, not an architectural ITS/LPI
    // block. Linux's GICv3 driver only probes ITS when the distributor advertises
    // LPIs; the Apple GIC does not, so the ACPI MSI contract must be this frame.
    t.u8(0x0D); // Type = Generic MSI Frame
    t.u8(24); // Length
    t.u16(0); // reserved
    t.u32(0); // MSI Frame ID
    t.u64(machine::GIC_MSI_FRAME.base); // Physical Base Address
    t.u32(1); // Flags: override SPI values below
    t.u16(machine::GIC_MSI_INTID_COUNT as u16); // SPI Count
    t.u16(machine::GIC_MSI_INTID_BASE as u16); // SPI Base (absolute INTID)

    t.finish()
}

/// Per-CPU GICR base = window base + cpu * stride.
pub(crate) fn redistributor_base(cpu: u64) -> u64 {
    machine::GIC_REDIST.base + cpu * machine::GICV3_REDIST_STRIDE
}

/// Linear MPIDR affinity for `cpu` (Aff0 = 0..15, Aff1 = group of 16), matching
/// the scheme QEMU `virt` uses for small CPU counts.
pub(crate) fn mpidr_for(cpu: u64) -> u64 {
    machine::cpu_mpidr(cpu)
}

pub(crate) const PPTT_NODE_PROCESSOR: u8 = 0;
pub(crate) const PPTT_PROCESSOR_PHYSICAL_PACKAGE: u32 = 1 << 0;
pub(crate) const PPTT_PROCESSOR_ACPI_ID_VALID: u32 = 1 << 1;
pub(crate) const PPTT_PROCESSOR_LEAF: u32 = 1 << 3;
pub(crate) const PPTT_PROCESSOR_IDENTICAL: u32 = 1 << 4;

/// Append an ACPI PPTT processor hierarchy node (type 0). Offsets stored in
/// `parent` and `private_resources` are relative to the start of the PPTT table.
pub(crate) fn append_pptt_processor_node(
    t: &mut Table,
    flags: u32,
    parent: u32,
    acpi_processor_id: u32,
    private_resources: &[u32],
) {
    let len = 20 + private_resources.len() * 4;
    let len = u8::try_from(len).expect("PPTT processor node length exceeds u8");
    t.u8(PPTT_NODE_PROCESSOR);
    t.u8(len);
    t.u16(0); // reserved
    t.u32(flags);
    t.u32(parent);
    t.u32(acpi_processor_id);
    t.u32(private_resources.len() as u32);
    for &resource in private_resources {
        t.u32(resource);
    }
}

/// PPTT (Processor Properties Topology Table). Match QEMU's simple homogeneous
/// topology for `virt`: one root package node, one socket node, and one leaf
/// processor node per ACPI Processor UID.
pub(crate) fn build_pptt(cpu_count: u64) -> Vec<u8> {
    let mut t = Table::new(b"PPTT", 2);

    let root_offset = t.bytes.len() as u32;
    append_pptt_processor_node(
        &mut t,
        PPTT_PROCESSOR_PHYSICAL_PACKAGE | PPTT_PROCESSOR_IDENTICAL,
        0,
        0,
        &[],
    );

    let socket_offset = t.bytes.len() as u32;
    append_pptt_processor_node(
        &mut t,
        PPTT_PROCESSOR_PHYSICAL_PACKAGE | PPTT_PROCESSOR_IDENTICAL,
        root_offset,
        0,
        &[],
    );

    for cpu in 0..cpu_count {
        append_pptt_processor_node(
            &mut t,
            PPTT_PROCESSOR_ACPI_ID_VALID | PPTT_PROCESSOR_LEAF,
            socket_offset,
            u32::try_from(cpu).expect("ACPI processor ID exceeds u32"),
            &[],
        );
    }

    t.finish()
}

/// GTDT (Generic Timer Description Table) describing the architected timer. The
/// per-CPU timer interrupts are PPIs; the GSIV is `PPI + 16` (PPIs occupy
/// INTIDs 16..31). Edge/level is encoded in the per-timer flags.
pub(crate) fn build_gtdt() -> Vec<u8> {
    /// GTDT timer flag: interrupt is level-triggered (bit 1 clear = level).
    const TIMER_FLAG_LEVEL: u32 = 0;

    let mut t = Table::new(b"GTDT", 2);
    t.u64(0xFFFF_FFFF_FFFF_FFFF); // CntControlBase — not memory-mapped
    t.u32(0); // reserved
    t.u32(ppi_to_gsiv(machine::PPI_TIMER_SECURE)); // Secure EL1 timer GSIV
    t.u32(TIMER_FLAG_LEVEL); // Secure EL1 timer flags
    t.u32(ppi_to_gsiv(machine::PPI_TIMER_NONSEC)); // Non-Secure EL1 timer GSIV
    t.u32(TIMER_FLAG_LEVEL); // Non-Secure EL1 timer flags
    t.u32(ppi_to_gsiv(machine::PPI_TIMER_VIRT)); // Virtual EL1 timer GSIV
    t.u32(TIMER_FLAG_LEVEL); // Virtual EL1 timer flags
    t.u32(ppi_to_gsiv(machine::PPI_TIMER_HYP)); // EL2 (hypervisor) timer GSIV
    t.u32(TIMER_FLAG_LEVEL); // EL2 timer flags
    t.u64(0xFFFF_FFFF_FFFF_FFFF); // CntReadBase — not memory-mapped
    t.u32(0); // Platform Timer Count
    t.u32(0); // Platform Timer Offset (none present)
    t.finish()
}

/// PPI number to its absolute GIC interrupt ID (GSIV). PPIs occupy INTIDs 16..31.
pub(crate) fn ppi_to_gsiv(ppi: u32) -> u32 {
    ppi + 16
}

/// MCFG (PCI memory-mapped configuration space) describing the ECAM window for
/// PCI segment 0, buses 0..=255.
pub(crate) fn build_mcfg() -> Vec<u8> {
    let mut t = Table::new(b"MCFG", 1);
    t.u64(0); // reserved
              // One configuration-space allocation entry (16 bytes).
    t.u64(machine::PCIE_ECAM.base); // Base Address of enhanced config space
    t.u16(0); // PCI Segment Group Number
    t.u8(0); // Start PCI bus number
    t.u8(0xFF); // End PCI bus number
    t.u32(0); // reserved
    t.finish()
}

/// SPCR (Serial Port Console Redirection) pointing the OS console at the PL011.
pub(crate) fn build_spcr() -> Vec<u8> {
    let mut t = Table::new(b"SPCR", 2);
    t.u8(0x03); // Interface Type = ARM PL011 UART
    t.pad(3); // reserved
    t.gas_memory_with_access_size(machine::UART.base, 32, 3); // Base Address (dword access)
    t.u8(0x08); // Interrupt Type = ARM GIC
    t.u8(0); // IRQ (8259, unused)
    t.u32(machine::spi_to_intid(machine::SPI_UART)); // Global System Interrupt
    t.u8(7); // Baud Rate = as-is (do not reconfigure)
    t.u8(0); // Parity = none
    t.u8(1); // Stop Bits = 1
    t.u8(0); // Flow Control = none
    t.u8(0); // Terminal Type = VT100
    t.u8(0); // Language (reserved)
    t.u16(0xFFFF); // PCI Device ID = not a PCI device
    t.u16(0xFFFF); // PCI Vendor ID = not a PCI device
    t.u8(0); // PCI Bus
    t.u8(0); // PCI Device
    t.u8(0); // PCI Function
    t.u32(0); // PCI Flags
    t.u8(0); // PCI Segment
    t.u32(0); // reserved
    t.finish()
}

/// DBG2 (Debug Port Table 2) describing the same PL011 as a serial debug port.
pub(crate) fn build_dbg2() -> Vec<u8> {
    const NAMESPACE: &[u8] = b"COM0\0";
    const DEBUG_DEVICE_INFO_OFFSET: u32 = 44;
    const BASE_ADDRESS_REGISTER_OFFSET: u16 = 22;
    const ADDRESS_SIZE_OFFSET: u16 = 34;
    const NAMESPACE_STRING_OFFSET: u16 = 38;

    let device_len =
        u16::try_from(BASE_ADDRESS_REGISTER_OFFSET as usize + 12 + 4 + NAMESPACE.len())
            .expect("DBG2 device info length exceeds u16");

    let mut t = Table::new(b"DBG2", 0);
    t.u32(DEBUG_DEVICE_INFO_OFFSET); // OffsetDbgDeviceInfo
    t.u32(1); // NumberDbgDeviceInfo

    t.u8(0); // Revision
    t.u16(device_len); // Length
    t.u8(1); // NumberOfGenericAddressRegisters
    t.u16(NAMESPACE.len() as u16); // NameSpaceStringLength
    t.u16(NAMESPACE_STRING_OFFSET); // NameSpaceStringOffset
    t.u16(0); // OemDataLength
    t.u16(0); // OemDataOffset
    t.u16(0x8000); // Port Type = Serial
    t.u16(0x0003); // Port Subtype = ARM PL011 UART
    t.u16(0); // Reserved
    t.u16(BASE_ADDRESS_REGISTER_OFFSET);
    t.u16(ADDRESS_SIZE_OFFSET);

    t.gas_memory_with_access_size(machine::UART.base, 32, 3);
    t.u32(machine::UART.size as u32);
    t.bytes.extend_from_slice(NAMESPACE);
    t.finish()
}
