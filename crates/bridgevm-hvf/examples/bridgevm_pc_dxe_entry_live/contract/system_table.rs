use super::{bytes_at, expect, u64_at};
use bridgevm_hvf::bridgevm_pc_boot_info::{BOOT_INFO_RSDP_OFFSET, BOOT_INFO_SMBIOS_ANCHOR_OFFSET};
use bridgevm_hvf::machine::bridgevm_pc as board;

const EFI_SYSTEM_TABLE_SIGNATURE: u64 = 0x5453_5953_2049_4249;
const ENTRY_COUNT_OFFSET: usize = 104;
const CONFIGURATION_TABLE_OFFSET: usize = 112;
const CONFIGURATION_ENTRY_SIZE: usize = 24;
const ACPI_20_GUID: [u8; 16] = [
    0x71, 0xe8, 0x68, 0x88, 0xf1, 0xe4, 0xd3, 0x11, 0xbc, 0x22, 0x00, 0x80, 0xc7, 0x3c, 0x88, 0x81,
];
const SMBIOS_3_GUID: [u8; 16] = [
    0x44, 0x15, 0xfd, 0xf2, 0x94, 0x97, 0x2c, 0x4a, 0x99, 0x2e, 0xe5, 0xbb, 0xcf, 0x20, 0xe3, 0x94,
];

#[derive(Debug, Eq, PartialEq)]
pub struct PublishedTables {
    pub entry_count: u64,
    pub acpi: u64,
    pub smbios: u64,
}

fn ram_offset(gpa: u64, label: &str) -> Result<usize, String> {
    gpa.checked_sub(board::RAM_BASE)
        .and_then(|offset| usize::try_from(offset).ok())
        .ok_or_else(|| format!("{label} {gpa:#x} is below RAM"))
}

pub fn validate(ram: &[u8], system_table: u64) -> Result<PublishedTables, String> {
    let table_offset = ram_offset(system_table, "DXE system table")?;
    expect(
        "EFI system-table signature",
        u64_at(ram, table_offset, "EFI system-table signature")?,
        EFI_SYSTEM_TABLE_SIGNATURE,
    )?;
    let entry_count = u64_at(
        ram,
        table_offset + ENTRY_COUNT_OFFSET,
        "EFI configuration-table count",
    )?;
    if !(5..=32).contains(&entry_count) {
        return Err(format!(
            "EFI configuration-table count is {entry_count}; expected 5..=32"
        ));
    }
    let configuration_table = u64_at(
        ram,
        table_offset + CONFIGURATION_TABLE_OFFSET,
        "EFI configuration-table pointer",
    )?;
    let configuration_offset = ram_offset(configuration_table, "EFI configuration table")?;
    let mut acpi = None;
    let mut smbios = None;
    for index in 0..usize::try_from(entry_count).map_err(|_| "entry count overflow")? {
        let offset = configuration_offset
            .checked_add(index * CONFIGURATION_ENTRY_SIZE)
            .ok_or_else(|| "EFI configuration-table offset overflow".to_string())?;
        let guid = bytes_at::<16>(ram, offset, "EFI configuration-table GUID")?;
        let value = u64_at(ram, offset + 16, "EFI configuration-table value")?;
        if guid == ACPI_20_GUID {
            if acpi.replace(value).is_some() {
                return Err("duplicate ACPI 2.0 configuration-table entry".to_string());
            }
        } else if guid == SMBIOS_3_GUID && smbios.replace(value).is_some() {
            return Err("duplicate SMBIOS 3 configuration-table entry".to_string());
        }
    }
    let acpi = acpi.ok_or_else(|| "ACPI 2.0 configuration-table entry is missing".to_string())?;
    let smbios =
        smbios.ok_or_else(|| "SMBIOS 3 configuration-table entry is missing".to_string())?;
    expect(
        "ACPI 2.0 table pointer",
        acpi,
        board::BOOT_INFO.base + BOOT_INFO_RSDP_OFFSET as u64,
    )?;
    expect(
        "SMBIOS 3 table pointer",
        smbios,
        board::BOOT_INFO.base + BOOT_INFO_SMBIOS_ANCHOR_OFFSET as u64,
    )?;
    Ok(PublishedTables {
        entry_count,
        acpi,
        smbios,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put_u64(ram: &mut [u8], offset: usize, value: u64) {
        ram[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }

    fn fixture(include_smbios: bool) -> (Vec<u8>, u64) {
        let mut ram = vec![0; 0x1000];
        let system = 0x100;
        let entries = 0x200;
        put_u64(&mut ram, system, EFI_SYSTEM_TABLE_SIGNATURE);
        put_u64(&mut ram, system + ENTRY_COUNT_OFFSET, 5);
        put_u64(
            &mut ram,
            system + CONFIGURATION_TABLE_OFFSET,
            board::RAM_BASE + entries as u64,
        );
        ram[entries + 3 * CONFIGURATION_ENTRY_SIZE..entries + 3 * CONFIGURATION_ENTRY_SIZE + 16]
            .copy_from_slice(&ACPI_20_GUID);
        put_u64(
            &mut ram,
            entries + 3 * CONFIGURATION_ENTRY_SIZE + 16,
            board::BOOT_INFO.base + BOOT_INFO_RSDP_OFFSET as u64,
        );
        if include_smbios {
            ram[entries + 4 * CONFIGURATION_ENTRY_SIZE
                ..entries + 4 * CONFIGURATION_ENTRY_SIZE + 16]
                .copy_from_slice(&SMBIOS_3_GUID);
        }
        put_u64(
            &mut ram,
            entries + 4 * CONFIGURATION_ENTRY_SIZE + 16,
            board::BOOT_INFO.base + BOOT_INFO_SMBIOS_ANCHOR_OFFSET as u64,
        );
        (ram, board::RAM_BASE + system as u64)
    }

    #[test]
    fn accepts_exact_platform_table_entries_among_core_entries() {
        let (ram, system) = fixture(true);
        assert_eq!(validate(&ram, system).unwrap().entry_count, 5);
    }

    #[test]
    fn rejects_a_missing_smbios_entry() {
        let (ram, system) = fixture(false);
        assert!(validate(&ram, system).unwrap_err().contains("SMBIOS 3"));
    }
}
