//! Split out of windows_arm.rs by responsibility.

use super::*;
use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmBootDiskLayoutOptions {
    pub disk_path: PathBuf,
    pub size_gib: u32,
    pub create: bool,
}

impl Default for WindowsArmBootDiskLayoutOptions {
    fn default() -> Self {
        Self {
            disk_path: PathBuf::from("windows-11-arm-hvf.raw"),
            size_gib: WINDOWS_ARM_BOOT_DISK_DEFAULT_SIZE_GIB,
            create: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmBootDiskPartition {
    pub name: &'static str,
    pub role: &'static str,
    pub type_guid: &'static str,
    pub start_lba: u64,
    pub end_lba: u64,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmBootDiskLayoutProbe {
    pub disk_path: PathBuf,
    pub requested_size_gib: u32,
    pub disk_size_bytes: Option<u64>,
    pub create_requested: bool,
    pub created: bool,
    pub reopened_for_verification: bool,
    pub protective_mbr_verified: bool,
    pub primary_gpt_verified: bool,
    pub backup_gpt_verified: bool,
    pub partition_entries_verified: bool,
    pub partitions: Vec<WindowsArmBootDiskPartition>,
    pub blockers: Vec<String>,
}

impl WindowsArmBootDiskLayoutProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("Windows 11 Arm HVF boot disk layout probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("HVF: not entered\n");
        output.push_str(
            "Guest execution: not entered; sparse raw GPT/UEFI Windows target disk layout\n",
        );
        output.push_str(&format!("Disk path: {}\n", self.disk_path.display()));
        output.push_str(&format!(
            "Requested size: {} GiB\n",
            self.requested_size_gib
        ));
        output.push_str(&format!(
            "Disk bytes: {}\n",
            render_optional_u64(self.disk_size_bytes)
        ));
        output.push_str(&format!("Create requested: {}\n", self.create_requested));
        output.push_str(&format!("Created: {}\n", self.created));
        output.push_str(&format!(
            "Reopened for verification: {}\n",
            self.reopened_for_verification
        ));
        output.push_str(&format!(
            "Protective MBR verified: {}\n",
            self.protective_mbr_verified
        ));
        output.push_str(&format!(
            "Primary GPT verified: {}\n",
            self.primary_gpt_verified
        ));
        output.push_str(&format!(
            "Backup GPT verified: {}\n",
            self.backup_gpt_verified
        ));
        output.push_str(&format!(
            "Partition entries verified: {}\n",
            self.partition_entries_verified
        ));
        output.push_str("Partitions:\n");
        for partition in &self.partitions {
            output.push_str(&format!(
                "- {}: {} - type {}, LBA {:#x}..{:#x}, bytes {:#x}\n",
                partition.name,
                partition.role,
                partition.type_guid,
                partition.start_lba,
                partition.end_lba,
                partition.size_bytes
            ));
        }
        if self.blockers.is_empty() {
            output.push_str("Blockers: none\n");
        } else {
            output.push_str("Blockers:\n");
            for blocker in &self.blockers {
                output.push_str(&format!("- {blocker}\n"));
            }
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_11_arm_boot_disk_layout_probe_creates_and_verifies_sparse_gpt() {
        let path = std::env::temp_dir().join(format!(
            "bridgevm-windows-arm-boot-disk-layout-{}-{}.raw",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&path);

        let probe = probe_windows_11_arm_boot_disk_layout(WindowsArmBootDiskLayoutOptions {
            disk_path: path.clone(),
            size_gib: WINDOWS_ARM_BOOT_DISK_MIN_SIZE_GIB,
            create: true,
        });
        let output = probe.render_text();
        let metadata = std::fs::metadata(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(probe.disk_path, path);
        assert_eq!(probe.requested_size_gib, WINDOWS_ARM_BOOT_DISK_MIN_SIZE_GIB);
        assert_eq!(
            probe.disk_size_bytes,
            gib_to_bytes(WINDOWS_ARM_BOOT_DISK_MIN_SIZE_GIB)
        );
        assert!(probe.create_requested);
        assert!(probe.created);
        assert!(probe.reopened_for_verification);
        assert!(probe.protective_mbr_verified);
        assert!(probe.primary_gpt_verified);
        assert!(probe.backup_gpt_verified);
        assert!(probe.partition_entries_verified);
        assert_eq!(
            metadata.len(),
            gib_to_bytes(WINDOWS_ARM_BOOT_DISK_MIN_SIZE_GIB).unwrap()
        );
        assert_eq!(probe.partitions.len(), 3);
        assert_eq!(probe.partitions[0].name, "EFI System Partition");
        assert_eq!(probe.partitions[1].name, "Microsoft Reserved");
        assert_eq!(probe.partitions[2].name, "Windows Basic Data");
        assert!(probe.blockers.is_empty());
        assert!(output.contains("Windows 11 Arm HVF boot disk layout probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("HVF: not entered"));
        assert!(output.contains("Create requested: true"));
        assert!(output.contains("Created: true"));
        assert!(output.contains("Protective MBR verified: true"));
        assert!(output.contains("Primary GPT verified: true"));
        assert!(output.contains("Backup GPT verified: true"));
        assert!(output.contains("Partition entries verified: true"));
        assert!(output.contains("EFI System Partition"));
        assert!(output.contains("Microsoft Reserved"));
        assert!(output.contains("Windows Basic Data"));
        assert!(output.contains("Blockers: none"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn windows_11_arm_boot_disk_layout_probe_without_create_is_metadata_safe() {
        let path = std::env::temp_dir().join(format!(
            "bridgevm-windows-arm-boot-disk-layout-missing-{}-{}.raw",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&path);

        let probe = probe_windows_11_arm_boot_disk_layout(WindowsArmBootDiskLayoutOptions {
            disk_path: path,
            size_gib: WINDOWS_ARM_BOOT_DISK_MIN_SIZE_GIB,
            create: false,
        });
        let output = probe.render_text();

        assert!(!probe.create_requested);
        assert!(!probe.created);
        assert!(!probe.reopened_for_verification);
        assert!(!probe.protective_mbr_verified);
        assert!(!probe.primary_gpt_verified);
        assert!(!probe.backup_gpt_verified);
        assert!(!probe.partition_entries_verified);
        assert_eq!(probe.partitions.len(), 3);
        assert!(probe
            .blockers
            .iter()
            .any(|blocker| blocker.contains("pass --create")));
        assert!(output.contains("Create requested: false"));
        assert!(output.contains("Created: false"));
        assert!(output.contains("disk file does not exist"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }
}
