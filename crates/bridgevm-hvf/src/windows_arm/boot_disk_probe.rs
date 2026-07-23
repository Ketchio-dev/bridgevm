//! Split out of windows_arm.rs by responsibility.

use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WindowsArmBootDiskLayoutVerification {
    pub(crate) protective_mbr_verified: bool,
    pub(crate) primary_gpt_verified: bool,
    pub(crate) backup_gpt_verified: bool,
    pub(crate) partition_entries_verified: bool,
    pub(crate) disk_size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GptHeader {
    pub(crate) current_lba: u64,
    pub(crate) backup_lba: u64,
    pub(crate) first_usable_lba: u64,
    pub(crate) last_usable_lba: u64,
    pub(crate) entries_lba: u64,
    pub(crate) entry_count: u32,
    pub(crate) entry_size: u32,
    pub(crate) entries_crc32: u32,
}

pub fn probe_windows_11_arm_boot_disk_layout(
    options: WindowsArmBootDiskLayoutOptions,
) -> WindowsArmBootDiskLayoutProbe {
    let mut blockers = Vec::new();
    let requested_size_bytes = match gib_to_bytes(options.size_gib) {
        Some(bytes) if options.size_gib >= WINDOWS_ARM_BOOT_DISK_MIN_SIZE_GIB => bytes,
        Some(_) => {
            blockers.push(format!(
                "--size-gib must be at least {WINDOWS_ARM_BOOT_DISK_MIN_SIZE_GIB} for the Windows Arm GPT layout"
            ));
            0
        }
        None => {
            blockers.push("--size-gib is too large to represent safely".to_string());
            0
        }
    };
    let mut disk_size_bytes = (requested_size_bytes > 0).then_some(requested_size_bytes);
    let mut created = false;
    let mut reopened_for_verification = false;
    let mut verification = WindowsArmBootDiskLayoutVerification {
        protective_mbr_verified: false,
        primary_gpt_verified: false,
        backup_gpt_verified: false,
        partition_entries_verified: false,
        disk_size_bytes: requested_size_bytes,
    };

    if requested_size_bytes > 0 {
        if options.create {
            if options.disk_path.exists() {
                blockers.push(format!(
                    "disk path already exists; refusing to overwrite {}",
                    options.disk_path.display()
                ));
            } else {
                match write_windows_arm_boot_disk_layout(&options.disk_path, requested_size_bytes) {
                    Ok(()) => {
                        created = true;
                        match verify_windows_arm_boot_disk_layout(&options.disk_path) {
                            Ok(result) => {
                                reopened_for_verification = true;
                                disk_size_bytes = Some(result.disk_size_bytes);
                                verification = result;
                            }
                            Err(error) => blockers.push(format!(
                                "created disk could not be reopened and verified: {error}"
                            )),
                        }
                    }
                    Err(error) => blockers.push(format!("create failed: {error}")),
                }
            }
        } else {
            match std::fs::metadata(&options.disk_path) {
                Ok(metadata) => {
                    disk_size_bytes = Some(metadata.len());
                    match verify_windows_arm_boot_disk_layout(&options.disk_path) {
                        Ok(result) => {
                            reopened_for_verification = true;
                            verification = result;
                        }
                        Err(error) => blockers
                            .push(format!("existing disk layout verification failed: {error}")),
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => blockers.push(
                    "disk file does not exist; pass --create to write a sparse raw GPT layout"
                        .to_string(),
                ),
                Err(error) => blockers.push(format!("disk metadata read failed: {error}")),
            }
        }
    }

    let partitions = disk_size_bytes
        .and_then(|bytes| windows_arm_boot_disk_partitions(bytes).ok())
        .unwrap_or_default();

    WindowsArmBootDiskLayoutProbe {
        disk_path: options.disk_path,
        requested_size_gib: options.size_gib,
        disk_size_bytes,
        create_requested: options.create,
        created,
        reopened_for_verification,
        protective_mbr_verified: verification.protective_mbr_verified,
        primary_gpt_verified: verification.primary_gpt_verified,
        backup_gpt_verified: verification.backup_gpt_verified,
        partition_entries_verified: verification.partition_entries_verified,
        partitions,
        blockers,
    }
}
