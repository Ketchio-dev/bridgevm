//! Split out of windows_arm.rs by responsibility.

use super::*;
use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiFirmwareHandoffOptions {
    pub firmware_path: PathBuf,
    pub vars_template_path: Option<PathBuf>,
    pub vars_path: Option<PathBuf>,
    pub create_vars: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UefiFirmwareVolumeMetadata {
    pub offset: u64,
    pub length_bytes: u64,
    pub header_length: u16,
    pub checksum_verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiFirmwareHandoffProbe {
    pub firmware_path: PathBuf,
    pub firmware_bytes: Option<u64>,
    pub firmware_slot_ipa: u64,
    pub firmware_slot_bytes: u64,
    pub firmware_volume: Option<UefiFirmwareVolumeMetadata>,
    pub firmware_verified: bool,
    pub vars_template_path: Option<PathBuf>,
    pub vars_template_bytes: Option<u64>,
    pub vars_template_verified: bool,
    pub vars_path: Option<PathBuf>,
    pub vars_bytes: Option<u64>,
    pub vars_slot_ipa: u64,
    pub vars_slot_bytes: u64,
    pub vars_created: bool,
    pub vars_reopened_for_verification: bool,
    pub vars_volume: Option<UefiFirmwareVolumeMetadata>,
    pub vars_verified: bool,
    pub planned_reset_vector_ipa: Option<u64>,
    pub blockers: Vec<String>,
}

impl WindowsArmUefiFirmwareHandoffProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("Windows 11 Arm HVF UEFI firmware handoff probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("HVF: not entered\n");
        output.push_str(
            "Guest execution: not entered; AArch64 UEFI firmware and vars pflash handoff\n",
        );
        output.push_str(&format!(
            "Firmware path: {}\n",
            self.firmware_path.display()
        ));
        output.push_str(&format!(
            "Firmware bytes: {}\n",
            render_optional_u64(self.firmware_bytes)
        ));
        output.push_str(&format!(
            "Firmware slot IPA: {:#x}\n",
            self.firmware_slot_ipa
        ));
        output.push_str(&format!(
            "Firmware slot bytes: {:#x}\n",
            self.firmware_slot_bytes
        ));
        output.push_str(&format!("Firmware verified: {}\n", self.firmware_verified));
        render_uefi_volume_metadata("Firmware volume", &self.firmware_volume, &mut output);
        output.push_str(&format!(
            "Vars template path: {}\n",
            self.vars_template_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "not provided".to_string())
        ));
        output.push_str(&format!(
            "Vars template bytes: {}\n",
            render_optional_u64(self.vars_template_bytes)
        ));
        output.push_str(&format!(
            "Vars template verified: {}\n",
            self.vars_template_verified
        ));
        output.push_str(&format!(
            "Vars path: {}\n",
            self.vars_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "not provided".to_string())
        ));
        output.push_str(&format!(
            "Vars bytes: {}\n",
            render_optional_u64(self.vars_bytes)
        ));
        output.push_str(&format!("Vars slot IPA: {:#x}\n", self.vars_slot_ipa));
        output.push_str(&format!("Vars slot bytes: {:#x}\n", self.vars_slot_bytes));
        output.push_str(&format!("Vars created: {}\n", self.vars_created));
        output.push_str(&format!(
            "Vars reopened for verification: {}\n",
            self.vars_reopened_for_verification
        ));
        output.push_str(&format!("Vars verified: {}\n", self.vars_verified));
        render_uefi_volume_metadata("Vars volume", &self.vars_volume, &mut output);
        output.push_str(&format!(
            "Planned reset vector IPA: {}\n",
            render_optional_u64(self.planned_reset_vector_ipa)
        ));
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiPflashMapOptions {
    pub firmware_path: PathBuf,
    pub vars_template_path: Option<PathBuf>,
    pub vars_path: Option<PathBuf>,
    pub create_vars: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiFirmwareRunLoopOptions {
    pub pflash: WindowsArmUefiPflashMapOptions,
    pub execution: WindowsArmUefiFirmwareRunLoopExecutionOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiFirmwareDeviceDiscoveryProbe {
    pub run_loop: WindowsArmUefiFirmwareRunLoopProbe,
}

impl WindowsArmUefiFirmwareDeviceDiscoveryProbe {
    pub fn device_boundary_reached(&self) -> bool {
        self.run_loop
            .low_vector_post_repair_first_device_interaction_observed
            || self
                .run_loop
                .low_vector_post_repair_first_unhandled_access_observed
            || self.run_loop.handled_mmio_read_count > 0
            || self.run_loop.handled_mmio_write_count > 0
            || self.run_loop.handled_icc_read_count > 0
            || self.run_loop.handled_icc_write_count > 0
    }

    pub fn device_discovery_ready(&self) -> bool {
        self.device_boundary_reached()
            && !self
                .run_loop
                .low_vector_post_repair_first_unhandled_access_observed
            && self.run_loop.blockers.is_empty()
    }

    pub fn boundary_status(&self) -> &'static str {
        if !self.device_boundary_reached() {
            "not reached"
        } else if self
            .run_loop
            .low_vector_post_repair_first_unhandled_access_observed
        {
            "reached-unhandled"
        } else if self.device_discovery_ready() {
            "reached-handled"
        } else {
            "reached-blocked"
        }
    }

    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("Windows 11 Arm HVF UEFI firmware device-discovery probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Windows boot: not claimed\n");
        output.push_str("Underlying probe: windows-firmware-run-loop-probe\n");
        output.push_str(&format!(
            "Device discovery boundary reached: {}\n",
            self.device_boundary_reached()
        ));
        output.push_str(&format!(
            "Device discovery boundary status: {}\n",
            self.boundary_status()
        ));
        output.push_str(&format!(
            "Device discovery ready: {}\n",
            self.device_discovery_ready()
        ));
        output.push_str(&format!(
            "First post-repair device interaction observed: {}\n",
            self.run_loop
                .low_vector_post_repair_first_device_interaction_observed
        ));
        output.push_str(&format!(
            "First post-repair unhandled access observed: {}\n",
            self.run_loop
                .low_vector_post_repair_first_unhandled_access_observed
        ));
        output.push_str(&format!(
            "Handled MMIO access count: {}\n",
            self.run_loop.handled_mmio_read_count + self.run_loop.handled_mmio_write_count
        ));
        output.push_str(&format!(
            "Handled ICC access count: {}\n",
            self.run_loop.handled_icc_read_count + self.run_loop.handled_icc_write_count
        ));
        if !self.device_boundary_reached() {
            output.push_str(
                "Device discovery blocker: firmware has not reached a non-diagnostic MMIO/sysreg boundary yet\n",
            );
        } else if self
            .run_loop
            .low_vector_post_repair_first_unhandled_access_observed
        {
            output.push_str(
                "Device discovery blocker: first firmware device boundary was unhandled\n",
            );
        } else if !self.run_loop.blockers.is_empty() {
            output
                .push_str("Device discovery blocker: underlying firmware run-loop has blockers\n");
        } else {
            output.push_str("Device discovery blocker: none\n");
        }
        output.push_str("Underlying firmware run-loop report:\n");
        output.push_str(&self.run_loop.render_text());
        output
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiFirmwareRunLoopExecutionOptions {
    pub allow_loop: bool,
    pub requested_exits: u32,
    pub guest_ram_mib: u32,
    pub watchdog_timeout_ms: u64,
    pub map_low_pflash_alias: bool,
    pub seed_diagnostic_vector: bool,
    pub seed_guest_ram_diagnostic_vector: bool,
    pub seed_executable_diagnostic_vector: bool,
    pub try_recommended_vector_base_vbar: bool,
    pub continue_after_recommended_vector_base_vbar: bool,
    pub repair_low_vector_diagnostic_page: bool,
    pub remap_low_vector_to_recommended_vector: bool,
    pub continue_after_low_vector_repair: bool,
    pub restore_low_vector_slot_before_eret: bool,
    pub wire_interrupt_timer: bool,
    pub stop_at_first_post_repair_device_boundary: bool,
    pub installer_iso_path: Option<PathBuf>,
    pub writable_target_disk_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiPflashSlotMap {
    pub name: &'static str,
    pub path: PathBuf,
    pub ipa_start: u64,
    pub slot_bytes: u64,
    pub source_bytes: u64,
    pub copied_bytes: u64,
    pub zero_padding_bytes: u64,
    pub writable: bool,
    pub prefix_verified: bool,
    pub padding_zeroed: bool,
}

impl WindowsArmUefiPflashSlotMap {
    pub fn ipa_end_exclusive(&self) -> u64 {
        self.ipa_start.saturating_add(self.slot_bytes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiPflashMapProbe {
    pub firmware_path: PathBuf,
    pub vars_path: Option<PathBuf>,
    pub vars_created: bool,
    pub firmware_verified: bool,
    pub vars_verified: bool,
    pub firmware_slot: Option<WindowsArmUefiPflashSlotMap>,
    pub vars_slot: Option<WindowsArmUefiPflashSlotMap>,
    pub pflash_region_start: u64,
    pub pflash_region_bytes: u64,
    pub pflash_slots_non_overlapping: bool,
    pub guest_ram_overlap_verified: bool,
    pub device_mmio_overlap_verified: bool,
    pub pflash_map_verified: bool,
    pub planned_reset_vector_ipa: Option<u64>,
    pub blockers: Vec<String>,
}

impl WindowsArmUefiPflashMapProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("Windows 11 Arm HVF UEFI pflash map probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("HVF: not entered\n");
        output.push_str(
            "Guest execution: not entered; AArch64 UEFI pflash slots loaded into memory images\n",
        );
        output.push_str(&format!(
            "Firmware path: {}\n",
            self.firmware_path.display()
        ));
        output.push_str(&format!(
            "Vars path: {}\n",
            self.vars_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "not provided".to_string())
        ));
        output.push_str(&format!("Vars created: {}\n", self.vars_created));
        output.push_str(&format!("Firmware verified: {}\n", self.firmware_verified));
        output.push_str(&format!("Vars verified: {}\n", self.vars_verified));
        output.push_str(&format!(
            "Pflash region: {:#x}..{:#x}\n",
            self.pflash_region_start,
            self.pflash_region_start
                .saturating_add(self.pflash_region_bytes)
        ));
        render_uefi_pflash_slot("Firmware pflash", &self.firmware_slot, &mut output);
        render_uefi_pflash_slot("Vars pflash", &self.vars_slot, &mut output);
        output.push_str(&format!(
            "Pflash slots non-overlapping: {}\n",
            self.pflash_slots_non_overlapping
        ));
        output.push_str(&format!(
            "Guest RAM overlap verified: {}\n",
            self.guest_ram_overlap_verified
        ));
        output.push_str(&format!(
            "Device MMIO overlap verified: {}\n",
            self.device_mmio_overlap_verified
        ));
        output.push_str(&format!(
            "Pflash map verified: {}\n",
            self.pflash_map_verified
        ));
        output.push_str(&format!(
            "Planned reset vector IPA: {}\n",
            render_optional_u64(self.planned_reset_vector_ipa)
        ));
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiPflashHvfMapProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub firmware_memory_allocated: bool,
    pub vars_memory_allocated: bool,
    pub firmware_memory_populated: bool,
    pub vars_memory_populated: bool,
    pub firmware_memory_mapped: bool,
    pub vars_memory_mapped: bool,
    pub firmware_memory_unmapped: bool,
    pub vars_memory_unmapped: bool,
    pub firmware_memory_deallocated: bool,
    pub vars_memory_deallocated: bool,
    pub vm_destroyed: bool,
    pub host: HvfHostCapabilities,
    pub pflash_map_verified: bool,
    pub firmware_slot_ipa: u64,
    pub vars_slot_ipa: u64,
    pub slot_bytes: u64,
    pub firmware_source_bytes: Option<u64>,
    pub vars_source_bytes: Option<u64>,
    pub firmware_map_flags: &'static str,
    pub vars_map_flags: &'static str,
    pub vm_create_status: Option<i32>,
    pub firmware_allocate_status: Option<i32>,
    pub vars_allocate_status: Option<i32>,
    pub firmware_map_status: Option<i32>,
    pub vars_map_status: Option<i32>,
    pub firmware_unmap_status: Option<i32>,
    pub vars_unmap_status: Option<i32>,
    pub firmware_deallocate_status: Option<i32>,
    pub vars_deallocate_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl WindowsArmUefiPflashHvfMapProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("Windows 11 Arm HVF UEFI pflash HVF map/unmap probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: not entered\n");
        output.push_str(&format!("Host: {}\n", self.host.host));
        output.push_str(&format!("Host HVF available: {}\n", self.host.available));
        output.push_str(&format!("Allowed: {}\n", self.allowed));
        output.push_str(&format!("Attempted: {}\n", self.attempted));
        output.push_str(&format!("VM created: {}\n", self.vm_created));
        output.push_str(&format!(
            "Firmware memory allocated: {}\n",
            self.firmware_memory_allocated
        ));
        output.push_str(&format!(
            "Vars memory allocated: {}\n",
            self.vars_memory_allocated
        ));
        output.push_str(&format!(
            "Firmware memory populated: {}\n",
            self.firmware_memory_populated
        ));
        output.push_str(&format!(
            "Vars memory populated: {}\n",
            self.vars_memory_populated
        ));
        output.push_str(&format!(
            "Firmware memory mapped: {}\n",
            self.firmware_memory_mapped
        ));
        output.push_str(&format!(
            "Vars memory mapped: {}\n",
            self.vars_memory_mapped
        ));
        output.push_str(&format!(
            "Firmware memory unmapped: {}\n",
            self.firmware_memory_unmapped
        ));
        output.push_str(&format!(
            "Vars memory unmapped: {}\n",
            self.vars_memory_unmapped
        ));
        output.push_str(&format!(
            "Firmware memory deallocated: {}\n",
            self.firmware_memory_deallocated
        ));
        output.push_str(&format!(
            "Vars memory deallocated: {}\n",
            self.vars_memory_deallocated
        ));
        output.push_str(&format!("VM destroyed: {}\n", self.vm_destroyed));
        output.push_str(&format!(
            "Pflash map verified: {}\n",
            self.pflash_map_verified
        ));
        output.push_str(&format!(
            "Firmware slot IPA: {:#x}\n",
            self.firmware_slot_ipa
        ));
        output.push_str(&format!("Vars slot IPA: {:#x}\n", self.vars_slot_ipa));
        output.push_str(&format!("Slot bytes: {:#x}\n", self.slot_bytes));
        output.push_str(&format!(
            "Firmware source bytes: {}\n",
            render_optional_u64(self.firmware_source_bytes)
        ));
        output.push_str(&format!(
            "Vars source bytes: {}\n",
            render_optional_u64(self.vars_source_bytes)
        ));
        output.push_str(&format!(
            "Firmware map flags: {}\n",
            self.firmware_map_flags
        ));
        output.push_str(&format!("Vars map flags: {}\n", self.vars_map_flags));
        output.push_str(&format!(
            "VM create status: {}\n",
            render_optional_status(self.vm_create_status)
        ));
        output.push_str(&format!(
            "VM create status name: {}\n",
            render_optional_status_name(self.vm_create_status)
        ));
        output.push_str(&format!(
            "Firmware allocate status: {}\n",
            render_optional_status(self.firmware_allocate_status)
        ));
        output.push_str(&format!(
            "Firmware allocate status name: {}\n",
            render_optional_status_name(self.firmware_allocate_status)
        ));
        output.push_str(&format!(
            "Vars allocate status: {}\n",
            render_optional_status(self.vars_allocate_status)
        ));
        output.push_str(&format!(
            "Vars allocate status name: {}\n",
            render_optional_status_name(self.vars_allocate_status)
        ));
        output.push_str(&format!(
            "Firmware map status: {}\n",
            render_optional_status(self.firmware_map_status)
        ));
        output.push_str(&format!(
            "Firmware map status name: {}\n",
            render_optional_status_name(self.firmware_map_status)
        ));
        output.push_str(&format!(
            "Vars map status: {}\n",
            render_optional_status(self.vars_map_status)
        ));
        output.push_str(&format!(
            "Vars map status name: {}\n",
            render_optional_status_name(self.vars_map_status)
        ));
        output.push_str(&format!(
            "Firmware unmap status: {}\n",
            render_optional_status(self.firmware_unmap_status)
        ));
        output.push_str(&format!(
            "Firmware unmap status name: {}\n",
            render_optional_status_name(self.firmware_unmap_status)
        ));
        output.push_str(&format!(
            "Vars unmap status: {}\n",
            render_optional_status(self.vars_unmap_status)
        ));
        output.push_str(&format!(
            "Vars unmap status name: {}\n",
            render_optional_status_name(self.vars_unmap_status)
        ));
        output.push_str(&format!(
            "Firmware deallocate status: {}\n",
            render_optional_status(self.firmware_deallocate_status)
        ));
        output.push_str(&format!(
            "Firmware deallocate status name: {}\n",
            render_optional_status_name(self.firmware_deallocate_status)
        ));
        output.push_str(&format!(
            "Vars deallocate status: {}\n",
            render_optional_status(self.vars_deallocate_status)
        ));
        output.push_str(&format!(
            "Vars deallocate status name: {}\n",
            render_optional_status_name(self.vars_deallocate_status)
        ));
        output.push_str(&format!(
            "VM destroy status: {}\n",
            render_optional_status(self.vm_destroy_status)
        ));
        output.push_str(&format!(
            "VM destroy status name: {}\n",
            render_optional_status_name(self.vm_destroy_status)
        ));
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

    fn test_uefi_fv_bytes(len: usize) -> Vec<u8> {
        assert!(len >= UEFI_FV_MIN_HEADER_BYTES);
        let header_length = 0x48_u16;
        let mut bytes = vec![0_u8; len];
        bytes[16..32].copy_from_slice(&[
            0x8c, 0x8c, 0xf9, 0x61, 0xd2, 0x4b, 0x2c, 0x4f, 0x8a, 0x89, 0x22, 0x4d, 0xaf, 0xdc,
            0xf1, 0x6f,
        ]);
        bytes[UEFI_FV_LENGTH_OFFSET..UEFI_FV_LENGTH_OFFSET + 8]
            .copy_from_slice(&(len as u64).to_le_bytes());
        bytes[UEFI_FV_SIGNATURE_OFFSET..UEFI_FV_SIGNATURE_OFFSET + 4]
            .copy_from_slice(UEFI_FV_SIGNATURE);
        bytes[0x2c..0x30].copy_from_slice(&0x0004_feff_u32.to_le_bytes());
        bytes[UEFI_FV_HEADER_LENGTH_OFFSET..UEFI_FV_HEADER_LENGTH_OFFSET + 2]
            .copy_from_slice(&header_length.to_le_bytes());
        bytes[0x34..0x36].copy_from_slice(&0_u16.to_le_bytes());
        bytes[0x36] = 0;
        bytes[0x37] = 2;
        bytes[0x38..0x3c].copy_from_slice(&1_u32.to_le_bytes());
        bytes[0x3c..0x40].copy_from_slice(&(len as u32).to_le_bytes());
        bytes[0x40..0x44].copy_from_slice(&0_u32.to_le_bytes());
        bytes[0x44..0x48].copy_from_slice(&0_u32.to_le_bytes());
        let checksum = 0_u16.wrapping_sub(uefi_checksum16(&bytes[..usize::from(header_length)]));
        bytes[0x32..0x34].copy_from_slice(&checksum.to_le_bytes());
        bytes
    }

    #[test]
    fn windows_11_arm_uefi_firmware_device_discovery_probe_defaults_to_not_reached() {
        let stem = format!(
            "bridgevm-windows-arm-uefi-firmware-device-discovery-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let firmware_path = std::env::temp_dir().join(format!("{stem}-code.fd"));
        let template_path = std::env::temp_dir().join(format!("{stem}-vars-template.fd"));
        let vars_path = std::env::temp_dir().join(format!("{stem}-vars.fd"));
        let installer_iso_path = std::env::temp_dir().join(format!("{stem}-win11-arm.iso"));
        let writable_target_disk_path = std::env::temp_dir().join(format!("{stem}-windows.raw"));
        std::fs::write(&firmware_path, test_uefi_fv_bytes(128 * 1024)).unwrap();
        std::fs::write(&template_path, test_uefi_fv_bytes(64 * 1024)).unwrap();
        let _ = std::fs::remove_file(&vars_path);

        let probe = probe_windows_11_arm_uefi_firmware_device_discovery(
            WindowsArmUefiFirmwareRunLoopOptions {
                pflash: WindowsArmUefiPflashMapOptions {
                    firmware_path: firmware_path.clone(),
                    vars_template_path: Some(template_path.clone()),
                    vars_path: Some(vars_path.clone()),
                    create_vars: true,
                },
                execution: WindowsArmUefiFirmwareRunLoopExecutionOptions {
                    allow_loop: false,
                    requested_exits: 8,
                    guest_ram_mib: 64,
                    watchdog_timeout_ms: 100,
                    map_low_pflash_alias: false,
                    seed_diagnostic_vector: false,
                    seed_guest_ram_diagnostic_vector: false,
                    seed_executable_diagnostic_vector: false,
                    try_recommended_vector_base_vbar: false,
                    continue_after_recommended_vector_base_vbar: false,
                    repair_low_vector_diagnostic_page: false,
                    remap_low_vector_to_recommended_vector: false,
                    continue_after_low_vector_repair: false,
                    restore_low_vector_slot_before_eret: false,
                    wire_interrupt_timer: false,
                    stop_at_first_post_repair_device_boundary: false,
                    installer_iso_path: Some(installer_iso_path.clone()),
                    writable_target_disk_path: Some(writable_target_disk_path.clone()),
                },
            },
        );
        let output = probe.render_text();
        let _ = std::fs::remove_file(&firmware_path);
        let _ = std::fs::remove_file(&template_path);
        let _ = std::fs::remove_file(&vars_path);

        assert!(!probe.device_boundary_reached());
        assert_eq!(probe.boundary_status(), "not reached");
        assert!(!probe.device_discovery_ready());
        assert!(!probe.run_loop.allowed);
        assert!(!probe.run_loop.attempted);
        assert!(probe.run_loop.low_pflash_alias_requested);
        assert!(probe.run_loop.low_vector_diagnostic_page_repair_requested);
        assert!(probe.run_loop.low_vector_post_repair_continue_requested);
        assert!(probe.run_loop.interrupt_timer_wiring_requested);
        assert!(
            probe
                .run_loop
                .stop_at_first_post_repair_device_boundary_requested
        );
        assert_eq!(probe.run_loop.handled_mmio_read_count, 0);
        assert_eq!(probe.run_loop.handled_mmio_write_count, 0);
        assert_eq!(probe.run_loop.handled_icc_read_count, 0);
        assert_eq!(probe.run_loop.handled_icc_write_count, 0);
        assert!(output.contains("Windows 11 Arm HVF UEFI firmware device-discovery probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("Windows boot: not claimed"));
        assert!(output.contains("Underlying probe: windows-firmware-run-loop-probe"));
        assert!(output.contains("Device discovery boundary reached: false"));
        assert!(output.contains("Device discovery boundary status: not reached"));
        assert!(output.contains("Device discovery ready: false"));
        assert!(output.contains(
            "Device discovery blocker: firmware has not reached a non-diagnostic MMIO/sysreg boundary yet"
        ));
        assert!(output.contains("Handled MMIO access count: 0"));
        assert!(output.contains("Handled ICC access count: 0"));
        assert!(output.contains("Low pflash alias requested: true"));
        assert!(output.contains("Low vector diagnostic page repair requested: true"));
        assert!(output.contains("Continue after low-vector repair requested: true"));
        assert!(output.contains("Interrupt/timer wiring requested: true"));
        assert!(output.contains("Stop at first post-repair device boundary requested: true"));
        assert!(output.contains(&format!(
            "Installer ISO path: {}",
            installer_iso_path.display()
        )));
        assert!(output.contains(&format!(
            "Writable target disk path: {}",
            writable_target_disk_path.display()
        )));
        assert!(output.contains("BRIDGEVM_HVF_ALLOW_UEFI_FIRMWARE_RUN_LOOP"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn windows_11_arm_uefi_firmware_run_loop_no_live_loop_reports_restore_before_eret_request() {
        let stem = format!(
            "bridgevm-windows-arm-uefi-firmware-run-loop-restore-before-eret-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let firmware_path = std::env::temp_dir().join(format!("{stem}-code.fd"));
        let template_path = std::env::temp_dir().join(format!("{stem}-vars-template.fd"));
        let vars_path = std::env::temp_dir().join(format!("{stem}-vars.fd"));
        std::fs::write(&firmware_path, test_uefi_fv_bytes(128 * 1024)).unwrap();
        std::fs::write(&template_path, test_uefi_fv_bytes(64 * 1024)).unwrap();
        let _ = std::fs::remove_file(&vars_path);

        let probe =
            probe_windows_11_arm_uefi_firmware_run_loop(WindowsArmUefiFirmwareRunLoopOptions {
                pflash: WindowsArmUefiPflashMapOptions {
                    firmware_path: firmware_path.clone(),
                    vars_template_path: Some(template_path.clone()),
                    vars_path: Some(vars_path.clone()),
                    create_vars: true,
                },
                execution: WindowsArmUefiFirmwareRunLoopExecutionOptions {
                    allow_loop: false,
                    requested_exits: 8,
                    guest_ram_mib: 64,
                    watchdog_timeout_ms: 100,
                    map_low_pflash_alias: true,
                    seed_diagnostic_vector: false,
                    seed_guest_ram_diagnostic_vector: false,
                    seed_executable_diagnostic_vector: false,
                    try_recommended_vector_base_vbar: false,
                    continue_after_recommended_vector_base_vbar: false,
                    repair_low_vector_diagnostic_page: true,
                    remap_low_vector_to_recommended_vector: false,
                    continue_after_low_vector_repair: true,
                    restore_low_vector_slot_before_eret: true,
                    wire_interrupt_timer: false,
                    stop_at_first_post_repair_device_boundary: false,
                    installer_iso_path: None,
                    writable_target_disk_path: None,
                },
            });
        let output = probe.render_text();
        let _ = std::fs::remove_file(&firmware_path);
        let _ = std::fs::remove_file(&template_path);
        let _ = std::fs::remove_file(&vars_path);

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(probe.low_vector_diagnostic_page_repair_requested);
        assert!(probe.low_vector_post_repair_continue_requested);
        assert!(probe.low_vector_diagnostic_page_restore_before_eret_requested);
        assert!(!probe.low_vector_diagnostic_page_restore_before_eret_attempted);
        assert!(!probe.low_vector_diagnostic_page_slot_restored);
        assert!(output.contains("Low vector diagnostic page repair requested: true"));
        assert!(output.contains("Continue after low-vector repair requested: true"));
        assert!(output.contains("Low vector diagnostic page restore before ERET requested: true"));
        assert!(output.contains("Low vector diagnostic page restore before ERET attempted: false"));
        assert!(output.contains("Low vector diagnostic page slot restored: false"));
        assert!(output.contains("BRIDGEVM_HVF_ALLOW_UEFI_FIRMWARE_RUN_LOOP"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn windows_11_arm_uefi_firmware_run_loop_no_live_loop_reports_executable_vector_request() {
        let stem = format!(
            "bridgevm-windows-arm-uefi-firmware-run-loop-exec-vector-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let firmware_path = std::env::temp_dir().join(format!("{stem}-code.fd"));
        let template_path = std::env::temp_dir().join(format!("{stem}-vars-template.fd"));
        let vars_path = std::env::temp_dir().join(format!("{stem}-vars.fd"));
        std::fs::write(&firmware_path, test_uefi_fv_bytes(128 * 1024)).unwrap();
        std::fs::write(&template_path, test_uefi_fv_bytes(64 * 1024)).unwrap();
        let _ = std::fs::remove_file(&vars_path);

        let probe =
            probe_windows_11_arm_uefi_firmware_run_loop(WindowsArmUefiFirmwareRunLoopOptions {
                pflash: WindowsArmUefiPflashMapOptions {
                    firmware_path: firmware_path.clone(),
                    vars_template_path: Some(template_path.clone()),
                    vars_path: Some(vars_path.clone()),
                    create_vars: true,
                },
                execution: WindowsArmUefiFirmwareRunLoopExecutionOptions {
                    allow_loop: false,
                    requested_exits: 8,
                    guest_ram_mib: 64,
                    watchdog_timeout_ms: 100,
                    map_low_pflash_alias: true,
                    seed_diagnostic_vector: false,
                    seed_guest_ram_diagnostic_vector: false,
                    seed_executable_diagnostic_vector: true,
                    try_recommended_vector_base_vbar: false,
                    continue_after_recommended_vector_base_vbar: false,
                    repair_low_vector_diagnostic_page: false,
                    remap_low_vector_to_recommended_vector: false,
                    continue_after_low_vector_repair: false,
                    restore_low_vector_slot_before_eret: false,
                    wire_interrupt_timer: false,
                    stop_at_first_post_repair_device_boundary: false,
                    installer_iso_path: None,
                    writable_target_disk_path: None,
                },
            });
        let output = probe.render_text();
        let _ = std::fs::remove_file(&firmware_path);
        let _ = std::fs::remove_file(&template_path);
        let _ = std::fs::remove_file(&vars_path);

        assert!(!probe.allowed);
        assert!(!probe.attempted);
        assert!(probe.diagnostic_vector_seed_requested);
        assert!(!probe.diagnostic_vector_populated);
        assert!(probe.low_pflash_alias_requested);
        assert_eq!(
            probe.diagnostic_vector_ipa,
            WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA
        );
        assert_eq!(
            probe.diagnostic_vector_location,
            "low-pflash-executable-candidate"
        );
        assert!(probe.exits.is_empty());
        assert!(output.contains("Diagnostic vector seed requested: true"));
        assert!(output.contains("Diagnostic vector location: low-pflash-executable-candidate"));
        assert!(output.contains("Diagnostic vector IPA: 0x200000"));
        assert!(output.contains("Low pflash alias requested: true"));
        assert!(output.contains("Observed exits: 0"));
        assert!(output.contains("BRIDGEVM_HVF_ALLOW_UEFI_FIRMWARE_RUN_LOOP"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn windows_11_arm_uefi_firmware_run_loop_render_records_vtimer_auto_mask() {
        let stem = format!(
            "bridgevm-windows-arm-uefi-firmware-run-loop-vtimer-exit-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let firmware_path = std::env::temp_dir().join(format!("{stem}-code.fd"));
        let template_path = std::env::temp_dir().join(format!("{stem}-vars-template.fd"));
        let vars_path = std::env::temp_dir().join(format!("{stem}-vars.fd"));
        std::fs::write(&firmware_path, test_uefi_fv_bytes(128 * 1024)).unwrap();
        std::fs::write(&template_path, test_uefi_fv_bytes(64 * 1024)).unwrap();
        let _ = std::fs::remove_file(&vars_path);

        let mut probe =
            probe_windows_11_arm_uefi_firmware_run_loop(WindowsArmUefiFirmwareRunLoopOptions {
                pflash: WindowsArmUefiPflashMapOptions {
                    firmware_path: firmware_path.clone(),
                    vars_template_path: Some(template_path.clone()),
                    vars_path: Some(vars_path.clone()),
                    create_vars: true,
                },
                execution: WindowsArmUefiFirmwareRunLoopExecutionOptions {
                    allow_loop: false,
                    requested_exits: 8,
                    guest_ram_mib: 64,
                    watchdog_timeout_ms: 100,
                    map_low_pflash_alias: false,
                    seed_diagnostic_vector: false,
                    seed_guest_ram_diagnostic_vector: false,
                    seed_executable_diagnostic_vector: false,
                    try_recommended_vector_base_vbar: false,
                    continue_after_recommended_vector_base_vbar: false,
                    repair_low_vector_diagnostic_page: false,
                    remap_low_vector_to_recommended_vector: false,
                    continue_after_low_vector_repair: false,
                    restore_low_vector_slot_before_eret: false,
                    wire_interrupt_timer: true,
                    stop_at_first_post_repair_device_boundary: false,
                    installer_iso_path: None,
                    writable_target_disk_path: None,
                },
            });
        let _ = std::fs::remove_file(&firmware_path);
        let _ = std::fs::remove_file(&template_path);
        let _ = std::fs::remove_file(&vars_path);

        probe.vtimer_exit_count = 1;
        probe.pending_irq_injected_count = 1;
        probe.device_irq_injected_count = 1;
        probe.device_irq_cleared_count = 1;
        probe.last_device_irq_set_status = Some(0);
        probe.last_device_irq_clear_status = Some(0);
        probe.exits = vec![WindowsArmUefiFirmwareRunLoopExit {
            index: 1,
            run_status: Some(0),
            exit_reason: Some(2),
            exit_syndrome: None,
            exit_exception_class: None,
            exit_virtual_address: None,
            exit_physical_address: None,
            pc_after_exit_status: Some(0),
            pc_after_exit: Some(WINDOWS_ARM_UEFI_CODE_IPA),
            instruction_word_after_exit: None,
            instruction_hint_after_exit: "not observed",
            pc_stage1_leaf_level_after_exit: None,
            pc_stage1_leaf_descriptor_after_exit: None,
            pc_stage1_leaf_descriptor_kind_after_exit: "not observed",
            pc_stage1_leaf_pxn_after_exit: None,
            pc_stage1_leaf_uxn_after_exit: None,
            stage1_descriptor_samples_after_exit: Vec::new(),
            stage1_walk_entries_after_exit: Vec::new(),
            stage1_executable_candidates_after_exit: Vec::new(),
            x0_after_exit: None,
            x1_after_exit: None,
            x2_after_exit: None,
            x3_after_exit: None,
            x4_after_exit: None,
            cpsr_after_exit: None,
            vbar_el1_after_exit: None,
            elr_el1_after_exit: None,
            esr_el1_after_exit: None,
            far_el1_after_exit: None,
            spsr_el1_after_exit: None,
            sctlr_el1_after_exit: None,
            tcr_el1_after_exit: None,
            ttbr0_el1_after_exit: None,
            ttbr1_el1_after_exit: None,
            mair_el1_after_exit: None,
            sp_el1_after_exit: None,
            watchdog_cancel_status: None,
            vtimer_auto_mask_get_status: Some(0),
            vtimer_auto_mask_after_exit: Some(true),
            vtimer_rearm_cval_value: Some(0x1234),
            vtimer_rearm_cval_set_status: Some(0),
            vtimer_ppi_pending_recorded: Some(true),
            vtimer_irq_line_assertable: Some(true),
            vtimer_gic_group1_enabled: Some(true),
            vtimer_gic_priority_mask: Some(0xff),
            vtimer_gic_running_priority: Some(0xff),
            vtimer_gic_priority_threshold: Some(0xff),
            vtimer_gic_pending_intid: Some(WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID),
            vtimer_pending_irq_set_status: Some(0),
            vtimer_unmask_status: Some(0),
            handled: true,
        }];

        let output = probe.render_text();

        assert!(output.contains("VTimer exit count: 1"));
        assert!(output.contains("Pending IRQ injected count: 1"));
        assert!(output.contains("Device IRQ line asserted count: 1"));
        assert!(output.contains("Device IRQ line deasserted count: 1"));
        assert!(output.contains("Handled MMIO read count: 0"));
        assert!(output.contains("Handled MMIO write count: 0"));
        assert!(output.contains("VirtIO queue_notify count: 0"));
        assert!(output.contains("VirtIO request completion count: 0"));
        assert!(output.contains("Handled ICC read count: 0"));
        assert!(output.contains("Handled ICC write count: 0"));
        assert!(output.contains("Last device IRQ line assert status name: HV_SUCCESS"));
        assert!(output.contains("Last device IRQ line deassert status name: HV_SUCCESS"));
        assert!(output.contains("CNTV_CVAL_EL0 value: 0x0"));
        assert!(output.contains("CNTV_CTL_EL0 value: 0x1"));
        assert!(output.contains("reason=HV_EXIT_REASON_VTIMER_ACTIVATED"));
        assert!(output.contains("vtimer_auto_mask=true"));
        assert!(output.contains("vtimer_auto_mask_get=HV_SUCCESS"));
        assert!(output.contains("vtimer_rearm_cval=0x1234"));
        assert!(output.contains("vtimer_rearm_cval_set=HV_SUCCESS"));
        assert!(output.contains("vtimer_ppi_pending_recorded=true"));
        assert!(output.contains("vtimer_irq_line_assertable=true"));
        assert!(output.contains("vtimer_gic_group1_enabled=true"));
        assert!(output.contains("vtimer_gic_priority_mask=0xff"));
        assert!(output.contains("vtimer_gic_running_priority=0xff"));
        assert!(output.contains("vtimer_gic_priority_threshold=0xff"));
        assert!(output.contains("vtimer_gic_pending_intid=27"));
        assert!(output.contains("vtimer_pending_irq=HV_SUCCESS"));
        assert!(output.contains("vtimer_unmask=HV_SUCCESS"));
        assert!(output.contains("handled=true"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }
}
