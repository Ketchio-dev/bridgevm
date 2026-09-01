use super::*;

fn le32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn le64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

fn split_tables(bytes: &[u8]) -> Vec<&[u8]> {
    let mut tables = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
        assert!(offset + ACPI_HEADER_LEN <= bytes.len());
        let length = le32(bytes, offset + 4) as usize;
        assert!(length >= ACPI_HEADER_LEN);
        let end = offset.checked_add(length).unwrap();
        assert!(end <= bytes.len());
        tables.push(&bytes[offset..end]);
        offset = align_table_offset(end as u64) as usize;
    }
    tables
}

fn find_table<'a>(tables: &'a [&'a [u8]], signature: &[u8; 4]) -> &'a [u8] {
    tables
        .iter()
        .copied()
        .find(|table| &table[..4] == signature)
        .unwrap_or_else(|| panic!("missing ACPI table {:?}", signature))
}

fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|window| window == needle)
}

#[test]
fn pc_acpi_is_finalized_inside_the_handoff_aperture() {
    let blobs = build_bridgevm_pc_acpi(4);
    assert_eq!(blobs.tables_base, BRIDGEVM_PC_ACPI_TABLES_GPA);
    assert!(blobs.tables_base + blobs.tables.len() as u64 <= board::BOOT_INFO.end());
    assert_eq!(&blobs.rsdp[..8], b"RSD PTR ");
    assert_eq!(le64(&blobs.rsdp, 24), blobs.tables_base);
    assert_eq!(
        blobs.rsdp[..20].iter().fold(0u8, |a, b| a.wrapping_add(*b)),
        0
    );
    assert_eq!(blobs.rsdp.iter().fold(0u8, |a, b| a.wrapping_add(*b)), 0);

    let tables = split_tables(&blobs.tables);
    let signatures: Vec<&[u8]> = tables.iter().map(|table| &table[..4]).collect();
    assert_eq!(
        signatures,
        [b"XSDT", b"DSDT", b"FACP", b"APIC", b"PPTT", b"GTDT", b"MCFG", b"SPCR", b"DBG2"]
    );
    for table in &tables {
        assert_eq!(&table[16..24], PC_OEM_TABLE_ID);
        assert_eq!(table.iter().fold(0u8, |a, b| a.wrapping_add(*b)), 0);
    }
}

#[test]
fn xsdt_and_fadt_point_to_exact_final_table_addresses() {
    let blobs = build_bridgevm_pc_acpi(2);
    let tables = split_tables(&blobs.tables);
    let mut starts = Vec::new();
    let mut offset = 0u64;
    for table in &tables {
        starts.push(blobs.tables_base + offset);
        offset = align_table_offset(offset + table.len() as u64);
    }
    assert!(starts.iter().all(|start| start % 8 == 0));
    let xsdt = find_table(&tables, b"XSDT");
    let entries: Vec<u64> = xsdt[ACPI_HEADER_LEN..]
        .chunks_exact(8)
        .map(|entry| u64::from_le_bytes(entry.try_into().unwrap()))
        .collect();
    assert_eq!(entries, starts[2..].to_vec());

    let fadt = find_table(&tables, b"FACP");
    assert_eq!(le64(fadt, 140), starts[1]);
}

#[test]
fn dsdt_mcfg_and_console_use_only_bridgevm_pc_addresses() {
    let blobs = build_bridgevm_pc_acpi(1);
    let tables = split_tables(&blobs.tables);
    let dsdt = find_table(&tables, b"DSDT");
    let mcfg = find_table(&tables, b"MCFG");
    let spcr = find_table(&tables, b"SPCR");

    assert!(contains(dsdt, &(board::UART.base as u32).to_le_bytes()));
    assert!(contains(dsdt, &board::PCIE_ECAM.base.to_le_bytes()));
    assert!(contains(
        dsdt,
        &(board::PCIE_MMIO_32.base as u32).to_le_bytes()
    ));
    assert!(contains(
        dsdt,
        &board::PCIE_MMIO_64_NON_PREFETCH.base.to_le_bytes()
    ));
    assert!(contains(
        dsdt,
        &board::PCIE_MMIO_64_PREFETCH.base.to_le_bytes()
    ));
    assert!(!contains(dsdt, b"MSFT0101"));
    assert_eq!(le64(mcfg, 44), board::PCIE_ECAM.base);
    assert_eq!(le64(spcr, 44), board::UART.base);
    assert_eq!(le32(spcr, 54), board::spi_to_intid(board::SPI_UART));
}

#[test]
fn madt_describes_pc_gic_cpus_redistributors_and_msi_frame() {
    let blobs = build_bridgevm_pc_acpi(3);
    let tables = split_tables(&blobs.tables);
    let madt = find_table(&tables, b"APIC");
    let mut offset = ACPI_HEADER_LEN + 8;
    let mut cpu = 0u64;
    let mut saw_dist = false;
    let mut saw_redist = false;
    let mut saw_msi = false;
    while offset < madt.len() {
        let kind = madt[offset];
        let length = madt[offset + 1] as usize;
        assert!(length >= 2 && offset + length <= madt.len());
        match kind {
            0x0b => {
                assert_eq!(
                    le64(madt, offset + 60),
                    board::GIC_REDIST.base + cpu * board::GICV3_REDIST_STRIDE
                );
                assert_eq!(le64(madt, offset + 68), board::cpu_mpidr(cpu));
                cpu += 1;
            }
            0x0c => {
                assert_eq!(le64(madt, offset + 8), board::GIC_DIST.base);
                saw_dist = true;
            }
            0x0d => {
                assert_eq!(le64(madt, offset + 8), board::GIC_MSI_FRAME.base);
                assert_eq!(
                    u16::from_le_bytes(madt[offset + 20..offset + 22].try_into().unwrap()),
                    board::GIC_MSI_INTID_COUNT as u16
                );
                assert_eq!(
                    u16::from_le_bytes(madt[offset + 22..offset + 24].try_into().unwrap()),
                    board::GIC_MSI_INTID_BASE as u16
                );
                saw_msi = true;
            }
            0x0e => {
                assert_eq!(le64(madt, offset + 4), board::GIC_REDIST.base);
                assert_eq!(le32(madt, offset + 12), board::GIC_REDIST.size as u32);
                saw_redist = true;
            }
            other => panic!("unexpected MADT structure {other:#x}"),
        }
        offset += length;
    }
    assert_eq!(cpu, 3);
    assert!(saw_dist && saw_redist && saw_msi);
}

#[test]
fn timer_and_serial_interrupts_match_the_live_board_contract() {
    let blobs = build_bridgevm_pc_acpi(1);
    let tables = split_tables(&blobs.tables);
    let gtdt = find_table(&tables, b"GTDT");
    assert_eq!(le32(gtdt, 64), ppi_to_gsiv(board::PPI_TIMER_VIRT));

    let text = String::from_utf8_lossy(&blobs.tables);
    assert!(!text.contains("QEMU"));
    assert!(!text.contains("qemu"));
    assert!(!text.split('\0').any(|value| value == "virt"));
    assert_eq!(blobs, build_bridgevm_pc_acpi(1));
}

#[test]
#[should_panic(expected = "outside the v1 contract")]
fn pc_acpi_rejects_zero_cpus() {
    build_bridgevm_pc_acpi(0);
}
