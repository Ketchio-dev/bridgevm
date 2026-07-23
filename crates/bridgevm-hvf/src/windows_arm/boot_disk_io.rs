//! Split out of windows_arm.rs by responsibility.

use super::*;
use crate::*;

pub(crate) fn gib_to_bytes(size_gib: u32) -> Option<u64> {
    u64::from(size_gib).checked_mul(1024 * 1024 * 1024)
}

pub(crate) fn align_lba(value: u64, alignment: u64) -> u64 {
    if alignment == 0 {
        return value;
    }
    value.div_ceil(alignment) * alignment
}

pub(crate) fn windows_arm_boot_disk_partitions(
    disk_size_bytes: u64,
) -> Result<Vec<WindowsArmBootDiskPartition>, String> {
    if disk_size_bytes < gib_to_bytes(WINDOWS_ARM_BOOT_DISK_MIN_SIZE_GIB).unwrap_or(0) {
        return Err(format!(
            "disk is smaller than {WINDOWS_ARM_BOOT_DISK_MIN_SIZE_GIB} GiB"
        ));
    }
    if disk_size_bytes % GPT_SECTOR_BYTES != 0 {
        return Err("disk size is not 512-byte sector aligned".to_string());
    }
    let total_lbas = disk_size_bytes / GPT_SECTOR_BYTES;
    if total_lbas <= GPT_FIRST_USABLE_LBA + GPT_ENTRY_ARRAY_SECTORS + 1 {
        return Err("disk does not have enough sectors for GPT headers".to_string());
    }
    let backup_header_lba = total_lbas - 1;
    let last_usable_lba = backup_header_lba - GPT_ENTRY_ARRAY_SECTORS - 1;

    let esp_start_lba = align_lba(GPT_FIRST_USABLE_LBA, WINDOWS_ARM_BOOT_DISK_ALIGNMENT_LBA);
    let esp_sectors = WINDOWS_ARM_ESP_SIZE_BYTES / GPT_SECTOR_BYTES;
    let esp_end_lba = esp_start_lba + esp_sectors - 1;
    let msr_start_lba = align_lba(esp_end_lba + 1, WINDOWS_ARM_BOOT_DISK_ALIGNMENT_LBA);
    let msr_sectors = WINDOWS_ARM_MSR_SIZE_BYTES / GPT_SECTOR_BYTES;
    let msr_end_lba = msr_start_lba + msr_sectors - 1;
    let windows_start_lba = align_lba(msr_end_lba + 1, WINDOWS_ARM_BOOT_DISK_ALIGNMENT_LBA);
    if windows_start_lba > last_usable_lba {
        return Err("disk does not have room for a Windows data partition".to_string());
    }

    Ok(vec![
        WindowsArmBootDiskPartition {
            name: "EFI System Partition",
            role: "UEFI boot files and Windows Boot Manager target",
            type_guid: "C12A7328-F81F-11D2-BA4B-00A0C93EC93B",
            start_lba: esp_start_lba,
            end_lba: esp_end_lba,
            size_bytes: WINDOWS_ARM_ESP_SIZE_BYTES,
        },
        WindowsArmBootDiskPartition {
            name: "Microsoft Reserved",
            role: "Windows GPT reserved partition",
            type_guid: "E3C9E316-0B5C-4DB8-817D-F92DF00215AE",
            start_lba: msr_start_lba,
            end_lba: msr_end_lba,
            size_bytes: WINDOWS_ARM_MSR_SIZE_BYTES,
        },
        WindowsArmBootDiskPartition {
            name: "Windows Basic Data",
            role: "Windows installation target partition",
            type_guid: "EBD0A0A2-B9E5-4433-87C0-68B6B72699C7",
            start_lba: windows_start_lba,
            end_lba: last_usable_lba,
            size_bytes: (last_usable_lba - windows_start_lba + 1) * GPT_SECTOR_BYTES,
        },
    ])
}

