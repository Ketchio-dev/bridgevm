//! Versioned firmware handoff image for BridgeVM Virtual ARM PC v1.

use crate::acpi::{checksum, BridgeVmPcAcpiBlobs, BRIDGEVM_PC_ACPI_TABLES_GPA};
use crate::machine::bridgevm_pc as board;
use crate::smbios::SmbiosBlobs;

pub const BOOT_INFO_MAGIC: &[u8; 8] = b"BVMBOOT1";
pub const BOOT_INFO_HEADER_SIZE: usize = 112;
pub const BOOT_INFO_RSDP_OFFSET: usize = 0x1000;
pub const BOOT_INFO_ACPI_OFFSET: usize = 0x2000;
pub const BOOT_INFO_SMBIOS_ANCHOR_OFFSET: usize = 0xc000;
pub const BOOT_INFO_SMBIOS_TABLES_OFFSET: usize = 0xd000;

const _: () = assert!(BOOT_INFO_HEADER_SIZE <= BOOT_INFO_RSDP_OFFSET);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeVmPcBootInfoImage {
    pub bytes: Vec<u8>,
    pub rsdp_gpa: u64,
    pub acpi_tables_gpa: u64,
    pub smbios_anchor_gpa: u64,
    pub smbios_tables_gpa: u64,
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn checked_u32(value: usize, field: &str) -> Result<u32, String> {
    u32::try_from(value).map_err(|_| format!("BridgeVM boot-info {field} exceeds u32"))
}

fn place(image: &mut [u8], offset: usize, payload: &[u8], limit: usize) -> Result<(), String> {
    let end = offset
        .checked_add(payload.len())
        .ok_or_else(|| "BridgeVM boot-info placement overflow".to_string())?;
    if end > limit || limit > image.len() {
        return Err("BridgeVM boot-info payload exceeds its v1 slot".to_string());
    }
    image[offset..end].copy_from_slice(payload);
    Ok(())
}

pub(crate) fn assemble_bridgevm_pc_boot_info(
    cpu_count: u64,
    ram_size: u64,
    acpi: &BridgeVmPcAcpiBlobs,
    smbios: &mut SmbiosBlobs,
) -> Result<BridgeVmPcBootInfoImage, String> {
    if acpi.tables_base != BRIDGEVM_PC_ACPI_TABLES_GPA {
        return Err("BridgeVM PC ACPI tables are not at the v1 GPA".to_string());
    }
    if smbios.anchor.len() != 24 || &smbios.anchor[..5] != b"_SM3_" {
        return Err("BridgeVM PC SMBIOS anchor is not SMBIOS 3.0".to_string());
    }
    if !(1..=board::MAX_CPUS).contains(&cpu_count) || board::ram_region(ram_size).is_none() {
        return Err("BridgeVM boot-info CPU or RAM is outside the v1 contract".to_string());
    }

    let image_len = usize::try_from(board::BOOT_INFO.size)
        .map_err(|_| "BridgeVM boot-info aperture exceeds host usize".to_string())?;
    let mut image = vec![0u8; image_len];
    let rsdp_gpa = board::BOOT_INFO.base + BOOT_INFO_RSDP_OFFSET as u64;
    let smbios_anchor_gpa = board::BOOT_INFO.base + BOOT_INFO_SMBIOS_ANCHOR_OFFSET as u64;
    let smbios_tables_gpa = board::BOOT_INFO.base + BOOT_INFO_SMBIOS_TABLES_OFFSET as u64;

    smbios.anchor[16..24].copy_from_slice(&smbios_tables_gpa.to_le_bytes());
    smbios.anchor[5] = 0;
    smbios.anchor[5] = checksum(&smbios.anchor);

    place(
        &mut image,
        BOOT_INFO_RSDP_OFFSET,
        &acpi.rsdp,
        BOOT_INFO_ACPI_OFFSET,
    )?;
    place(
        &mut image,
        BOOT_INFO_ACPI_OFFSET,
        &acpi.tables,
        BOOT_INFO_SMBIOS_ANCHOR_OFFSET,
    )?;
    place(
        &mut image,
        BOOT_INFO_SMBIOS_ANCHOR_OFFSET,
        &smbios.anchor,
        BOOT_INFO_SMBIOS_TABLES_OFFSET,
    )?;
    place(
        &mut image,
        BOOT_INFO_SMBIOS_TABLES_OFFSET,
        &smbios.tables,
        image_len,
    )?;

    image[..8].copy_from_slice(BOOT_INFO_MAGIC);
    put_u32(&mut image, 8, board::BOARD_ABI_VERSION);
    put_u32(&mut image, 12, BOOT_INFO_HEADER_SIZE as u32);
    put_u32(&mut image, 16, image_len as u32);
    image[21] = 1;
    put_u64(&mut image, 24, rsdp_gpa);
    put_u32(&mut image, 32, checked_u32(acpi.rsdp.len(), "RSDP length")?);
    put_u64(&mut image, 40, acpi.tables_base);
    put_u32(
        &mut image,
        48,
        checked_u32(acpi.tables.len(), "ACPI length")?,
    );
    put_u64(&mut image, 56, smbios_anchor_gpa);
    put_u32(
        &mut image,
        64,
        checked_u32(smbios.anchor.len(), "SMBIOS anchor length")?,
    );
    put_u64(&mut image, 72, smbios_tables_gpa);
    put_u32(
        &mut image,
        80,
        checked_u32(smbios.tables.len(), "SMBIOS table length")?,
    );
    put_u64(&mut image, 88, board::RAM_BASE);
    put_u64(&mut image, 96, ram_size);
    put_u32(
        &mut image,
        104,
        u32::try_from(cpu_count).map_err(|_| "BridgeVM boot-info CPU count exceeds u32")?,
    );
    image[20] = checksum(&image[..BOOT_INFO_HEADER_SIZE]);

    Ok(BridgeVmPcBootInfoImage {
        bytes: image,
        rsdp_gpa,
        acpi_tables_gpa: acpi.tables_base,
        smbios_anchor_gpa,
        smbios_tables_gpa,
    })
}

#[cfg(test)]
#[path = "bridgevm_pc_boot_info_tests.rs"]
mod tests;
