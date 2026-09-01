//! Direct ACPI image for the experimental BridgeVM Virtual ARM PC v1.
//!
//! Unlike the current compatibility platform, this image has final physical
//! pointers and checksums and does not use `fw_cfg` or a table-loader protocol.

use super::*;
use crate::machine::bridgevm_pc as board;

const PC_OEM_TABLE_ID: &[u8; 8] = b"BVMPC   ";
pub const BRIDGEVM_PC_ACPI_TABLES_GPA: u64 = board::BOOT_INFO.base + 0x2000;
fn align_table_offset(offset: u64) -> u64 {
    (offset + 7) & !7
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeVmPcAcpiBlobs {
    pub rsdp: Vec<u8>,
    pub tables: Vec<u8>,
    pub tables_base: u64,
}
/// Build finalized ACPI tables in the board's firmware-handoff aperture.
pub fn build_bridgevm_pc_acpi(cpu_count: u64) -> BridgeVmPcAcpiBlobs {
    assert!(
        (1..=board::MAX_CPUS).contains(&cpu_count),
        "BridgeVM PC ACPI CPU count is outside the v1 contract"
    );
    let dsdt = build_pc_dsdt(cpu_count);
    let madt = build_pc_madt(cpu_count);
    let pptt = retag_table(build_pptt(cpu_count));
    let gtdt = build_pc_gtdt();
    let mcfg = build_pc_mcfg();
    let spcr = build_pc_spcr();
    let dbg2 = build_pc_dbg2();
    let base = BRIDGEVM_PC_ACPI_TABLES_GPA;
    let off_xsdt = 0;
    let off_dsdt = align_table_offset(off_xsdt + xsdt_len_for(7));
    let off_fadt = align_table_offset(off_dsdt + dsdt.len() as u64);
    let off_madt = align_table_offset(off_fadt + fadt_len());
    let off_pptt = align_table_offset(off_madt + madt.len() as u64);
    let off_gtdt = align_table_offset(off_pptt + pptt.len() as u64);
    let off_mcfg = align_table_offset(off_gtdt + gtdt.len() as u64);
    let off_spcr = align_table_offset(off_mcfg + mcfg.len() as u64);
    let off_dbg2 = align_table_offset(off_spcr + spcr.len() as u64);

    let fadt = retag_table(build_fadt(base + off_dsdt));
    let xsdt = build_pc_xsdt(&[
        base + off_fadt,
        base + off_madt,
        base + off_pptt,
        base + off_gtdt,
        base + off_mcfg,
        base + off_spcr,
        base + off_dbg2,
    ]);
    assert_eq!(align_table_offset(xsdt.len() as u64), off_dsdt);

    let mut tables = Vec::new();
    for table in [
        &xsdt, &dsdt, &fadt, &madt, &pptt, &gtdt, &mcfg, &spcr, &dbg2,
    ] {
        tables.resize(align_table_offset(tables.len() as u64) as usize, 0);
        tables.extend_from_slice(table);
    }
    assert!(
        tables.len() as u64 <= board::BOOT_INFO.size,
        "BridgeVM PC ACPI tables exceed the v1 handoff aperture"
    );

    BridgeVmPcAcpiBlobs {
        rsdp: build_rsdp(base),
        tables,
        tables_base: base,
    }
}
fn pc_table(signature: &[u8; 4], revision: u8) -> Table {
    let mut table = Table::new(signature, revision);
    table.bytes[16..24].copy_from_slice(PC_OEM_TABLE_ID);
    table
}

fn retag_table(mut table: Vec<u8>) -> Vec<u8> {
    assert!(table.len() >= ACPI_HEADER_LEN);
    table[16..24].copy_from_slice(PC_OEM_TABLE_ID);
    table[9] = 0;
    table[9] = checksum(&table);
    table
}

fn build_pc_pl011_device() -> Vec<u8> {
    let mut resources = Vec::new();
    resources.extend(resource_memory32_fixed(board::UART.base, board::UART.size));
    resources.extend(resource_interrupt(board::spi_to_intid(board::SPI_UART)));
    resources.extend(resource_end_tag());

    let mut body = Vec::new();
    body.extend(aml_name_string(b"_HID", "ARMH0011"));
    body.extend(aml_name_simple(b"_UID", AML_ZERO_OP));
    body.extend(aml_name_simple(b"_CCA", AML_ONE_OP));
    body.extend(aml_name_buffer(b"_CRS", &resources));
    aml_device(b"COM0", &body)
}

fn build_pc_pci_root_device() -> Vec<u8> {
    let mut resources = Vec::new();
    resources.extend(resource_word_bus_number(0, 0x00ff));
    resources.extend(resource_dword_memory(
        board::PCIE_MMIO_32.base,
        board::PCIE_MMIO_32.size,
    ));
    resources.extend(resource_qword_memory(
        board::PCIE_MMIO_64_NON_PREFETCH.base,
        board::PCIE_MMIO_64_NON_PREFETCH.size,
    ));
    resources.extend(resource_qword_prefetchable_memory(
        board::PCIE_MMIO_64_PREFETCH.base,
        board::PCIE_MMIO_64_PREFETCH.size,
    ));
    resources.extend(resource_end_tag());

    let mut body = Vec::new();
    body.extend(aml_name_eisa(b"_HID", EISA_PNP0A08));
    body.extend(aml_name_eisa(b"_CID", EISA_PNP0A03));
    body.extend(aml_name_simple(b"_SEG", AML_ZERO_OP));
    body.extend(aml_name_simple(b"_BBN", AML_ZERO_OP));
    body.extend(aml_name_simple(b"_UID", AML_ZERO_OP));
    body.extend(aml_name_simple(b"_CCA", AML_ONE_OP));
    body.extend(aml_name_buffer(b"_CRS", &resources));
    body.extend(build_pci_root_osc_method());
    aml_device(b"PCI0", &body)
}

fn build_pc_ecam_reserved_device() -> Vec<u8> {
    let mut resources = resource_qword_memory(board::PCIE_ECAM.base, board::PCIE_ECAM.size);
    resources.extend(resource_end_tag());

    let mut body = Vec::new();
    body.extend(aml_name_eisa(b"_HID", EISA_PNP0C02));
    body.extend(aml_name_simple(b"_UID", AML_ZERO_OP));
    body.extend(aml_name_buffer(b"_CRS", &resources));
    aml_device(b"RES0", &body)
}

fn build_pc_dsdt(cpu_count: u64) -> Vec<u8> {
    let mut scope = Vec::new();
    for cpu in 0..cpu_count {
        scope.extend(build_cpu_dsdt_device(cpu));
    }
    scope.extend(build_pc_pl011_device());
    scope.extend(build_pc_pci_root_device());
    scope.extend(build_pc_ecam_reserved_device());
    scope.extend(build_power_button_dsdt_device());

    let mut table = pc_table(b"DSDT", 2);
    table.bytes.extend(aml_scope(b"_SB_", &scope));
    table.finish()
}

fn build_pc_madt(cpu_count: u64) -> Vec<u8> {
    let mut table = pc_table(b"APIC", 4);
    table.u32(0);
    table.u32(0);

    table.u8(0x0c);
    table.u8(24);
    table.u16(0);
    table.u32(0);
    table.u64(board::GIC_DIST.base);
    table.u32(0);
    table.u8(3);
    table.pad(3);

    for cpu in 0..cpu_count {
        table.u8(0x0b);
        table.u8(80);
        table.u16(0);
        table.u32(cpu as u32);
        table.u32(cpu as u32);
        table.u32(1);
        table.u32(0);
        table.u32(ppi_to_gsiv(board::PPI_PMU));
        table.u64(0);
        table.u64(0);
        table.u64(0);
        table.u64(0);
        table.u32(0);
        table.u64(board::GIC_REDIST.base + cpu * board::GICV3_REDIST_STRIDE);
        table.u64(board::cpu_mpidr(cpu));
        table.u8(0);
        table.u8(0);
        table.u16(0);
    }

    table.u8(0x0e);
    table.u8(16);
    table.u16(0);
    table.u64(board::GIC_REDIST.base);
    table.u32(board::GIC_REDIST.size as u32);

    table.u8(0x0d);
    table.u8(24);
    table.u16(0);
    table.u32(0);
    table.u64(board::GIC_MSI_FRAME.base);
    table.u32(1);
    table.u16(board::GIC_MSI_INTID_COUNT as u16);
    table.u16(board::GIC_MSI_INTID_BASE as u16);
    table.finish()
}

fn build_pc_gtdt() -> Vec<u8> {
    let mut table = pc_table(b"GTDT", 2);
    table.u64(u64::MAX);
    table.u32(0);
    for ppi in [
        board::PPI_TIMER_SECURE,
        board::PPI_TIMER_NONSEC,
        board::PPI_TIMER_VIRT,
        board::PPI_TIMER_HYP,
    ] {
        table.u32(ppi_to_gsiv(ppi));
        table.u32(0);
    }
    table.u64(u64::MAX);
    table.u32(0);
    table.u32(0);
    table.finish()
}

fn build_pc_mcfg() -> Vec<u8> {
    let mut table = pc_table(b"MCFG", 1);
    table.u64(0);
    table.u64(board::PCIE_ECAM.base);
    table.u16(0);
    table.u8(0);
    table.u8(0xff);
    table.u32(0);
    table.finish()
}

fn build_pc_spcr() -> Vec<u8> {
    let mut table = pc_table(b"SPCR", 2);
    table.u8(0x03);
    table.pad(3);
    table.gas_memory_with_access_size(board::UART.base, 32, 3);
    table.u8(0x08);
    table.u8(0);
    table.u32(board::spi_to_intid(board::SPI_UART));
    table.u8(7);
    table.u8(0);
    table.u8(1);
    table.u8(0);
    table.u8(0);
    table.u8(0);
    table.u16(0xffff);
    table.u16(0xffff);
    table.u8(0);
    table.u8(0);
    table.u8(0);
    table.u32(0);
    table.u8(0);
    table.u32(0);
    table.finish()
}

fn build_pc_dbg2() -> Vec<u8> {
    const NAMESPACE: &[u8] = b"COM0\0";
    const DEVICE_OFFSET: u32 = 44;
    const GAS_OFFSET: u16 = 22;
    const SIZE_OFFSET: u16 = 34;
    const NAME_OFFSET: u16 = 38;
    let device_len = u16::try_from(GAS_OFFSET as usize + 16 + NAMESPACE.len()).unwrap();

    let mut table = pc_table(b"DBG2", 0);
    table.u32(DEVICE_OFFSET);
    table.u32(1);
    table.u8(0);
    table.u16(device_len);
    table.u8(1);
    table.u16(NAMESPACE.len() as u16);
    table.u16(NAME_OFFSET);
    table.u16(0);
    table.u16(0);
    table.u16(0x8000);
    table.u16(0x0003);
    table.u16(0);
    table.u16(GAS_OFFSET);
    table.u16(SIZE_OFFSET);
    table.gas_memory_with_access_size(board::UART.base, 32, 3);
    table.u32(board::UART.size as u32);
    table.bytes.extend_from_slice(NAMESPACE);
    table.finish()
}

fn build_pc_xsdt(entries: &[u64]) -> Vec<u8> {
    let mut table = pc_table(b"XSDT", 1);
    for &entry in entries {
        table.u64(entry);
    }
    table.finish()
}

#[cfg(test)]
#[path = "bridgevm_pc_tests.rs"]
mod tests;