pub(crate) fn write_windows_arm_boot_disk_layout(
    path: &PathBuf,
    disk_size_bytes: u64,
) -> Result<(), String> {
    let partitions = windows_arm_boot_disk_partitions(disk_size_bytes)?;
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    file.set_len(disk_size_bytes)
        .map_err(|error| error.to_string())?;

    let total_lbas = disk_size_bytes / GPT_SECTOR_BYTES;
    let backup_header_lba = total_lbas - 1;
    let backup_entry_lba = backup_header_lba - GPT_ENTRY_ARRAY_SECTORS;
    let last_usable_lba = backup_entry_lba - 1;
    let disk_guid = stable_guid_bytes(path, "disk", disk_size_bytes);
    let entries = build_gpt_entry_array(path, disk_size_bytes, &partitions);
    let entries_crc32 = crc32(&entries);

    write_protective_mbr(&mut file, total_lbas)?;
    write_all_at(
        &mut file,
        GPT_PRIMARY_ENTRY_LBA * GPT_SECTOR_BYTES,
        &entries,
    )?;
    let primary_header = build_gpt_header(
        GPT_PRIMARY_HEADER_LBA,
        backup_header_lba,
        GPT_FIRST_USABLE_LBA,
        last_usable_lba,
        disk_guid,
        GPT_PRIMARY_ENTRY_LBA,
        entries_crc32,
    );
    write_all_at(
        &mut file,
        GPT_PRIMARY_HEADER_LBA * GPT_SECTOR_BYTES,
        &primary_header,
    )?;
    write_all_at(&mut file, backup_entry_lba * GPT_SECTOR_BYTES, &entries)?;
    let backup_header = build_gpt_header(
        backup_header_lba,
        GPT_PRIMARY_HEADER_LBA,
        GPT_FIRST_USABLE_LBA,
        last_usable_lba,
        disk_guid,
        backup_entry_lba,
        entries_crc32,
    );
    write_all_at(
        &mut file,
        backup_header_lba * GPT_SECTOR_BYTES,
        &backup_header,
    )?;
    file.sync_all().map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn verify_windows_arm_boot_disk_layout(
    path: &PathBuf,
) -> Result<WindowsArmBootDiskLayoutVerification, String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let disk_size_bytes = file.metadata().map_err(|error| error.to_string())?.len();
    let partitions = windows_arm_boot_disk_partitions(disk_size_bytes)?;
    let total_lbas = disk_size_bytes / GPT_SECTOR_BYTES;
    let backup_header_lba = total_lbas - 1;
    let backup_entry_lba = backup_header_lba - GPT_ENTRY_ARRAY_SECTORS;
    let last_usable_lba = backup_entry_lba - 1;
    let entries = read_exact_at(
        &mut file,
        GPT_PRIMARY_ENTRY_LBA * GPT_SECTOR_BYTES,
        GPT_ENTRY_ARRAY_BYTES,
    )?;
    let entries_crc32 = crc32(&entries);

    verify_protective_mbr(&mut file, total_lbas)?;
    let primary_header = read_gpt_header(&mut file, GPT_PRIMARY_HEADER_LBA)?;
    verify_gpt_header(
        &primary_header,
        GPT_PRIMARY_HEADER_LBA,
        backup_header_lba,
        GPT_PRIMARY_ENTRY_LBA,
        GPT_FIRST_USABLE_LBA,
        last_usable_lba,
        entries_crc32,
    )?;
    verify_gpt_entries(&entries, &partitions)?;

    let backup_entries = read_exact_at(
        &mut file,
        backup_entry_lba * GPT_SECTOR_BYTES,
        GPT_ENTRY_ARRAY_BYTES,
    )?;
    if crc32(&backup_entries) != entries_crc32 {
        return Err("backup GPT partition-entry CRC does not match primary".to_string());
    }
    let backup_header = read_gpt_header(&mut file, backup_header_lba)?;
    verify_gpt_header(
        &backup_header,
        backup_header_lba,
        GPT_PRIMARY_HEADER_LBA,
        backup_entry_lba,
        GPT_FIRST_USABLE_LBA,
        last_usable_lba,
        entries_crc32,
    )?;

    Ok(WindowsArmBootDiskLayoutVerification {
        protective_mbr_verified: true,
        primary_gpt_verified: true,
        backup_gpt_verified: true,
        partition_entries_verified: true,
        disk_size_bytes,
    })
}

pub(crate) fn write_protective_mbr(file: &mut File, total_lbas: u64) -> Result<(), String> {
    let mut mbr = [0_u8; GPT_SECTOR_BYTES_USIZE];
    let partition_len = total_lbas.saturating_sub(1).min(u64::from(u32::MAX)) as u32;
    mbr[446 + 1] = 0xff;
    mbr[446 + 2] = 0xff;
    mbr[446 + 3] = 0xff;
    mbr[446 + 4] = 0xee;
    mbr[446 + 5] = 0xff;
    mbr[446 + 6] = 0xff;
    mbr[446 + 7] = 0xff;
    mbr[446 + 8..446 + 12].copy_from_slice(&1_u32.to_le_bytes());
    mbr[446 + 12..446 + 16].copy_from_slice(&partition_len.to_le_bytes());
    mbr[510] = 0x55;
    mbr[511] = 0xaa;
    write_all_at(file, 0, &mbr)
}

