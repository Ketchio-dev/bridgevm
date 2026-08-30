use super::*;
use crate::acpi::build_bridgevm_pc_acpi;
use crate::smbios::build_bridgevm_pc_smbios;

fn le32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn le64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

fn image_slice(image: &BridgeVmPcBootInfoImage, gpa: u64, len: usize) -> &[u8] {
    let offset = usize::try_from(gpa - board::BOOT_INFO.base).unwrap();
    &image.bytes[offset..offset + len]
}

#[test]
fn v1_header_binds_cpu_ram_and_every_firmware_table() {
    let acpi = build_bridgevm_pc_acpi(4);
    let mut smbios = build_bridgevm_pc_smbios(4, 8 << 30);
    let image = assemble_bridgevm_pc_boot_info(4, 8 << 30, &acpi, &mut smbios).unwrap();

    assert_eq!(&image.bytes[..8], BOOT_INFO_MAGIC);
    assert_eq!(le32(&image.bytes, 8), board::BOARD_ABI_VERSION);
    assert_eq!(le32(&image.bytes, 12), BOOT_INFO_HEADER_SIZE as u32);
    assert_eq!(le32(&image.bytes, 16), board::BOOT_INFO.size as u32);
    assert_eq!(image.bytes[21], 1);
    assert_eq!(le64(&image.bytes, 24), image.rsdp_gpa);
    assert_eq!(le32(&image.bytes, 32), acpi.rsdp.len() as u32);
    assert_eq!(le64(&image.bytes, 40), image.acpi_tables_gpa);
    assert_eq!(le32(&image.bytes, 48), acpi.tables.len() as u32);
    assert_eq!(le64(&image.bytes, 56), image.smbios_anchor_gpa);
    assert_eq!(le32(&image.bytes, 64), smbios.anchor.len() as u32);
    assert_eq!(le64(&image.bytes, 72), image.smbios_tables_gpa);
    assert_eq!(le32(&image.bytes, 80), smbios.tables.len() as u32);
    assert_eq!(le64(&image.bytes, 88), board::RAM_BASE);
    assert_eq!(le64(&image.bytes, 96), 8 << 30);
    assert_eq!(le32(&image.bytes, 104), 4);
    assert_eq!(
        image.bytes[..BOOT_INFO_HEADER_SIZE]
            .iter()
            .fold(0u8, |sum, byte| sum.wrapping_add(*byte)),
        0
    );

    assert_eq!(
        image_slice(&image, image.rsdp_gpa, acpi.rsdp.len()),
        acpi.rsdp
    );
    assert_eq!(
        image_slice(&image, image.acpi_tables_gpa, acpi.tables.len()),
        acpi.tables
    );
    assert_eq!(
        image_slice(&image, image.smbios_anchor_gpa, smbios.anchor.len()),
        smbios.anchor
    );
    assert_eq!(
        image_slice(&image, image.smbios_tables_gpa, smbios.tables.len()),
        smbios.tables
    );
}

#[test]
fn smbios_entry_point_is_finalized_for_its_v1_slot() {
    let acpi = build_bridgevm_pc_acpi(1);
    let mut smbios = build_bridgevm_pc_smbios(1, 512 << 20);
    assert_eq!(le64(&smbios.anchor, 16), 0);
    assert_eq!(smbios.anchor[5], 0);

    let image = assemble_bridgevm_pc_boot_info(1, 512 << 20, &acpi, &mut smbios).unwrap();
    assert_eq!(le64(&smbios.anchor, 16), image.smbios_tables_gpa);
    assert_eq!(
        smbios
            .anchor
            .iter()
            .fold(0u8, |sum, byte| sum.wrapping_add(*byte)),
        0
    );
}

#[test]
fn v1_slots_are_non_overlapping_and_unused_bytes_remain_zero() {
    let acpi = build_bridgevm_pc_acpi(2);
    let mut smbios = build_bridgevm_pc_smbios(2, 2 << 30);
    let image = assemble_bridgevm_pc_boot_info(2, 2 << 30, &acpi, &mut smbios).unwrap();

    assert!(BOOT_INFO_RSDP_OFFSET + acpi.rsdp.len() <= BOOT_INFO_ACPI_OFFSET);
    assert!(BOOT_INFO_ACPI_OFFSET + acpi.tables.len() <= BOOT_INFO_SMBIOS_ANCHOR_OFFSET);
    assert!(BOOT_INFO_SMBIOS_ANCHOR_OFFSET + smbios.anchor.len() <= BOOT_INFO_SMBIOS_TABLES_OFFSET);
    assert!(BOOT_INFO_SMBIOS_TABLES_OFFSET + smbios.tables.len() <= image.bytes.len());
    assert!(image.bytes[BOOT_INFO_HEADER_SIZE..BOOT_INFO_RSDP_OFFSET]
        .iter()
        .all(|byte| *byte == 0));
}

#[test]
fn assembly_rejects_wrong_acpi_gpa_and_oversized_slots() {
    let mut acpi = build_bridgevm_pc_acpi(1);
    let mut smbios = build_bridgevm_pc_smbios(1, 512 << 20);
    acpi.tables_base += 0x1000;
    assert!(assemble_bridgevm_pc_boot_info(1, 512 << 20, &acpi, &mut smbios).is_err());

    let mut acpi = build_bridgevm_pc_acpi(1);
    let mut smbios = build_bridgevm_pc_smbios(1, 512 << 20);
    acpi.tables.resize(
        BOOT_INFO_SMBIOS_ANCHOR_OFFSET - BOOT_INFO_ACPI_OFFSET + 1,
        0,
    );
    assert!(assemble_bridgevm_pc_boot_info(1, 512 << 20, &acpi, &mut smbios).is_err());
}

#[test]
fn assembly_rejects_invalid_smbios_anchor_and_guest_shape() {
    let acpi = build_bridgevm_pc_acpi(1);
    let mut smbios = build_bridgevm_pc_smbios(1, 512 << 20);
    smbios.anchor[0] = 0;
    assert!(assemble_bridgevm_pc_boot_info(1, 512 << 20, &acpi, &mut smbios).is_err());

    let mut smbios = build_bridgevm_pc_smbios(1, 512 << 20);
    assert!(assemble_bridgevm_pc_boot_info(0, 512 << 20, &acpi, &mut smbios).is_err());
    assert!(assemble_bridgevm_pc_boot_info(1, 0, &acpi, &mut smbios).is_err());
}