pub(crate) fn verify_protective_mbr(file: &mut File, total_lbas: u64) -> Result<(), String> {
    let mbr = read_exact_at(file, 0, GPT_SECTOR_BYTES_USIZE)?;
    if mbr[510] != 0x55 || mbr[511] != 0xaa {
        return Err("protective MBR signature is missing".to_string());
    }
    if mbr[446 + 4] != 0xee {
        return Err("protective MBR does not contain a GPT protective partition".to_string());
    }
    let start_lba = u32::from_le_bytes(
        mbr[446 + 8..446 + 12]
            .try_into()
            .map_err(|_| "protective MBR start LBA parse failed".to_string())?,
    );
    if start_lba != 1 {
        return Err("protective MBR start LBA is not 1".to_string());
    }
    let partition_len = u32::from_le_bytes(
        mbr[446 + 12..446 + 16]
            .try_into()
            .map_err(|_| "protective MBR length parse failed".to_string())?,
    );
    let expected_len = total_lbas.saturating_sub(1).min(u64::from(u32::MAX)) as u32;
    if partition_len != expected_len {
        return Err("protective MBR length does not cover the disk".to_string());
    }
    Ok(())
}

pub(crate) fn build_gpt_entry_array(
    path: &Path,
    disk_size_bytes: u64,
    partitions: &[WindowsArmBootDiskPartition],
) -> Vec<u8> {
    let mut entries = vec![0_u8; GPT_ENTRY_ARRAY_BYTES];
    for (index, partition) in partitions.iter().enumerate() {
        let type_guid = match partition.type_guid {
            "C12A7328-F81F-11D2-BA4B-00A0C93EC93B" => EFI_SYSTEM_PARTITION_GUID,
            "E3C9E316-0B5C-4DB8-817D-F92DF00215AE" => MICROSOFT_RESERVED_PARTITION_GUID,
            "EBD0A0A2-B9E5-4433-87C0-68B6B72699C7" => MICROSOFT_BASIC_DATA_PARTITION_GUID,
            _ => [0_u8; 16],
        };
        let unique_guid = stable_guid_bytes(path, partition.name, disk_size_bytes);
        let offset = index * GPT_ENTRY_SIZE;
        entries[offset..offset + 16].copy_from_slice(&type_guid);
        entries[offset + 16..offset + 32].copy_from_slice(&unique_guid);
        entries[offset + 32..offset + 40].copy_from_slice(&partition.start_lba.to_le_bytes());
        entries[offset + 40..offset + 48].copy_from_slice(&partition.end_lba.to_le_bytes());
        for (name_index, code_unit) in partition.name.encode_utf16().take(36).enumerate() {
            let name_offset = offset + 56 + name_index * 2;
            entries[name_offset..name_offset + 2].copy_from_slice(&code_unit.to_le_bytes());
        }
    }
    entries
}

pub(crate) fn build_gpt_header(
    current_lba: u64,
    backup_lba: u64,
    first_usable_lba: u64,
    last_usable_lba: u64,
    disk_guid: [u8; 16],
    entries_lba: u64,
    entries_crc32: u32,
) -> [u8; GPT_SECTOR_BYTES_USIZE] {
    let mut header = [0_u8; GPT_SECTOR_BYTES_USIZE];
    header[0..8].copy_from_slice(b"EFI PART");
    header[8..12].copy_from_slice(&0x0001_0000_u32.to_le_bytes());
    header[12..16].copy_from_slice(&92_u32.to_le_bytes());
    header[24..32].copy_from_slice(&current_lba.to_le_bytes());
    header[32..40].copy_from_slice(&backup_lba.to_le_bytes());
    header[40..48].copy_from_slice(&first_usable_lba.to_le_bytes());
    header[48..56].copy_from_slice(&last_usable_lba.to_le_bytes());
    header[56..72].copy_from_slice(&disk_guid);
    header[72..80].copy_from_slice(&entries_lba.to_le_bytes());
    header[80..84].copy_from_slice(&(GPT_ENTRY_COUNT as u32).to_le_bytes());
    header[84..88].copy_from_slice(&(GPT_ENTRY_SIZE as u32).to_le_bytes());
    header[88..92].copy_from_slice(&entries_crc32.to_le_bytes());
    let header_crc32 = crc32(&header[0..92]);
    header[16..20].copy_from_slice(&header_crc32.to_le_bytes());
    header
}

pub(crate) fn read_gpt_header(file: &mut File, lba: u64) -> Result<GptHeader, String> {
    let mut header = read_exact_at(file, lba * GPT_SECTOR_BYTES, GPT_SECTOR_BYTES_USIZE)?;
    if &header[0..8] != b"EFI PART" {
        return Err(format!(
            "GPT header at LBA {lba:#x} has an invalid signature"
        ));
    }
    let header_size = u32::from_le_bytes(
        header[12..16]
            .try_into()
            .map_err(|_| "GPT header size parse failed".to_string())?,
    ) as usize;
    if !(92..=GPT_SECTOR_BYTES_USIZE).contains(&header_size) {
        return Err("GPT header size is invalid".to_string());
    }
    let stored_crc = u32::from_le_bytes(
        header[16..20]
            .try_into()
            .map_err(|_| "GPT header CRC parse failed".to_string())?,
    );
    header[16..20].fill(0);
    let computed_crc = crc32(&header[0..header_size]);
    if stored_crc != computed_crc {
        return Err("GPT header CRC verification failed".to_string());
    }
    Ok(GptHeader {
        current_lba: u64_from_le(&header, 24)?,
        backup_lba: u64_from_le(&header, 32)?,
        first_usable_lba: u64_from_le(&header, 40)?,
        last_usable_lba: u64_from_le(&header, 48)?,
        entries_lba: u64_from_le(&header, 72)?,
        entry_count: u32_from_le(&header, 80)?,
        entry_size: u32_from_le(&header, 84)?,
        entries_crc32: u32_from_le(&header, 88)?,
    })
}

pub(crate) fn verify_gpt_header(
    header: &GptHeader,
    current_lba: u64,
    backup_lba: u64,
    entries_lba: u64,
    first_usable_lba: u64,
    last_usable_lba: u64,
    entries_crc32: u32,
) -> Result<(), String> {
    if header.current_lba != current_lba {
        return Err("GPT header current LBA mismatch".to_string());
    }
    if header.backup_lba != backup_lba {
        return Err("GPT header backup LBA mismatch".to_string());
    }
    if header.entries_lba != entries_lba {
        return Err("GPT header partition-entry LBA mismatch".to_string());
    }
    if header.first_usable_lba != first_usable_lba || header.last_usable_lba != last_usable_lba {
        return Err("GPT header usable LBA range mismatch".to_string());
    }
    if header.entry_count != GPT_ENTRY_COUNT as u32 || header.entry_size != GPT_ENTRY_SIZE as u32 {
        return Err("GPT header partition-entry geometry mismatch".to_string());
    }
    if header.entries_crc32 != entries_crc32 {
        return Err("GPT partition-entry CRC mismatch".to_string());
    }
    Ok(())
}

pub(crate) fn verify_gpt_entries(
    entries: &[u8],
    partitions: &[WindowsArmBootDiskPartition],
) -> Result<(), String> {
    for (index, partition) in partitions.iter().enumerate() {
        let offset = index * GPT_ENTRY_SIZE;
        let expected_type_guid = match partition.type_guid {
            "C12A7328-F81F-11D2-BA4B-00A0C93EC93B" => EFI_SYSTEM_PARTITION_GUID,
            "E3C9E316-0B5C-4DB8-817D-F92DF00215AE" => MICROSOFT_RESERVED_PARTITION_GUID,
            "EBD0A0A2-B9E5-4433-87C0-68B6B72699C7" => MICROSOFT_BASIC_DATA_PARTITION_GUID,
            _ => return Err("unknown partition type GUID".to_string()),
        };
        if entries[offset..offset + 16] != expected_type_guid {
            return Err(format!("partition {} type GUID mismatch", partition.name));
        }
        if u64_from_le(entries, offset + 32)? != partition.start_lba
            || u64_from_le(entries, offset + 40)? != partition.end_lba
        {
            return Err(format!("partition {} LBA range mismatch", partition.name));
        }
        if decode_gpt_partition_name(&entries[offset + 56..offset + GPT_ENTRY_SIZE])
            != partition.name
        {
            return Err(format!("partition {} name mismatch", partition.name));
        }
    }
    Ok(())
}
