//! Windows-on-Arm machine description, firmware handoff and boot-media
//! tooling: layout constants, the UEFI handoff/pflash/reset-vector/run-loop
//! probes, GPT boot-disk planning, the FDT blob builder/parser, the platform
//! description probe, and the firmware run-loop diagnosis/decoder helpers.
//!
//! Moved verbatim out of the legacy probe monolith. Each item keeps the
//! visibility it had at the crate root, and the root glob re-exports this
//! module, so the public API is unchanged.

use crate::*;

pub const WINDOWS_ARM_BOOT_DISK_DEFAULT_SIZE_GIB: u32 = 64;
pub const WINDOWS_ARM_BOOT_DISK_MIN_SIZE_GIB: u32 = 8;
pub const WINDOWS_ARM_UEFI_LOW_CODE_ALIAS_IPA: u64 = 0x0000_0000;
pub const WINDOWS_ARM_UEFI_LOW_VARS_ALIAS_IPA: u64 = 0x0400_0000;
pub const WINDOWS_ARM_UEFI_CODE_IPA: u64 = 0x0800_0000;
pub const WINDOWS_ARM_UEFI_VARS_IPA: u64 = 0x0c00_0000;
pub const WINDOWS_ARM_UEFI_SLOT_BYTES: u64 = 64 * 1024 * 1024;
pub const WINDOWS_ARM_UEFI_PFLASH_BYTES: u64 = WINDOWS_ARM_UEFI_SLOT_BYTES * 2;
pub const WINDOWS_ARM_DEVICE_MMIO_IPA: u64 = 0x1000_0000;
pub const WINDOWS_ARM_DEVICE_MMIO_BYTES: u64 = 0x1000_0000;
pub(crate) const WINDOWS_ARM_PL011_MMIO_IPA: u64 = WINDOWS_ARM_DEVICE_MMIO_IPA;
pub(crate) const WINDOWS_ARM_PL031_MMIO_IPA: u64 = WINDOWS_ARM_DEVICE_MMIO_IPA + 0x1000;
pub(crate) const WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA: u64 =
    WINDOWS_ARM_DEVICE_MMIO_IPA + 0x2000;
pub(crate) const WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA: u64 =
    WINDOWS_ARM_DEVICE_MMIO_IPA + 0x3000;
pub(crate) const WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA: u64 = WINDOWS_ARM_DEVICE_MMIO_IPA + 0x1_0000;
pub(crate) const WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA: u64 =
    WINDOWS_ARM_DEVICE_MMIO_IPA + 0x2_0000;
pub(crate) const WINDOWS_ARM_GIC_DISTRIBUTOR_BYTES: u64 = 0x1_0000;
pub(crate) const WINDOWS_ARM_GIC_REDISTRIBUTOR_BYTES: u64 = 0x2_0000;
pub(crate) const WINDOWS_ARM_GIC_PHANDLE: u32 = 1;
pub(crate) const GIC_SPI: u32 = 0;
pub(crate) const GIC_PPI: u32 = 1;
pub(crate) const IRQ_TYPE_LEVEL_HIGH: u32 = 4;
// FDT GIC SPI interrupt numbers are encoded as the global IRQ minus 32.
pub(crate) const WINDOWS_ARM_PL011_SPI: u32 = 0;
pub(crate) const WINDOWS_ARM_PL031_SPI: u32 = 1;
pub(crate) const WINDOWS_ARM_VIRTIO_INSTALLER_ISO_SPI: u32 = 2;
pub(crate) const WINDOWS_ARM_VIRTIO_TARGET_DISK_SPI: u32 = 3;
pub(crate) const WINDOWS_ARM_PL011_FLAG_VALUE: u64 = 0x90;
pub(crate) const WINDOWS_ARM_PL031_READ_VALUE: u64 = 0x2026_0618;
pub const WINDOWS_ARM_GUEST_RAM_IPA: u64 = 0x4000_0000;
pub const WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA: u64 = 0x0020_0000;
pub const WINDOWS_ARM_DIAGNOSTIC_VECTOR_IPA: u64 = WINDOWS_ARM_UEFI_CODE_IPA;
pub const WINDOWS_ARM_GUEST_RAM_DIAGNOSTIC_VECTOR_IPA: u64 = WINDOWS_ARM_GUEST_RAM_IPA;
pub const WINDOWS_ARM_DIAGNOSTIC_VECTOR_BYTES: u64 = 0x800;
pub(crate) const WINDOWS_ARM_PLATFORM_DTB_GUEST_RAM_OFFSET: u64 = 0x0001_0000;
pub(crate) const WINDOWS_ARM_PLATFORM_DTB_IPA: u64 =
    WINDOWS_ARM_GUEST_RAM_IPA + WINDOWS_ARM_PLATFORM_DTB_GUEST_RAM_OFFSET;
pub(crate) const WINDOWS_ARM_FIRMWARE_RUN_LOOP_FDT_VCPU_COUNT: u8 = 1;
pub(crate) const WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET: usize = 0x200;
pub(crate) const AARCH64_STAGE1_PAGE_OUTPUT_ADDRESS_MASK: u64 = 0x0000_ffff_ffff_f000;
pub(crate) const WINDOWS_ARM_LOW_VECTOR_DIAGNOSTIC_PAGE_DESCRIPTOR: u64 = 0xf8f;
pub(crate) const AARCH64_HVC_0_INSTRUCTION: u32 = 0xd400_0002;
pub(crate) const AARCH64_HVC_1_INSTRUCTION: u32 = 0xd400_0022;
pub(crate) const AARCH64_ERET_INSTRUCTION: u32 = 0xd69f_03e0;
pub(crate) const WINDOWS_ARM_PLATFORM_DESCRIPTION_DEFAULT_GUEST_RAM_BYTES: u64 =
    6 * 1024 * 1024 * 1024;
pub(crate) const WINDOWS_ARM_PLATFORM_DESCRIPTION_DEFAULT_VCPU_COUNT: u8 = 4;
pub(crate) const FDT_MAGIC: u32 = 0xd00d_feed;
pub(crate) const FDT_BEGIN_NODE: u32 = 1;
pub(crate) const FDT_END_NODE: u32 = 2;
pub(crate) const FDT_PROP: u32 = 3;
pub(crate) const FDT_END: u32 = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WindowsArmDiagnosticVectorSelection {
    pub(crate) requested: bool,
    pub(crate) location: &'static str,
    pub(crate) ipa: u64,
}

pub(crate) fn windows_arm_diagnostic_vector_selection(
    seed_diagnostic_vector: bool,
    seed_guest_ram_diagnostic_vector: bool,
    seed_executable_diagnostic_vector: bool,
) -> WindowsArmDiagnosticVectorSelection {
    let requested = seed_diagnostic_vector
        || seed_guest_ram_diagnostic_vector
        || seed_executable_diagnostic_vector;
    if seed_executable_diagnostic_vector {
        return WindowsArmDiagnosticVectorSelection {
            requested,
            location: "low-pflash-executable-candidate",
            ipa: WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA,
        };
    }
    if seed_guest_ram_diagnostic_vector {
        return WindowsArmDiagnosticVectorSelection {
            requested,
            location: "guest-ram",
            ipa: WINDOWS_ARM_GUEST_RAM_DIAGNOSTIC_VECTOR_IPA,
        };
    }
    WindowsArmDiagnosticVectorSelection {
        requested,
        location: "pflash",
        ipa: WINDOWS_ARM_DIAGNOSTIC_VECTOR_IPA,
    }
}

pub(crate) fn windows_arm_vector_slot_instruction_is_populated(word: Option<u32>) -> bool {
    !matches!(word, None | Some(0) | Some(0xffff_ffff))
}

pub(crate) fn windows_arm_instruction_is_bridgevm_diagnostic_vector_word(word: u32) -> bool {
    matches!(
        word,
        AARCH64_HVC_0_INSTRUCTION | AARCH64_HVC_1_INSTRUCTION | AARCH64_ERET_INSTRUCTION
    )
}

pub(crate) fn windows_arm_vector_slot_instruction_is_non_diagnostic_populated(
    word: Option<u32>,
) -> bool {
    match word {
        Some(word) => {
            windows_arm_vector_slot_instruction_is_populated(Some(word))
                && !windows_arm_instruction_is_bridgevm_diagnostic_vector_word(word)
        }
        None => false,
    }
}

pub(crate) fn windows_arm_gic_redistributor_fdt_bytes(vcpu_count: u8) -> u64 {
    WINDOWS_ARM_GIC_REDISTRIBUTOR_BYTES * u64::from(vcpu_count.max(1))
}

pub(crate) const GPT_SECTOR_BYTES: u64 = 512;
pub(crate) const GPT_SECTOR_BYTES_USIZE: usize = GPT_SECTOR_BYTES as usize;
pub(crate) const GPT_ENTRY_COUNT: usize = 128;
pub(crate) const GPT_ENTRY_SIZE: usize = 128;
pub(crate) const GPT_ENTRY_ARRAY_BYTES: usize = GPT_ENTRY_COUNT * GPT_ENTRY_SIZE;
pub(crate) const GPT_ENTRY_ARRAY_SECTORS: u64 = (GPT_ENTRY_ARRAY_BYTES as u64) / GPT_SECTOR_BYTES;
pub(crate) const GPT_PRIMARY_HEADER_LBA: u64 = 1;
pub(crate) const GPT_PRIMARY_ENTRY_LBA: u64 = 2;
pub(crate) const GPT_FIRST_USABLE_LBA: u64 = GPT_PRIMARY_ENTRY_LBA + GPT_ENTRY_ARRAY_SECTORS;
pub(crate) const WINDOWS_ARM_BOOT_DISK_ALIGNMENT_LBA: u64 = 2048;
pub(crate) const WINDOWS_ARM_ESP_SIZE_BYTES: u64 = 260 * 1024 * 1024;
pub(crate) const WINDOWS_ARM_MSR_SIZE_BYTES: u64 = 16 * 1024 * 1024;
pub(crate) const EFI_SYSTEM_PARTITION_GUID: [u8; 16] = [
    0x28, 0x73, 0x2a, 0xc1, 0x1f, 0xf8, 0xd2, 0x11, 0xba, 0x4b, 0x00, 0xa0, 0xc9, 0x3e, 0xc9, 0x3b,
];
pub(crate) const MICROSOFT_RESERVED_PARTITION_GUID: [u8; 16] = [
    0x16, 0xe3, 0xc9, 0xe3, 0x5c, 0x0b, 0xb8, 0x4d, 0x81, 0x7d, 0xf9, 0x2d, 0xf0, 0x02, 0x15, 0xae,
];
pub(crate) const MICROSOFT_BASIC_DATA_PARTITION_GUID: [u8; 16] = [
    0xa2, 0xa0, 0xd0, 0xeb, 0xe5, 0xb9, 0x33, 0x44, 0x87, 0xc0, 0x68, 0xb6, 0xb7, 0x26, 0x99, 0xc7,
];
pub(crate) const UEFI_FV_SIGNATURE_OFFSET: usize = 0x28;
pub(crate) const UEFI_FV_LENGTH_OFFSET: usize = 0x20;
pub(crate) const UEFI_FV_HEADER_LENGTH_OFFSET: usize = 0x30;
pub(crate) const UEFI_FV_MIN_HEADER_BYTES: usize = 0x38;
pub(crate) const UEFI_FV_SIGNATURE: &[u8; 4] = b"_FVH";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmPlatformDescriptionOptions {
    pub guest_ram_bytes: u64,
    pub vcpu_count: u8,
}

impl Default for WindowsArmPlatformDescriptionOptions {
    fn default() -> Self {
        Self {
            guest_ram_bytes: WINDOWS_ARM_PLATFORM_DESCRIPTION_DEFAULT_GUEST_RAM_BYTES,
            vcpu_count: WINDOWS_ARM_PLATFORM_DESCRIPTION_DEFAULT_VCPU_COUNT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmFdtMmioNodeCheck {
    pub label: &'static str,
    pub node_name: &'static str,
    pub base_ipa: Option<u64>,
    pub bytes: Option<u64>,
    pub inside_device_window: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmFdtInterruptCheck {
    pub label: &'static str,
    pub node_name: &'static str,
    pub interrupt_type: Option<u32>,
    pub interrupt_number: Option<u32>,
    pub trigger: Option<u32>,
    pub described: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmVirtioBlockDeviceMetadata {
    pub role: &'static str,
    pub label: &'static str,
    pub node_name: &'static str,
    pub base_ipa: u64,
    pub bytes: u64,
    pub read_only: bool,
    pub backing_kind: &'static str,
    pub backing_path: Option<PathBuf>,
    pub device_features: u64,
    pub capacity_sectors: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmPlatformDescriptionProbe {
    pub qemu_used: bool,
    pub apple_vz_used: bool,
    pub hvf_entered: bool,
    pub format: &'static str,
    pub fdt_blob: Vec<u8>,
    pub fdt_blob_bytes: usize,
    pub fdt_magic: u32,
    pub fdt_magic_verified: bool,
    pub memory_node_base_ipa: Option<u64>,
    pub memory_node_at_guest_ram_base: bool,
    pub requested_cpu_count: u8,
    pub cpu_count: u8,
    pub cpu_count_verified: bool,
    pub device_mmio_start_ipa: u64,
    pub device_mmio_end_ipa: u64,
    pub mmio_nodes: Vec<WindowsArmFdtMmioNodeCheck>,
    pub mmio_nodes_inside_device_window: bool,
    pub root_interrupt_parent: Option<u32>,
    pub gic_phandle: Option<u32>,
    pub gic_distributor_base_ipa: Option<u64>,
    pub gic_distributor_bytes: Option<u64>,
    pub gic_redistributor_base_ipa: Option<u64>,
    pub gic_redistributor_bytes: Option<u64>,
    pub gic_nodes_inside_device_window: bool,
    pub arch_timer_node_present: bool,
    pub arch_timer_interrupt_count: usize,
    pub interrupt_nodes: Vec<WindowsArmFdtInterruptCheck>,
    pub interrupt_nodes_described: bool,
    pub acpi_implemented: bool,
    pub fw_cfg_used: bool,
    pub gic_status: &'static str,
    pub gic_emulated: bool,
    pub blockers: Vec<String>,
}

impl WindowsArmPlatformDescriptionProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("Windows 11 Arm HVF platform description probe\n");
        output.push_str(if self.qemu_used {
            "QEMU: used\n"
        } else {
            "QEMU: not used\n"
        });
        output.push_str(if self.apple_vz_used {
            "Apple VZ: used\n"
        } else {
            "Apple VZ: not used\n"
        });
        output.push_str(if self.hvf_entered {
            "HVF: entered\n"
        } else {
            "HVF: not entered\n"
        });
        output.push_str("Guest execution: not entered; metadata-only FDT platform description\n");
        output.push_str(&format!("Format: {}\n", self.format));
        output.push_str(&format!("FDT blob bytes: {:#x}\n", self.fdt_blob_bytes));
        output.push_str(&format!("FDT magic: {:#x}\n", self.fdt_magic));
        output.push_str(&format!(
            "FDT magic verified: {}\n",
            self.fdt_magic_verified
        ));
        output.push_str(&format!(
            "Memory node base: {}\n",
            render_optional_u64(self.memory_node_base_ipa)
        ));
        output.push_str(&format!(
            "Memory node at 0x40000000: {}\n",
            self.memory_node_at_guest_ram_base
        ));
        output.push_str(&format!(
            "Requested CPU count: {}\n",
            self.requested_cpu_count
        ));
        output.push_str(&format!("CPU count: {}\n", self.cpu_count));
        output.push_str(&format!(
            "CPU count verified: {}\n",
            self.cpu_count_verified
        ));
        output.push_str(&format!(
            "Device MMIO window: {:#x}..{:#x}\n",
            self.device_mmio_start_ipa, self.device_mmio_end_ipa
        ));
        output.push_str(&format!(
            "PL011/PL031/VirtIO-MMIO installer ISO/target disk nodes inside device window: {}\n",
            self.mmio_nodes_inside_device_window
        ));
        for node in &self.mmio_nodes {
            output.push_str(&format!("{} node: {}\n", node.label, node.node_name));
            output.push_str(&format!(
                "{} node base: {}\n",
                node.label,
                render_optional_u64(node.base_ipa)
            ));
            output.push_str(&format!(
                "{} node bytes: {}\n",
                node.label,
                render_optional_u64(node.bytes)
            ));
            output.push_str(&format!(
                "{} node inside device window: {}\n",
                node.label, node.inside_device_window
            ));
        }
        output.push_str(&format!(
            "Root interrupt-parent: {}\n",
            render_optional_u64(self.root_interrupt_parent.map(u64::from))
        ));
        output.push_str(&format!(
            "GIC phandle: {}\n",
            render_optional_u64(self.gic_phandle.map(u64::from))
        ));
        output.push_str(&format!(
            "GIC distributor base: {}\n",
            render_optional_u64(self.gic_distributor_base_ipa)
        ));
        output.push_str(&format!(
            "GIC distributor bytes: {}\n",
            render_optional_u64(self.gic_distributor_bytes)
        ));
        output.push_str(&format!(
            "GIC redistributor base: {}\n",
            render_optional_u64(self.gic_redistributor_base_ipa)
        ));
        output.push_str(&format!(
            "GIC redistributor bytes: {}\n",
            render_optional_u64(self.gic_redistributor_bytes)
        ));
        output.push_str(&format!(
            "GIC nodes inside device window: {}\n",
            self.gic_nodes_inside_device_window
        ));
        output.push_str(&format!(
            "ARM arch timer node present: {}\n",
            self.arch_timer_node_present
        ));
        output.push_str(&format!(
            "ARM arch timer interrupt count: {}\n",
            self.arch_timer_interrupt_count
        ));
        output.push_str(&format!(
            "Interrupt nodes described: {}\n",
            self.interrupt_nodes_described
        ));
        for interrupt in &self.interrupt_nodes {
            output.push_str(&format!(
                "{} interrupt node: {}\n",
                interrupt.label, interrupt.node_name
            ));
            output.push_str(&format!(
                "{} interrupt type: {}\n",
                interrupt.label,
                render_optional_u64(interrupt.interrupt_type.map(u64::from))
            ));
            output.push_str(&format!(
                "{} interrupt number: {}\n",
                interrupt.label,
                render_optional_u64(interrupt.interrupt_number.map(u64::from))
            ));
            output.push_str(&format!(
                "{} interrupt trigger: {}\n",
                interrupt.label,
                render_optional_u64(interrupt.trigger.map(u64::from))
            ));
            output.push_str(&format!(
                "{} interrupt described: {}\n",
                interrupt.label, interrupt.described
            ));
        }
        output.push_str(if self.acpi_implemented {
            "ACPI: implemented\n"
        } else {
            "ACPI: not implemented\n"
        });
        output.push_str(if self.fw_cfg_used {
            "fw_cfg: used\n"
        } else {
            "fw_cfg: not used\n"
        });
        output.push_str(&format!("GIC: {}\n", self.gic_status));
        output.push_str(&format!("GIC emulated: {}\n", self.gic_emulated));
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiResetVectorEntryProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub firmware_memory_allocated: bool,
    pub vars_memory_allocated: bool,
    pub firmware_memory_populated: bool,
    pub vars_memory_populated: bool,
    pub firmware_memory_mapped: bool,
    pub vars_memory_mapped: bool,
    pub vcpu_created: bool,
    pub pc_set: bool,
    pub cpsr_set: bool,
    pub run_attempted: bool,
    pub reset_vector_entry_observed: bool,
    pub firmware_progress_observed: bool,
    pub watchdog_cancel_fired: bool,
    pub vcpu_destroyed: bool,
    pub firmware_memory_unmapped: bool,
    pub vars_memory_unmapped: bool,
    pub firmware_memory_deallocated: bool,
    pub vars_memory_deallocated: bool,
    pub vm_destroyed: bool,
    pub host: HvfHostCapabilities,
    pub pflash_map_verified: bool,
    pub reset_vector_ipa: u64,
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
    pub vcpu_create_status: Option<i32>,
    pub pc_set_status: Option<i32>,
    pub cpsr_set_status: Option<i32>,
    pub run_status: Option<i32>,
    pub exit_reason: Option<u32>,
    pub exit_syndrome: Option<u64>,
    pub exit_exception_class: Option<u64>,
    pub exit_virtual_address: Option<u64>,
    pub exit_physical_address: Option<u64>,
    pub pc_after_run_status: Option<i32>,
    pub pc_after_run: Option<u64>,
    pub watchdog_cancel_status: Option<i32>,
    pub vcpu_destroy_status: Option<i32>,
    pub firmware_unmap_status: Option<i32>,
    pub vars_unmap_status: Option<i32>,
    pub firmware_deallocate_status: Option<i32>,
    pub vars_deallocate_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub blockers: Vec<String>,
}

impl WindowsArmUefiResetVectorEntryProbe {
    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("Windows 11 Arm HVF UEFI reset-vector entry probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: UEFI reset vector entered under watchdog\n");
        output.push_str("Windows boot: not claimed\n");
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
        output.push_str(&format!("vCPU created: {}\n", self.vcpu_created));
        output.push_str(&format!("PC set: {}\n", self.pc_set));
        output.push_str(&format!("CPSR set: {}\n", self.cpsr_set));
        output.push_str(&format!("Run attempted: {}\n", self.run_attempted));
        output.push_str(&format!(
            "Reset-vector entry observed: {}\n",
            self.reset_vector_entry_observed
        ));
        output.push_str(&format!(
            "Firmware progress observed: {}\n",
            self.firmware_progress_observed
        ));
        output.push_str(&format!(
            "Watchdog cancel fired: {}\n",
            self.watchdog_cancel_fired
        ));
        output.push_str(&format!("vCPU destroyed: {}\n", self.vcpu_destroyed));
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
        output.push_str(&format!("Reset vector IPA: {:#x}\n", self.reset_vector_ipa));
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
            "VM create status name: {}\n",
            render_optional_status_name(self.vm_create_status)
        ));
        output.push_str(&format!(
            "Firmware allocate status name: {}\n",
            render_optional_status_name(self.firmware_allocate_status)
        ));
        output.push_str(&format!(
            "Vars allocate status name: {}\n",
            render_optional_status_name(self.vars_allocate_status)
        ));
        output.push_str(&format!(
            "Firmware map status name: {}\n",
            render_optional_status_name(self.firmware_map_status)
        ));
        output.push_str(&format!(
            "Vars map status name: {}\n",
            render_optional_status_name(self.vars_map_status)
        ));
        output.push_str(&format!(
            "vCPU create status name: {}\n",
            render_optional_status_name(self.vcpu_create_status)
        ));
        output.push_str(&format!(
            "PC set status name: {}\n",
            render_optional_status_name(self.pc_set_status)
        ));
        output.push_str(&format!(
            "CPSR set status name: {}\n",
            render_optional_status_name(self.cpsr_set_status)
        ));
        output.push_str(&format!(
            "Run status: {}\n",
            render_optional_status(self.run_status)
        ));
        output.push_str(&format!(
            "Run status name: {}\n",
            render_optional_status_name(self.run_status)
        ));
        output.push_str(&format!(
            "Exit reason: {}\n",
            render_optional_exit_reason(self.exit_reason)
        ));
        output.push_str(&format!(
            "Exit reason name: {}\n",
            render_optional_exit_reason_name(self.exit_reason)
        ));
        output.push_str(&format!(
            "Exit syndrome: {}\n",
            render_optional_u64(self.exit_syndrome)
        ));
        output.push_str(&format!(
            "Exit exception class: {}\n",
            render_optional_u64(self.exit_exception_class)
        ));
        output.push_str(&format!(
            "Exit exception class name: {}\n",
            render_optional_exception_class_name(self.exit_exception_class)
        ));
        output.push_str(&format!(
            "Exit virtual address: {}\n",
            render_optional_u64(self.exit_virtual_address)
        ));
        output.push_str(&format!(
            "Exit physical address: {}\n",
            render_optional_u64(self.exit_physical_address)
        ));
        output.push_str(&format!(
            "PC after run status name: {}\n",
            render_optional_status_name(self.pc_after_run_status)
        ));
        output.push_str(&format!(
            "PC after run: {}\n",
            render_optional_u64(self.pc_after_run)
        ));
        output.push_str(&format!(
            "Watchdog cancel status name: {}\n",
            render_optional_status_name(self.watchdog_cancel_status)
        ));
        output.push_str(&format!(
            "vCPU destroy status name: {}\n",
            render_optional_status_name(self.vcpu_destroy_status)
        ));
        output.push_str(&format!(
            "Firmware unmap status name: {}\n",
            render_optional_status_name(self.firmware_unmap_status)
        ));
        output.push_str(&format!(
            "Vars unmap status name: {}\n",
            render_optional_status_name(self.vars_unmap_status)
        ));
        output.push_str(&format!(
            "Firmware deallocate status name: {}\n",
            render_optional_status_name(self.firmware_deallocate_status)
        ));
        output.push_str(&format!(
            "Vars deallocate status name: {}\n",
            render_optional_status_name(self.vars_deallocate_status)
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiFirmwareRunLoopExit {
    pub index: u32,
    pub run_status: Option<i32>,
    pub exit_reason: Option<u32>,
    pub exit_syndrome: Option<u64>,
    pub exit_exception_class: Option<u64>,
    pub exit_virtual_address: Option<u64>,
    pub exit_physical_address: Option<u64>,
    pub pc_after_exit_status: Option<i32>,
    pub pc_after_exit: Option<u64>,
    pub instruction_word_after_exit: Option<u32>,
    pub instruction_hint_after_exit: &'static str,
    pub pc_stage1_leaf_level_after_exit: Option<u8>,
    pub pc_stage1_leaf_descriptor_after_exit: Option<u64>,
    pub pc_stage1_leaf_descriptor_kind_after_exit: &'static str,
    pub pc_stage1_leaf_pxn_after_exit: Option<bool>,
    pub pc_stage1_leaf_uxn_after_exit: Option<bool>,
    pub stage1_descriptor_samples_after_exit: Vec<WindowsArmUefiStage1DescriptorSample>,
    pub stage1_walk_entries_after_exit: Vec<WindowsArmUefiStage1WalkEntry>,
    pub stage1_executable_candidates_after_exit: Vec<WindowsArmUefiStage1ExecutableCandidate>,
    pub x0_after_exit: Option<u64>,
    pub x1_after_exit: Option<u64>,
    pub x2_after_exit: Option<u64>,
    pub x3_after_exit: Option<u64>,
    pub x4_after_exit: Option<u64>,
    pub cpsr_after_exit: Option<u64>,
    pub vbar_el1_after_exit: Option<u64>,
    pub elr_el1_after_exit: Option<u64>,
    pub esr_el1_after_exit: Option<u64>,
    pub far_el1_after_exit: Option<u64>,
    pub spsr_el1_after_exit: Option<u64>,
    pub sctlr_el1_after_exit: Option<u64>,
    pub tcr_el1_after_exit: Option<u64>,
    pub ttbr0_el1_after_exit: Option<u64>,
    pub ttbr1_el1_after_exit: Option<u64>,
    pub mair_el1_after_exit: Option<u64>,
    pub sp_el1_after_exit: Option<u64>,
    pub watchdog_cancel_status: Option<i32>,
    pub vtimer_auto_mask_get_status: Option<i32>,
    pub vtimer_auto_mask_after_exit: Option<bool>,
    pub vtimer_rearm_cval_value: Option<u64>,
    pub vtimer_rearm_cval_set_status: Option<i32>,
    pub vtimer_ppi_pending_recorded: Option<bool>,
    pub vtimer_irq_line_assertable: Option<bool>,
    pub vtimer_gic_group1_enabled: Option<bool>,
    pub vtimer_gic_priority_mask: Option<u8>,
    pub vtimer_gic_running_priority: Option<u8>,
    pub vtimer_gic_priority_threshold: Option<u8>,
    pub vtimer_gic_pending_intid: Option<u32>,
    pub vtimer_pending_irq_set_status: Option<i32>,
    pub vtimer_unmask_status: Option<i32>,
    pub handled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiStage1DescriptorSample {
    pub label: &'static str,
    pub virtual_address: u64,
    pub region: &'static str,
    pub level: Option<u8>,
    pub descriptor: Option<u64>,
    pub descriptor_kind: &'static str,
    pub output_address: Option<u64>,
    pub attr_index: Option<u8>,
    pub access_permissions: Option<u8>,
    pub shareability: Option<u8>,
    pub access_flag: Option<bool>,
    pub pxn: Option<bool>,
    pub uxn: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiStage1WalkEntry {
    pub label: &'static str,
    pub virtual_address: u64,
    pub region: &'static str,
    pub level: u8,
    pub table_ipa: u64,
    pub index: u64,
    pub entry_ipa: u64,
    pub descriptor: Option<u64>,
    pub descriptor_kind: &'static str,
    pub next_table_ipa: Option<u64>,
    pub output_address: Option<u64>,
    pub attr_index: Option<u8>,
    pub access_permissions: Option<u8>,
    pub shareability: Option<u8>,
    pub access_flag: Option<bool>,
    pub pxn: Option<bool>,
    pub uxn: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiStage1ExecutableCandidate {
    pub virtual_address: u64,
    pub region: &'static str,
    pub level: u8,
    pub descriptor: u64,
    pub descriptor_kind: &'static str,
    pub output_address: Option<u64>,
    pub span_bytes: Option<u64>,
    pub vector_sync_virtual_address: Option<u64>,
    pub vector_sync_physical_address: Option<u64>,
    pub vector_sync_instruction_word: Option<u32>,
    pub vector_sync_instruction_hint: &'static str,
    pub vector_base_scan_scanned_count: u32,
    pub vector_base_scan_suppressed_count: u32,
    pub vector_base_scan_limit_reached: bool,
    pub recommended_vector_base_candidate: Option<WindowsArmUefiVectorBaseRecommendation>,
    pub vector_base_candidates: Vec<WindowsArmUefiVectorBaseCandidate>,
    pub attr_index: u8,
    pub access_permissions: u8,
    pub shareability: u8,
    pub access_flag: bool,
    pub pxn: bool,
    pub uxn: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiVectorBaseCandidate {
    pub base_virtual_address: u64,
    pub base_physical_address: Option<u64>,
    pub current_el_sp0_sync_instruction_word: Option<u32>,
    pub current_el_spx_sync_instruction_word: Option<u32>,
    pub lower_aarch64_sync_instruction_word: Option<u32>,
    pub lower_aarch32_sync_instruction_word: Option<u32>,
    pub current_el_spx_sync_instruction_hint: &'static str,
    pub populated_slot_count: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiVectorBaseRecommendation {
    pub base_virtual_address: u64,
    pub base_physical_address: Option<u64>,
    pub current_el_spx_sync_instruction_word: Option<u32>,
    pub current_el_spx_sync_instruction_hint: &'static str,
    pub reason: &'static str,
}

impl WindowsArmUefiVectorBaseRecommendation {
    pub(crate) fn is_populated_low_vector_remap_target(&self) -> bool {
        self.base_physical_address.is_some()
            && windows_arm_vector_slot_instruction_is_non_diagnostic_populated(
                self.current_el_spx_sync_instruction_word,
            )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WindowsArmUefiVectorBaseCandidateScan {
    pub(crate) scanned_count: u32,
    pub(crate) suppressed_count: u32,
    pub(crate) limit_reached: bool,
    pub(crate) candidates: Vec<WindowsArmUefiVectorBaseCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsArmUefiFirmwareRunLoopProbe {
    pub allowed: bool,
    pub attempted: bool,
    pub vm_created: bool,
    pub firmware_memory_allocated: bool,
    pub vars_memory_allocated: bool,
    pub guest_ram_memory_allocated: bool,
    pub firmware_memory_populated: bool,
    pub vars_memory_populated: bool,
    pub firmware_memory_mapped: bool,
    pub vars_memory_mapped: bool,
    pub low_firmware_alias_mapped: bool,
    pub low_vars_alias_mapped: bool,
    pub guest_ram_memory_mapped: bool,
    pub platform_dtb_populated: bool,
    pub diagnostic_vector_seed_requested: bool,
    pub diagnostic_vector_populated: bool,
    pub low_vector_diagnostic_page_repair_requested: bool,
    pub low_vector_diagnostic_page_repaired: bool,
    pub low_vector_diagnostic_page_slot_restored: bool,
    pub low_vector_diagnostic_page_restore_before_eret_requested: bool,
    pub low_vector_diagnostic_page_restore_before_eret_attempted: bool,
    pub low_vector_diagnostic_page_entry_ipa: Option<u64>,
    pub low_vector_diagnostic_page_previous_descriptor: Option<u64>,
    pub low_vector_diagnostic_page_descriptor: Option<u64>,
    pub low_vector_diagnostic_page_repeated_fault_observed: bool,
    pub low_vector_recommended_vector_remap_requested: bool,
    pub low_vector_recommended_vector_remap_attempted: bool,
    pub low_vector_recommended_vector_remap_succeeded: bool,
    pub low_vector_recommended_vector_remap_target_physical_address: Option<u64>,
    pub low_vector_recommended_vector_remap_descriptor: Option<u64>,
    pub low_vector_post_repair_continue_requested: bool,
    pub low_vector_post_repair_continue_attempted: bool,
    pub stop_at_first_post_repair_device_boundary_requested: bool,
    pub low_vector_post_repair_unsupported_exit_observed: bool,
    pub low_vector_post_repair_unsupported_exit_reason: Option<u32>,
    pub low_vector_post_repair_unsupported_exit_diagnosis: &'static str,
    pub low_vector_post_repair_first_exit_observed: bool,
    pub low_vector_post_repair_first_exit_index: Option<u32>,
    pub low_vector_post_repair_first_exit_reason: Option<u32>,
    pub low_vector_post_repair_first_exit_diagnosis: &'static str,
    pub low_vector_post_repair_first_exit_pc: Option<u64>,
    pub low_vector_post_repair_first_interaction_kind: &'static str,
    pub low_vector_post_repair_first_exit_access_kind: &'static str,
    pub low_vector_post_repair_first_exit_access_direction: &'static str,
    pub low_vector_post_repair_first_exit_access_address: Option<u64>,
    pub low_vector_post_repair_first_exit_access_sysreg: Option<u16>,
    pub low_vector_post_repair_first_exit_access_syndrome: Option<u64>,
    pub low_vector_post_repair_first_device_interaction_observed: bool,
    pub low_vector_post_repair_first_device_interaction_index: Option<u32>,
    pub low_vector_post_repair_first_device_interaction_reason: Option<u32>,
    pub low_vector_post_repair_first_device_interaction_diagnosis: &'static str,
    pub low_vector_post_repair_first_device_interaction_pc: Option<u64>,
    pub low_vector_post_repair_first_device_interaction_kind: &'static str,
    pub low_vector_post_repair_first_device_interaction_access_kind: &'static str,
    pub low_vector_post_repair_first_device_interaction_access_direction: &'static str,
    pub low_vector_post_repair_first_device_interaction_access_address: Option<u64>,
    pub low_vector_post_repair_first_device_interaction_access_sysreg: Option<u16>,
    pub low_vector_post_repair_first_device_interaction_access_syndrome: Option<u64>,
    pub low_vector_post_repair_first_unhandled_access_observed: bool,
    pub low_vector_post_repair_first_unhandled_access_index: Option<u32>,
    pub low_vector_post_repair_first_unhandled_access_reason: Option<u32>,
    pub low_vector_post_repair_first_unhandled_access_diagnosis: &'static str,
    pub low_vector_post_repair_first_unhandled_access_pc: Option<u64>,
    pub low_vector_post_repair_first_unhandled_access_syndrome: Option<u64>,
    pub low_vector_post_repair_first_unhandled_access_kind: &'static str,
    pub low_vector_post_repair_first_unhandled_access_direction: &'static str,
    pub low_vector_post_repair_first_unhandled_access_register: Option<u8>,
    pub low_vector_post_repair_first_unhandled_access_value: Option<u64>,
    pub low_vector_post_repair_first_unhandled_access_handler_result: &'static str,
    pub low_vector_post_repair_first_unhandled_access_mmio_ipa: Option<u64>,
    pub low_vector_post_repair_first_unhandled_access_mmio_width: Option<u8>,
    pub low_vector_post_repair_first_unhandled_access_mmio_device_kind: &'static str,
    pub low_vector_post_repair_first_unhandled_access_sysreg: Option<u16>,
    pub low_vector_post_repair_first_unhandled_access_sysreg_name: &'static str,
    pub low_vector_post_repair_first_unhandled_access_sysreg_op0: Option<u8>,
    pub low_vector_post_repair_first_unhandled_access_sysreg_op1: Option<u8>,
    pub low_vector_post_repair_first_unhandled_access_sysreg_crn: Option<u8>,
    pub low_vector_post_repair_first_unhandled_access_sysreg_crm: Option<u8>,
    pub low_vector_post_repair_first_unhandled_access_sysreg_op2: Option<u8>,
    pub low_vector_diagnostic_page_resume_attempted: bool,
    pub low_vector_diagnostic_page_resume_armed: bool,
    pub low_vector_diagnostic_page_resume_original_pc: Option<u64>,
    pub low_vector_diagnostic_page_resume_original_elr_el1: Option<u64>,
    pub low_vector_diagnostic_page_resume_original_esr_el1: Option<u64>,
    pub low_vector_diagnostic_page_resume_original_far_el1: Option<u64>,
    pub low_vector_diagnostic_page_resume_original_spsr_el1: Option<u64>,
    pub low_vector_diagnostic_page_original_slot_bytes: Option<[u8; 12]>,
    pub low_vector_diagnostic_page_resume_target_instruction_before_eret: Option<u32>,
    pub low_vector_diagnostic_page_resume_target_stage1_leaf_descriptor_before_eret: Option<u64>,
    pub low_vector_diagnostic_page_resume_target_stage1_leaf_kind_before_eret: &'static str,
    pub low_vector_diagnostic_page_resume_target_is_installed_diagnostic_hvc_before_eret: bool,
    pub low_vector_diagnostic_page_resume_elr_el1_set_status: Option<i32>,
    pub low_vector_diagnostic_page_resume_spsr_el1_set_status: Option<i32>,
    pub low_vector_diagnostic_page_resume_cpsr_set_status: Option<i32>,
    pub low_vector_diagnostic_page_resume_pc_set_status: Option<i32>,
    pub vcpu_created: bool,
    pub pc_set: bool,
    pub x0_dtb_ipa_set: bool,
    pub cpsr_set: bool,
    pub sp_el1_set: bool,
    pub diagnostic_vector_vbar_el1_set: bool,
    pub recommended_vector_base_vbar_requested: bool,
    pub recommended_vector_base_vbar_attempted: bool,
    pub recommended_vector_base_vbar_set: bool,
    pub recommended_vector_base_vbar_diagnostic_vector_populated: bool,
    pub recommended_vector_base_vbar_resume_requested: bool,
    pub recommended_vector_base_vbar_resume_attempted: bool,
    pub recommended_vector_base_vbar_resume_armed: bool,
    pub interrupt_timer_wiring_requested: bool,
    pub interrupt_timer_initialized: bool,
    pub run_loop_attempted: bool,
    pub firmware_progress_observed: bool,
    pub unsupported_exit_observed: bool,
    pub watchdog_cancel_fired: bool,
    pub vcpu_destroyed: bool,
    pub firmware_memory_unmapped: bool,
    pub vars_memory_unmapped: bool,
    pub guest_ram_memory_unmapped: bool,
    pub firmware_memory_deallocated: bool,
    pub vars_memory_deallocated: bool,
    pub guest_ram_memory_deallocated: bool,
    pub vm_destroyed: bool,
    pub host: HvfHostCapabilities,
    pub pflash_map_verified: bool,
    pub reset_vector_ipa: u64,
    pub firmware_slot_ipa: u64,
    pub vars_slot_ipa: u64,
    pub low_firmware_alias_ipa: u64,
    pub low_vars_alias_ipa: u64,
    pub guest_ram_ipa: u64,
    pub platform_dtb_ipa: u64,
    pub platform_dtb_guest_ram_offset: u64,
    pub sp_el1_seed_ipa: u64,
    pub diagnostic_vector_location: &'static str,
    pub diagnostic_vector_ipa: u64,
    pub diagnostic_vector_bytes: u64,
    pub recommended_vector_base_vbar_source_exit_index: Option<u32>,
    pub recommended_vector_base_vbar_target: Option<u64>,
    pub recommended_vector_base_vbar_target_physical_address: Option<u64>,
    pub recommended_vector_base_vbar_reason: &'static str,
    pub recommended_vector_base_vbar_current_el_spx_sync_instruction_word: Option<u32>,
    pub recommended_vector_base_vbar_current_el_spx_sync_instruction_hint: &'static str,
    pub recommended_vector_base_vbar_followup_exit_observed: bool,
    pub recommended_vector_base_vbar_followup_exit_index: Option<u32>,
    pub recommended_vector_base_vbar_followup_exit_reason: Option<u32>,
    pub recommended_vector_base_vbar_followup_exit_diagnosis: &'static str,
    pub recommended_vector_base_vbar_followup_pc: Option<u64>,
    pub recommended_vector_base_vbar_followup_vbar_el1: Option<u64>,
    pub recommended_vector_base_vbar_followup_target_still_set: bool,
    pub recommended_vector_base_vbar_resume_original_pc: Option<u64>,
    pub recommended_vector_base_vbar_resume_original_elr_el1: Option<u64>,
    pub recommended_vector_base_vbar_resume_original_esr_el1: Option<u64>,
    pub recommended_vector_base_vbar_resume_original_far_el1: Option<u64>,
    pub recommended_vector_base_vbar_resume_original_spsr_el1: Option<u64>,
    pub slot_bytes: u64,
    pub guest_ram_bytes: u64,
    pub platform_dtb_bytes: usize,
    pub platform_dtb_magic: u32,
    pub platform_dtb_magic_verified: bool,
    pub requested_exits: u32,
    pub observed_exits: u32,
    pub watchdog_timeout_ms: u64,
    pub vtimer_offset_value: Option<u64>,
    pub cntv_cval_value: Option<u64>,
    pub cntv_ctl_value: Option<u64>,
    pub vtimer_exit_count: u32,
    pub pending_irq_injected_count: u32,
    pub device_irq_injected_count: u32,
    pub device_irq_cleared_count: u32,
    pub handled_mmio_read_count: u32,
    pub handled_mmio_write_count: u32,
    pub handled_pl011_mmio_count: u32,
    pub handled_pl031_mmio_count: u32,
    pub handled_gicd_mmio_count: u32,
    pub handled_gicr_mmio_count: u32,
    pub handled_virtio_installer_iso_mmio_count: u32,
    pub handled_virtio_target_disk_mmio_count: u32,
    pub virtio_queue_notify_count: u32,
    pub virtio_request_completion_count: u32,
    pub handled_icc_read_count: u32,
    pub handled_icc_write_count: u32,
    pub handled_icc_iar1_read_count: u32,
    pub handled_icc_eoir1_write_count: u32,
    pub handled_icc_dir_write_count: u32,
    pub last_icc_iar1_intid: Option<u32>,
    pub last_icc_eoir1_intid: Option<u32>,
    pub last_icc_dir_intid: Option<u32>,
    pub firmware_source_bytes: Option<u64>,
    pub vars_source_bytes: Option<u64>,
    pub installer_iso_path: Option<PathBuf>,
    pub writable_target_disk_path: Option<PathBuf>,
    pub block_devices: Vec<WindowsArmVirtioBlockDeviceMetadata>,
    pub firmware_map_flags: &'static str,
    pub vars_map_flags: &'static str,
    pub low_firmware_alias_map_flags: &'static str,
    pub low_vars_alias_map_flags: &'static str,
    pub guest_ram_map_flags: &'static str,
    pub low_pflash_alias_requested: bool,
    pub vm_create_status: Option<i32>,
    pub firmware_allocate_status: Option<i32>,
    pub vars_allocate_status: Option<i32>,
    pub guest_ram_allocate_status: Option<i32>,
    pub firmware_map_status: Option<i32>,
    pub vars_map_status: Option<i32>,
    pub low_firmware_alias_map_status: Option<i32>,
    pub low_vars_alias_map_status: Option<i32>,
    pub guest_ram_map_status: Option<i32>,
    pub vcpu_create_status: Option<i32>,
    pub pc_set_status: Option<i32>,
    pub x0_dtb_ipa_set_status: Option<i32>,
    pub cpsr_set_status: Option<i32>,
    pub sp_el1_set_status: Option<i32>,
    pub diagnostic_vector_vbar_el1_set_status: Option<i32>,
    pub recommended_vector_base_vbar_set_status: Option<i32>,
    pub recommended_vector_base_vbar_resume_vbar_el1_set_status: Option<i32>,
    pub recommended_vector_base_vbar_resume_elr_el1_set_status: Option<i32>,
    pub recommended_vector_base_vbar_resume_spsr_el1_set_status: Option<i32>,
    pub recommended_vector_base_vbar_resume_pc_set_status: Option<i32>,
    pub vtimer_offset_set_status: Option<i32>,
    pub cntv_cval_set_status: Option<i32>,
    pub cntv_ctl_set_status: Option<i32>,
    pub vtimer_initial_unmask_status: Option<i32>,
    pub last_pending_irq_set_status: Option<i32>,
    pub last_device_irq_set_status: Option<i32>,
    pub last_device_irq_clear_status: Option<i32>,
    pub last_vtimer_unmask_status: Option<i32>,
    pub final_pc_status: Option<i32>,
    pub final_pc: Option<u64>,
    pub vcpu_destroy_status: Option<i32>,
    pub firmware_unmap_status: Option<i32>,
    pub vars_unmap_status: Option<i32>,
    pub low_firmware_alias_unmap_status: Option<i32>,
    pub low_vars_alias_unmap_status: Option<i32>,
    pub guest_ram_unmap_status: Option<i32>,
    pub firmware_deallocate_status: Option<i32>,
    pub vars_deallocate_status: Option<i32>,
    pub guest_ram_deallocate_status: Option<i32>,
    pub vm_destroy_status: Option<i32>,
    pub exits: Vec<WindowsArmUefiFirmwareRunLoopExit>,
    pub blockers: Vec<String>,
}

impl WindowsArmUefiFirmwareRunLoopProbe {
    pub(crate) fn low_vector_post_repair_first_exit_telemetry(
        &self,
    ) -> LowVectorPostRepairExitTelemetry {
        LowVectorPostRepairExitTelemetry {
            observed: self.low_vector_post_repair_first_exit_observed,
            index: self.low_vector_post_repair_first_exit_index,
            reason: self.low_vector_post_repair_first_exit_reason,
            diagnosis: self.low_vector_post_repair_first_exit_diagnosis,
            pc: self.low_vector_post_repair_first_exit_pc,
            interaction_kind: self.low_vector_post_repair_first_interaction_kind,
            access: LowVectorPostRepairAccessTelemetry {
                kind: self.low_vector_post_repair_first_exit_access_kind,
                direction: self.low_vector_post_repair_first_exit_access_direction,
                address: self.low_vector_post_repair_first_exit_access_address,
                sysreg: self.low_vector_post_repair_first_exit_access_sysreg,
                syndrome: self.low_vector_post_repair_first_exit_access_syndrome,
            },
        }
    }

    pub(crate) fn low_vector_post_repair_first_device_interaction_telemetry(
        &self,
    ) -> LowVectorPostRepairExitTelemetry {
        LowVectorPostRepairExitTelemetry {
            observed: self.low_vector_post_repair_first_device_interaction_observed,
            index: self.low_vector_post_repair_first_device_interaction_index,
            reason: self.low_vector_post_repair_first_device_interaction_reason,
            diagnosis: self.low_vector_post_repair_first_device_interaction_diagnosis,
            pc: self.low_vector_post_repair_first_device_interaction_pc,
            interaction_kind: self.low_vector_post_repair_first_device_interaction_kind,
            access: LowVectorPostRepairAccessTelemetry {
                kind: self.low_vector_post_repair_first_device_interaction_access_kind,
                direction: self.low_vector_post_repair_first_device_interaction_access_direction,
                address: self.low_vector_post_repair_first_device_interaction_access_address,
                sysreg: self.low_vector_post_repair_first_device_interaction_access_sysreg,
                syndrome: self.low_vector_post_repair_first_device_interaction_access_syndrome,
            },
        }
    }

    pub(crate) fn low_vector_post_repair_first_unhandled_access_telemetry(
        &self,
    ) -> LowVectorPostRepairUnhandledAccessTelemetry {
        LowVectorPostRepairUnhandledAccessTelemetry {
            observed: self.low_vector_post_repair_first_unhandled_access_observed,
            index: self.low_vector_post_repair_first_unhandled_access_index,
            reason: self.low_vector_post_repair_first_unhandled_access_reason,
            diagnosis: self.low_vector_post_repair_first_unhandled_access_diagnosis,
            pc: self.low_vector_post_repair_first_unhandled_access_pc,
            syndrome: self.low_vector_post_repair_first_unhandled_access_syndrome,
            kind: self.low_vector_post_repair_first_unhandled_access_kind,
            access: self.low_vector_post_repair_first_unhandled_access_direction,
            register: self.low_vector_post_repair_first_unhandled_access_register,
            value: self.low_vector_post_repair_first_unhandled_access_value,
            handler_result: self.low_vector_post_repair_first_unhandled_access_handler_result,
            mmio_ipa: self.low_vector_post_repair_first_unhandled_access_mmio_ipa,
            mmio_width: self.low_vector_post_repair_first_unhandled_access_mmio_width,
            mmio_device_kind: self.low_vector_post_repair_first_unhandled_access_mmio_device_kind,
            sysreg: self.low_vector_post_repair_first_unhandled_access_sysreg,
            sysreg_name: self.low_vector_post_repair_first_unhandled_access_sysreg_name,
            sysreg_op0: self.low_vector_post_repair_first_unhandled_access_sysreg_op0,
            sysreg_op1: self.low_vector_post_repair_first_unhandled_access_sysreg_op1,
            sysreg_crn: self.low_vector_post_repair_first_unhandled_access_sysreg_crn,
            sysreg_crm: self.low_vector_post_repair_first_unhandled_access_sysreg_crm,
            sysreg_op2: self.low_vector_post_repair_first_unhandled_access_sysreg_op2,
        }
    }

    pub fn render_text(&self) -> String {
        let mut output = String::new();
        output.push_str("Windows 11 Arm HVF UEFI firmware run-loop probe\n");
        output.push_str("QEMU: not used\n");
        output.push_str("Apple VZ: not used\n");
        output.push_str("Guest execution: bounded UEFI firmware exit classification loop\n");
        output.push_str("Windows boot: not claimed\n");
        output.push_str(&format!(
            "Device models: {}\n",
            WINDOWS_ARM_FIRMWARE_MMIO_DEVICE_MODELS
        ));
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
            "Guest RAM memory allocated: {}\n",
            self.guest_ram_memory_allocated
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
            "Low firmware alias mapped: {}\n",
            self.low_firmware_alias_mapped
        ));
        output.push_str(&format!(
            "Low vars alias mapped: {}\n",
            self.low_vars_alias_mapped
        ));
        output.push_str(&format!(
            "Guest RAM memory mapped: {}\n",
            self.guest_ram_memory_mapped
        ));
        output.push_str(&format!(
            "Platform DTB populated: {}\n",
            self.platform_dtb_populated
        ));
        output.push_str(&format!(
            "Diagnostic vector seed requested: {}\n",
            self.diagnostic_vector_seed_requested
        ));
        output.push_str(&format!(
            "Diagnostic vector populated: {}\n",
            self.diagnostic_vector_populated
        ));
        output.push_str(&format!(
            "Low vector diagnostic page repair requested: {}\n",
            self.low_vector_diagnostic_page_repair_requested
        ));
        output.push_str(&format!(
            "Low vector diagnostic page repaired: {}\n",
            self.low_vector_diagnostic_page_repaired
        ));
        output.push_str(&format!(
            "Low vector diagnostic page slot restored: {}\n",
            self.low_vector_diagnostic_page_slot_restored
        ));
        output.push_str(&format!(
            "Low vector diagnostic page restore before ERET requested: {}\n",
            self.low_vector_diagnostic_page_restore_before_eret_requested
        ));
        output.push_str(&format!(
            "Low vector diagnostic page restore before ERET attempted: {}\n",
            self.low_vector_diagnostic_page_restore_before_eret_attempted
        ));
        output.push_str(&format!(
            "Low vector diagnostic page entry IPA: {}\n",
            render_optional_u64(self.low_vector_diagnostic_page_entry_ipa)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page previous descriptor: {}\n",
            render_optional_u64(self.low_vector_diagnostic_page_previous_descriptor)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page descriptor: {}\n",
            render_optional_u64(self.low_vector_diagnostic_page_descriptor)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page repeated fault observed: {}\n",
            self.low_vector_diagnostic_page_repeated_fault_observed
        ));
        output.push_str(&format!(
            "Low vector recommended-vector remap requested: {}\n",
            self.low_vector_recommended_vector_remap_requested
        ));
        output.push_str(&format!(
            "Low vector recommended-vector remap attempted: {}\n",
            self.low_vector_recommended_vector_remap_attempted
        ));
        output.push_str(&format!(
            "Low vector recommended-vector remap succeeded: {}\n",
            self.low_vector_recommended_vector_remap_succeeded
        ));
        output.push_str(&format!(
            "Low vector recommended-vector remap target PA: {}\n",
            render_optional_u64(self.low_vector_recommended_vector_remap_target_physical_address)
        ));
        output.push_str(&format!(
            "Low vector recommended-vector remap descriptor: {}\n",
            render_optional_u64(self.low_vector_recommended_vector_remap_descriptor)
        ));
        output.push_str(&format!(
            "Continue after low-vector repair requested: {}\n",
            self.low_vector_post_repair_continue_requested
        ));
        output.push_str(&format!(
            "Continue after low-vector repair attempted: {}\n",
            self.low_vector_post_repair_continue_attempted
        ));
        output.push_str(&format!(
            "Stop at first post-repair device boundary requested: {}\n",
            self.stop_at_first_post_repair_device_boundary_requested
        ));
        output.push_str(&format!(
            "Post-repair unsupported exit observed: {}\n",
            self.low_vector_post_repair_unsupported_exit_observed
        ));
        output.push_str(&format!(
            "Post-repair unsupported exit reason name: {}\n",
            render_optional_exit_reason_name(self.low_vector_post_repair_unsupported_exit_reason)
        ));
        output.push_str(&format!(
            "Post-repair unsupported exit classification: {}\n",
            self.low_vector_post_repair_unsupported_exit_diagnosis
        ));
        let post_repair_first_exit = self.low_vector_post_repair_first_exit_telemetry();
        let post_repair_first_exit_context =
            low_vector_post_repair_context_exit(&self.exits, post_repair_first_exit.index);
        append_low_vector_post_repair_exit_telemetry(
            &mut output,
            "Post-repair first exit",
            &post_repair_first_exit,
            "Post-repair first interaction kind",
            post_repair_first_exit_context,
        );
        let post_repair_first_device_interaction =
            self.low_vector_post_repair_first_device_interaction_telemetry();
        let post_repair_first_device_interaction_context = low_vector_post_repair_context_exit(
            &self.exits,
            post_repair_first_device_interaction.index,
        );
        append_low_vector_post_repair_exit_telemetry(
            &mut output,
            "Post-repair first device interaction",
            &post_repair_first_device_interaction,
            "Post-repair first device interaction kind",
            post_repair_first_device_interaction_context,
        );
        let post_repair_first_unhandled_access =
            self.low_vector_post_repair_first_unhandled_access_telemetry();
        append_low_vector_post_repair_unhandled_access_telemetry(
            &mut output,
            "Post-repair first unhandled access",
            &post_repair_first_unhandled_access,
        );
        output.push_str(&format!(
            "Low vector diagnostic page resume attempted: {}\n",
            self.low_vector_diagnostic_page_resume_attempted
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume armed: {}\n",
            self.low_vector_diagnostic_page_resume_armed
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume original PC: {}\n",
            render_optional_u64(self.low_vector_diagnostic_page_resume_original_pc)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume original ELR_EL1: {}\n",
            render_optional_u64(self.low_vector_diagnostic_page_resume_original_elr_el1)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume original ESR_EL1: {}\n",
            render_optional_u64(self.low_vector_diagnostic_page_resume_original_esr_el1)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume original FAR_EL1: {}\n",
            render_optional_u64(self.low_vector_diagnostic_page_resume_original_far_el1)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume original SPSR_EL1: {}\n",
            render_optional_u64(self.low_vector_diagnostic_page_resume_original_spsr_el1)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page original slot bytes: {}\n",
            self.low_vector_diagnostic_page_original_slot_bytes
                .as_ref()
                .map_or_else(
                    || "not observed".to_string(),
                    |bytes| render_hex_bytes(bytes)
                )
        ));
        let original_sync_instruction = self
            .low_vector_diagnostic_page_original_slot_bytes
            .and_then(|bytes| Some(u32::from_le_bytes(bytes[0..4].try_into().ok()?)));
        output.push_str(&format!(
            "Low vector diagnostic page original sync instruction: {}\n",
            render_optional_instruction_word(original_sync_instruction)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page original sync hint: {}\n",
            original_sync_instruction
                .map(aarch64_instruction_hint)
                .unwrap_or("not observed")
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume target instruction before ERET: {}\n",
            render_optional_instruction_word(
                self.low_vector_diagnostic_page_resume_target_instruction_before_eret,
            )
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume target hint before ERET: {}\n",
            self.low_vector_diagnostic_page_resume_target_instruction_before_eret
                .map(aarch64_instruction_hint)
                .unwrap_or("not observed")
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume target stage-1 descriptor before ERET: {}\n",
            render_optional_u64(
                self.low_vector_diagnostic_page_resume_target_stage1_leaf_descriptor_before_eret,
            )
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume target stage-1 kind before ERET: {}\n",
            self.low_vector_diagnostic_page_resume_target_stage1_leaf_kind_before_eret
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume target is installed diagnostic HVC before ERET: {}\n",
            self.low_vector_diagnostic_page_resume_target_is_installed_diagnostic_hvc_before_eret
        ));
        output.push_str(&format!("vCPU created: {}\n", self.vcpu_created));
        output.push_str(&format!("PC set: {}\n", self.pc_set));
        output.push_str(&format!("X0 DTB IPA set: {}\n", self.x0_dtb_ipa_set));
        output.push_str(&format!("CPSR set: {}\n", self.cpsr_set));
        output.push_str(&format!("SP_EL1 set: {}\n", self.sp_el1_set));
        output.push_str(&format!(
            "Diagnostic vector VBAR_EL1 set: {}\n",
            self.diagnostic_vector_vbar_el1_set
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR requested: {}\n",
            self.recommended_vector_base_vbar_requested
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR attempted: {}\n",
            self.recommended_vector_base_vbar_attempted
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR set: {}\n",
            self.recommended_vector_base_vbar_set
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR diagnostic vector populated: {}\n",
            self.recommended_vector_base_vbar_diagnostic_vector_populated
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume requested: {}\n",
            self.recommended_vector_base_vbar_resume_requested
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume attempted: {}\n",
            self.recommended_vector_base_vbar_resume_attempted
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume armed: {}\n",
            self.recommended_vector_base_vbar_resume_armed
        ));
        output.push_str(&format!(
            "Interrupt/timer wiring requested: {}\n",
            self.interrupt_timer_wiring_requested
        ));
        output.push_str(&format!(
            "Interrupt/timer initialized: {}\n",
            self.interrupt_timer_initialized
        ));
        output.push_str(&format!(
            "Run loop attempted: {}\n",
            self.run_loop_attempted
        ));
        output.push_str(&format!(
            "Firmware progress observed: {}\n",
            self.firmware_progress_observed
        ));
        output.push_str(&format!(
            "Unsupported exit observed: {}\n",
            self.unsupported_exit_observed
        ));
        output.push_str(&format!(
            "Watchdog cancel fired: {}\n",
            self.watchdog_cancel_fired
        ));
        output.push_str(&format!("vCPU destroyed: {}\n", self.vcpu_destroyed));
        output.push_str(&format!(
            "Firmware memory unmapped: {}\n",
            self.firmware_memory_unmapped
        ));
        output.push_str(&format!(
            "Vars memory unmapped: {}\n",
            self.vars_memory_unmapped
        ));
        output.push_str(&format!(
            "Guest RAM memory unmapped: {}\n",
            self.guest_ram_memory_unmapped
        ));
        output.push_str(&format!(
            "Firmware memory deallocated: {}\n",
            self.firmware_memory_deallocated
        ));
        output.push_str(&format!(
            "Vars memory deallocated: {}\n",
            self.vars_memory_deallocated
        ));
        output.push_str(&format!(
            "Guest RAM memory deallocated: {}\n",
            self.guest_ram_memory_deallocated
        ));
        output.push_str(&format!("VM destroyed: {}\n", self.vm_destroyed));
        output.push_str(&format!(
            "Pflash map verified: {}\n",
            self.pflash_map_verified
        ));
        output.push_str(&format!("Reset vector IPA: {:#x}\n", self.reset_vector_ipa));
        output.push_str(&format!(
            "Firmware slot IPA: {:#x}\n",
            self.firmware_slot_ipa
        ));
        output.push_str(&format!("Vars slot IPA: {:#x}\n", self.vars_slot_ipa));
        output.push_str(&format!(
            "Low firmware alias IPA: {:#x}\n",
            self.low_firmware_alias_ipa
        ));
        output.push_str(&format!(
            "Low vars alias IPA: {:#x}\n",
            self.low_vars_alias_ipa
        ));
        output.push_str(&format!("Guest RAM IPA: {:#x}\n", self.guest_ram_ipa));
        output.push_str(&format!("Platform DTB IPA: {:#x}\n", self.platform_dtb_ipa));
        output.push_str(&format!(
            "Platform DTB guest RAM offset: {:#x}\n",
            self.platform_dtb_guest_ram_offset
        ));
        output.push_str(&format!("SP_EL1 seed IPA: {:#x}\n", self.sp_el1_seed_ipa));
        output.push_str(&format!(
            "Diagnostic vector location: {}\n",
            self.diagnostic_vector_location
        ));
        output.push_str(&format!(
            "Diagnostic vector IPA: {:#x}\n",
            self.diagnostic_vector_ipa
        ));
        output.push_str(&format!(
            "Diagnostic vector bytes: {:#x}\n",
            self.diagnostic_vector_bytes
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR source exit: {}\n",
            render_optional_intid(self.recommended_vector_base_vbar_source_exit_index)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR target: {}\n",
            render_optional_u64(self.recommended_vector_base_vbar_target)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR target PA: {}\n",
            render_optional_u64(self.recommended_vector_base_vbar_target_physical_address)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR reason: {}\n",
            self.recommended_vector_base_vbar_reason
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR current EL/SPx sync instruction: {}\n",
            render_optional_instruction_word(
                self.recommended_vector_base_vbar_current_el_spx_sync_instruction_word,
            )
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR current EL/SPx sync hint: {}\n",
            self.recommended_vector_base_vbar_current_el_spx_sync_instruction_hint
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR follow-up exit observed: {}\n",
            self.recommended_vector_base_vbar_followup_exit_observed
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR follow-up exit: {}\n",
            render_optional_intid(self.recommended_vector_base_vbar_followup_exit_index)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR follow-up exit reason name: {}\n",
            render_optional_exit_reason_name(
                self.recommended_vector_base_vbar_followup_exit_reason
            )
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR follow-up classification: {}\n",
            self.recommended_vector_base_vbar_followup_exit_diagnosis
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR follow-up PC: {}\n",
            render_optional_u64(self.recommended_vector_base_vbar_followup_pc)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR follow-up VBAR_EL1: {}\n",
            render_optional_u64(self.recommended_vector_base_vbar_followup_vbar_el1)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR follow-up target still set: {}\n",
            self.recommended_vector_base_vbar_followup_target_still_set
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume original PC: {}\n",
            render_optional_u64(self.recommended_vector_base_vbar_resume_original_pc)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume original ELR_EL1: {}\n",
            render_optional_u64(self.recommended_vector_base_vbar_resume_original_elr_el1)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume original ESR_EL1: {}\n",
            render_optional_u64(self.recommended_vector_base_vbar_resume_original_esr_el1)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume original FAR_EL1: {}\n",
            render_optional_u64(self.recommended_vector_base_vbar_resume_original_far_el1)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume original SPSR_EL1: {}\n",
            render_optional_u64(self.recommended_vector_base_vbar_resume_original_spsr_el1)
        ));
        output.push_str(&format!("Slot bytes: {:#x}\n", self.slot_bytes));
        output.push_str(&format!("Guest RAM bytes: {:#x}\n", self.guest_ram_bytes));
        output.push_str(&format!(
            "Platform DTB bytes: {:#x}\n",
            self.platform_dtb_bytes
        ));
        output.push_str(&format!(
            "Platform DTB magic: {:#x}\n",
            self.platform_dtb_magic
        ));
        output.push_str(&format!(
            "Platform DTB magic verified: {}\n",
            self.platform_dtb_magic_verified
        ));
        output.push_str(&format!("Requested exits: {}\n", self.requested_exits));
        output.push_str(&format!("Observed exits: {}\n", self.observed_exits));
        output.push_str(&format!(
            "Watchdog timeout ms: {}\n",
            self.watchdog_timeout_ms
        ));
        output.push_str(&format!(
            "VTimer offset value: {}\n",
            render_optional_u64(self.vtimer_offset_value)
        ));
        output.push_str(&format!(
            "CNTV_CVAL_EL0 value: {}\n",
            render_optional_u64(self.cntv_cval_value)
        ));
        output.push_str(&format!(
            "CNTV_CTL_EL0 value: {}\n",
            render_optional_u64(self.cntv_ctl_value)
        ));
        output.push_str(&format!("VTimer exit count: {}\n", self.vtimer_exit_count));
        output.push_str(&format!(
            "Pending IRQ injected count: {}\n",
            self.pending_irq_injected_count
        ));
        output.push_str(&format!(
            "Device IRQ line asserted count: {}\n",
            self.device_irq_injected_count
        ));
        output.push_str(&format!(
            "Device IRQ line deasserted count: {}\n",
            self.device_irq_cleared_count
        ));
        output.push_str(&format!(
            "Handled MMIO read count: {}\n",
            self.handled_mmio_read_count
        ));
        output.push_str(&format!(
            "Handled MMIO write count: {}\n",
            self.handled_mmio_write_count
        ));
        output.push_str(&format!(
            "Handled PL011 MMIO count: {}\n",
            self.handled_pl011_mmio_count
        ));
        output.push_str(&format!(
            "Handled PL031 MMIO count: {}\n",
            self.handled_pl031_mmio_count
        ));
        output.push_str(&format!(
            "Handled GICD MMIO count: {}\n",
            self.handled_gicd_mmio_count
        ));
        output.push_str(&format!(
            "Handled GICR MMIO count: {}\n",
            self.handled_gicr_mmio_count
        ));
        output.push_str(&format!(
            "Handled VirtIO installer ISO MMIO count: {}\n",
            self.handled_virtio_installer_iso_mmio_count
        ));
        output.push_str(&format!(
            "Handled VirtIO target disk MMIO count: {}\n",
            self.handled_virtio_target_disk_mmio_count
        ));
        output.push_str(&format!(
            "VirtIO queue_notify count: {}\n",
            self.virtio_queue_notify_count
        ));
        output.push_str(&format!(
            "VirtIO request completion count: {}\n",
            self.virtio_request_completion_count
        ));
        output.push_str(&format!(
            "Handled ICC read count: {}\n",
            self.handled_icc_read_count
        ));
        output.push_str(&format!(
            "Handled ICC write count: {}\n",
            self.handled_icc_write_count
        ));
        output.push_str(&format!(
            "Handled ICC_IAR1 read count: {}\n",
            self.handled_icc_iar1_read_count
        ));
        output.push_str(&format!(
            "Handled ICC_EOIR1 write count: {}\n",
            self.handled_icc_eoir1_write_count
        ));
        output.push_str(&format!(
            "Handled ICC_DIR write count: {}\n",
            self.handled_icc_dir_write_count
        ));
        output.push_str(&format!(
            "Last ICC_IAR1 INTID: {}\n",
            render_optional_intid(self.last_icc_iar1_intid)
        ));
        output.push_str(&format!(
            "Last ICC_EOIR1 INTID: {}\n",
            render_optional_intid(self.last_icc_eoir1_intid)
        ));
        output.push_str(&format!(
            "Last ICC_DIR INTID: {}\n",
            render_optional_intid(self.last_icc_dir_intid)
        ));
        output.push_str(&format!(
            "Firmware source bytes: {}\n",
            render_optional_u64(self.firmware_source_bytes)
        ));
        output.push_str(&format!(
            "Vars source bytes: {}\n",
            render_optional_u64(self.vars_source_bytes)
        ));
        output.push_str(&format!(
            "Installer ISO path: {}\n",
            self.installer_iso_path.as_ref().map_or_else(
                || "not provided".to_string(),
                |path| path.display().to_string()
            )
        ));
        output.push_str(&format!(
            "Writable target disk path: {}\n",
            self.writable_target_disk_path.as_ref().map_or_else(
                || "not provided".to_string(),
                |path| path.display().to_string()
            )
        ));
        output.push_str("Firmware block devices:\n");
        for device in &self.block_devices {
            output.push_str(&format!(
                "- role={}, label={}, node={}, base={:#x}, bytes={:#x}, read_only={}, backing_kind={}, backing_path={}, device_features={:#x}, capacity_sectors={:#x}\n",
                device.role,
                device.label,
                device.node_name,
                device.base_ipa,
                device.bytes,
                device.read_only,
                device.backing_kind,
                device
                    .backing_path
                    .as_ref()
                    .map_or_else(|| "not provided".to_string(), |path| path.display().to_string()),
                device.device_features,
                device.capacity_sectors,
            ));
        }
        output.push_str(&format!(
            "Firmware map flags: {}\n",
            self.firmware_map_flags
        ));
        output.push_str(&format!("Vars map flags: {}\n", self.vars_map_flags));
        output.push_str(&format!(
            "Low firmware alias map flags: {}\n",
            self.low_firmware_alias_map_flags
        ));
        output.push_str(&format!(
            "Low vars alias map flags: {}\n",
            self.low_vars_alias_map_flags
        ));
        output.push_str(&format!(
            "Guest RAM map flags: {}\n",
            self.guest_ram_map_flags
        ));
        output.push_str(&format!(
            "Low pflash alias requested: {}\n",
            self.low_pflash_alias_requested
        ));
        output.push_str(&format!(
            "VM create status name: {}\n",
            render_optional_status_name(self.vm_create_status)
        ));
        output.push_str(&format!(
            "Firmware allocate status name: {}\n",
            render_optional_status_name(self.firmware_allocate_status)
        ));
        output.push_str(&format!(
            "Vars allocate status name: {}\n",
            render_optional_status_name(self.vars_allocate_status)
        ));
        output.push_str(&format!(
            "Guest RAM allocate status name: {}\n",
            render_optional_status_name(self.guest_ram_allocate_status)
        ));
        output.push_str(&format!(
            "Firmware map status name: {}\n",
            render_optional_status_name(self.firmware_map_status)
        ));
        output.push_str(&format!(
            "Vars map status name: {}\n",
            render_optional_status_name(self.vars_map_status)
        ));
        output.push_str(&format!(
            "Low firmware alias map status name: {}\n",
            render_optional_status_name(self.low_firmware_alias_map_status)
        ));
        output.push_str(&format!(
            "Low vars alias map status name: {}\n",
            render_optional_status_name(self.low_vars_alias_map_status)
        ));
        output.push_str(&format!(
            "Guest RAM map status name: {}\n",
            render_optional_status_name(self.guest_ram_map_status)
        ));
        output.push_str(&format!(
            "vCPU create status name: {}\n",
            render_optional_status_name(self.vcpu_create_status)
        ));
        output.push_str(&format!(
            "PC set status name: {}\n",
            render_optional_status_name(self.pc_set_status)
        ));
        output.push_str(&format!(
            "X0 DTB IPA set status name: {}\n",
            render_optional_status_name(self.x0_dtb_ipa_set_status)
        ));
        output.push_str(&format!(
            "CPSR set status name: {}\n",
            render_optional_status_name(self.cpsr_set_status)
        ));
        output.push_str(&format!(
            "SP_EL1 set status name: {}\n",
            render_optional_status_name(self.sp_el1_set_status)
        ));
        output.push_str(&format!(
            "Diagnostic vector VBAR_EL1 set status name: {}\n",
            render_optional_status_name(self.diagnostic_vector_vbar_el1_set_status)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR set status name: {}\n",
            render_optional_status_name(self.recommended_vector_base_vbar_set_status)
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume ELR_EL1 set status name: {}\n",
            render_optional_status_name(
                self.recommended_vector_base_vbar_resume_elr_el1_set_status,
            )
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume VBAR_EL1 set status name: {}\n",
            render_optional_status_name(
                self.recommended_vector_base_vbar_resume_vbar_el1_set_status,
            )
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume SPSR_EL1 set status name: {}\n",
            render_optional_status_name(
                self.recommended_vector_base_vbar_resume_spsr_el1_set_status,
            )
        ));
        output.push_str(&format!(
            "Recommended vector-base VBAR resume PC set status name: {}\n",
            render_optional_status_name(self.recommended_vector_base_vbar_resume_pc_set_status)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume ELR_EL1 set status name: {}\n",
            render_optional_status_name(self.low_vector_diagnostic_page_resume_elr_el1_set_status)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume SPSR_EL1 set status name: {}\n",
            render_optional_status_name(self.low_vector_diagnostic_page_resume_spsr_el1_set_status)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume CPSR set status name: {}\n",
            render_optional_status_name(self.low_vector_diagnostic_page_resume_cpsr_set_status)
        ));
        output.push_str(&format!(
            "Low vector diagnostic page resume PC set status name: {}\n",
            render_optional_status_name(self.low_vector_diagnostic_page_resume_pc_set_status)
        ));
        output.push_str(&format!(
            "VTimer offset set status name: {}\n",
            render_optional_status_name(self.vtimer_offset_set_status)
        ));
        output.push_str(&format!(
            "CNTV_CVAL_EL0 set status name: {}\n",
            render_optional_status_name(self.cntv_cval_set_status)
        ));
        output.push_str(&format!(
            "CNTV_CTL_EL0 set status name: {}\n",
            render_optional_status_name(self.cntv_ctl_set_status)
        ));
        output.push_str(&format!(
            "VTimer initial unmask status name: {}\n",
            render_optional_status_name(self.vtimer_initial_unmask_status)
        ));
        output.push_str(&format!(
            "Last pending IRQ set status name: {}\n",
            render_optional_status_name(self.last_pending_irq_set_status)
        ));
        output.push_str(&format!(
            "Last device IRQ line assert status name: {}\n",
            render_optional_status_name(self.last_device_irq_set_status)
        ));
        output.push_str(&format!(
            "Last device IRQ line deassert status name: {}\n",
            render_optional_status_name(self.last_device_irq_clear_status)
        ));
        output.push_str(&format!(
            "Last VTimer unmask status name: {}\n",
            render_optional_status_name(self.last_vtimer_unmask_status)
        ));
        output.push_str(&format!(
            "Final PC status name: {}\n",
            render_optional_status_name(self.final_pc_status)
        ));
        output.push_str(&format!(
            "Final PC: {}\n",
            render_optional_u64(self.final_pc)
        ));
        output.push_str("Run-loop exits:\n");
        if self.exits.is_empty() {
            output.push_str("- none\n");
        } else {
            for exit in &self.exits {
                output.push_str(&format!(
                    "- Exit {}: run={}, reason={}, exception_class={}, exception_class_name={}, syndrome={}, abort_iss={}, abort_fault_status={}, abort_fault_status_name={}, va={}, va_region={}, pa={}, pa_region={}, pc={}, instruction={}, instruction_hint={}, pc_stage1_leaf_level={}, pc_stage1_leaf_descriptor={}, pc_stage1_leaf_kind={}, pc_stage1_leaf_pxn={}, pc_stage1_leaf_uxn={}, x0={}, x1={}, x2={}, x3={}, x4={}, cpsr={}, vbar_el1={}, elr_el1={}, esr_el1={}, esr_el1_class_name={}, esr_el1_fault_status_name={}, far_el1={}, spsr_el1={}, sctlr_el1={}, sctlr_el1_mmu_enabled={}, tcr_el1={}, ttbr0_el1={}, ttbr1_el1={}, mair_el1={}, sp_el1={}, diagnosis={}, watchdog={}, vtimer_auto_mask={}, vtimer_auto_mask_get={}, vtimer_rearm_cval={}, vtimer_rearm_cval_set={}, vtimer_ppi_pending_recorded={}, vtimer_irq_line_assertable={}, vtimer_gic_group1_enabled={}, vtimer_gic_priority_mask={}, vtimer_gic_running_priority={}, vtimer_gic_priority_threshold={}, vtimer_gic_pending_intid={}, vtimer_pending_irq={}, vtimer_unmask={}, handled={}\n",
                    exit.index,
                    render_optional_status_name(exit.run_status),
                    render_optional_exit_reason_name(exit.exit_reason),
                    render_optional_u64(exit.exit_exception_class),
                    render_optional_exception_class_name(exit.exit_exception_class),
                    render_optional_u64(exit.exit_syndrome),
                    render_optional_abort_iss(exit.exit_syndrome),
                    render_optional_abort_fault_status(exit.exit_syndrome),
                    render_optional_abort_fault_status_name(exit.exit_syndrome),
                    render_optional_u64(exit.exit_virtual_address),
                    windows_arm_guest_region_name(exit.exit_virtual_address, self.guest_ram_bytes),
                    render_optional_u64(exit.exit_physical_address),
                    windows_arm_guest_region_name(exit.exit_physical_address, self.guest_ram_bytes),
                    render_optional_u64(exit.pc_after_exit),
                    render_optional_instruction_word(exit.instruction_word_after_exit),
                    exit.instruction_hint_after_exit,
                    render_optional_u8(exit.pc_stage1_leaf_level_after_exit),
                    render_optional_u64(exit.pc_stage1_leaf_descriptor_after_exit),
                    exit.pc_stage1_leaf_descriptor_kind_after_exit,
                    render_optional_bool(exit.pc_stage1_leaf_pxn_after_exit),
                    render_optional_bool(exit.pc_stage1_leaf_uxn_after_exit),
                    render_optional_u64(exit.x0_after_exit),
                    render_optional_u64(exit.x1_after_exit),
                    render_optional_u64(exit.x2_after_exit),
                    render_optional_u64(exit.x3_after_exit),
                    render_optional_u64(exit.x4_after_exit),
                    render_optional_u64(exit.cpsr_after_exit),
                    render_optional_u64(exit.vbar_el1_after_exit),
                    render_optional_u64(exit.elr_el1_after_exit),
                    render_optional_u64(exit.esr_el1_after_exit),
                    render_optional_esr_exception_class_name(exit.esr_el1_after_exit),
                    render_optional_abort_fault_status_name(exit.esr_el1_after_exit),
                    render_optional_u64(exit.far_el1_after_exit),
                    render_optional_u64(exit.spsr_el1_after_exit),
                    render_optional_u64(exit.sctlr_el1_after_exit),
                    render_optional_sctlr_mmu_enabled(exit.sctlr_el1_after_exit),
                    render_optional_u64(exit.tcr_el1_after_exit),
                    render_optional_u64(exit.ttbr0_el1_after_exit),
                    render_optional_u64(exit.ttbr1_el1_after_exit),
                    render_optional_u64(exit.mair_el1_after_exit),
                    render_optional_u64(exit.sp_el1_after_exit),
                    windows_arm_firmware_run_loop_exit_diagnosis(exit),
                    render_optional_status_name(exit.watchdog_cancel_status),
                    render_optional_bool(exit.vtimer_auto_mask_after_exit),
                    render_optional_status_name(exit.vtimer_auto_mask_get_status),
                    render_optional_u64(exit.vtimer_rearm_cval_value),
                    render_optional_status_name(exit.vtimer_rearm_cval_set_status),
                    render_optional_bool(exit.vtimer_ppi_pending_recorded),
                    render_optional_bool(exit.vtimer_irq_line_assertable),
                    render_optional_bool(exit.vtimer_gic_group1_enabled),
                    render_optional_u8(exit.vtimer_gic_priority_mask),
                    render_optional_u8(exit.vtimer_gic_running_priority),
                    render_optional_u8(exit.vtimer_gic_priority_threshold),
                    render_optional_gic_intid(exit.vtimer_gic_pending_intid),
                    render_optional_status_name(exit.vtimer_pending_irq_set_status),
                    render_optional_status_name(exit.vtimer_unmask_status),
                    exit.handled
                ));
                if exit.stage1_descriptor_samples_after_exit.is_empty() {
                    output.push_str("  Stage-1 descriptor samples: none\n");
                } else {
                    output.push_str("  Stage-1 descriptor samples:\n");
                    for sample in &exit.stage1_descriptor_samples_after_exit {
                        output.push_str(&format!(
                            "  - label={}, va={:#x}, region={}, level={}, descriptor={}, kind={}, output={}, attr_index={}, ap={}, sh={}, af={}, pxn={}, uxn={}\n",
                            sample.label,
                            sample.virtual_address,
                            sample.region,
                            render_optional_u8(sample.level),
                            render_optional_u64(sample.descriptor),
                            sample.descriptor_kind,
                            render_optional_u64(sample.output_address),
                            render_optional_u8(sample.attr_index),
                            render_optional_u8(sample.access_permissions),
                            render_optional_u8(sample.shareability),
                            render_optional_bool(sample.access_flag),
                            render_optional_bool(sample.pxn),
                            render_optional_bool(sample.uxn),
                        ));
                    }
                }
                if exit.stage1_walk_entries_after_exit.is_empty() {
                    output.push_str("  Stage-1 walk entries: none\n");
                } else {
                    output.push_str("  Stage-1 walk entries:\n");
                    for entry in &exit.stage1_walk_entries_after_exit {
                        output.push_str(&format!(
                            "  - label={}, va={:#x}, region={}, level={}, table_ipa={:#x}, index={:#x}, entry_ipa={:#x}, descriptor={}, kind={}, next_table={}, output={}, attr_index={}, ap={}, sh={}, af={}, pxn={}, uxn={}\n",
                            entry.label,
                            entry.virtual_address,
                            entry.region,
                            entry.level,
                            entry.table_ipa,
                            entry.index,
                            entry.entry_ipa,
                            render_optional_u64(entry.descriptor),
                            entry.descriptor_kind,
                            render_optional_u64(entry.next_table_ipa),
                            render_optional_u64(entry.output_address),
                            render_optional_u8(entry.attr_index),
                            render_optional_u8(entry.access_permissions),
                            render_optional_u8(entry.shareability),
                            render_optional_bool(entry.access_flag),
                            render_optional_bool(entry.pxn),
                            render_optional_bool(entry.uxn),
                        ));
                    }
                }
                if exit.stage1_executable_candidates_after_exit.is_empty() {
                    output.push_str("  Stage-1 EL1-executable leaf candidates: none\n");
                } else {
                    output.push_str("  Stage-1 EL1-executable leaf candidates:\n");
                    for candidate in &exit.stage1_executable_candidates_after_exit {
                        output.push_str(&format!(
                            "  - va={:#x}, region={}, level={}, descriptor={:#x}, kind={}, output={}, span={}, vector_sync_va={}, vector_sync_pa={}, vector_sync_instruction={}, vector_sync_hint={}, vector_base_scan_scanned={}, vector_base_scan_suppressed={}, vector_base_scan_limit_reached={}, attr_index={}, ap={}, sh={}, af={}, pxn={}, uxn={}\n",
                            candidate.virtual_address,
                            candidate.region,
                            candidate.level,
                            candidate.descriptor,
                            candidate.descriptor_kind,
                            render_optional_u64(candidate.output_address),
                            render_optional_u64(candidate.span_bytes),
                            render_optional_u64(candidate.vector_sync_virtual_address),
                            render_optional_u64(candidate.vector_sync_physical_address),
                            render_optional_instruction_word(candidate.vector_sync_instruction_word),
                            candidate.vector_sync_instruction_hint,
                            candidate.vector_base_scan_scanned_count,
                            candidate.vector_base_scan_suppressed_count,
                            candidate.vector_base_scan_limit_reached,
                            candidate.attr_index,
                            candidate.access_permissions,
                            candidate.shareability,
                            candidate.access_flag,
                            candidate.pxn,
                            candidate.uxn,
                        ));
                        if let Some(recommendation) = &candidate.recommended_vector_base_candidate {
                            output.push_str(&format!(
                                "    Recommended vector base: base_va={:#x}, base_pa={}, current_el_spx_sync={}, current_el_spx_hint={}, reason={}\n",
                                recommendation.base_virtual_address,
                                render_optional_u64(recommendation.base_physical_address),
                                render_optional_instruction_word(
                                    recommendation.current_el_spx_sync_instruction_word,
                                ),
                                recommendation.current_el_spx_sync_instruction_hint,
                                recommendation.reason,
                            ));
                        } else {
                            output.push_str("    Recommended vector base: none\n");
                        }
                        if candidate.vector_base_candidates.is_empty() {
                            output.push_str("    Vector base candidates: none\n");
                        } else {
                            output.push_str("    Vector base candidates:\n");
                            for vector_candidate in &candidate.vector_base_candidates {
                                output.push_str(&format!(
                                    "    - base_va={:#x}, base_pa={}, current_el_sp0_sync={}, current_el_spx_sync={}, current_el_spx_hint={}, lower_aarch64_sync={}, lower_aarch32_sync={}, populated_slots={}\n",
                                    vector_candidate.base_virtual_address,
                                    render_optional_u64(vector_candidate.base_physical_address),
                                    render_optional_instruction_word(
                                        vector_candidate.current_el_sp0_sync_instruction_word,
                                    ),
                                    render_optional_instruction_word(
                                        vector_candidate.current_el_spx_sync_instruction_word,
                                    ),
                                    vector_candidate.current_el_spx_sync_instruction_hint,
                                    render_optional_instruction_word(
                                        vector_candidate.lower_aarch64_sync_instruction_word,
                                    ),
                                    render_optional_instruction_word(
                                        vector_candidate.lower_aarch32_sync_instruction_word,
                                    ),
                                    vector_candidate.populated_slot_count,
                                ));
                            }
                        }
                    }
                }
            }
        }
        output.push_str(&format!(
            "vCPU destroy status name: {}\n",
            render_optional_status_name(self.vcpu_destroy_status)
        ));
        output.push_str(&format!(
            "Firmware unmap status name: {}\n",
            render_optional_status_name(self.firmware_unmap_status)
        ));
        output.push_str(&format!(
            "Vars unmap status name: {}\n",
            render_optional_status_name(self.vars_unmap_status)
        ));
        output.push_str(&format!(
            "Low firmware alias unmap status name: {}\n",
            render_optional_status_name(self.low_firmware_alias_unmap_status)
        ));
        output.push_str(&format!(
            "Low vars alias unmap status name: {}\n",
            render_optional_status_name(self.low_vars_alias_unmap_status)
        ));
        output.push_str(&format!(
            "Guest RAM unmap status name: {}\n",
            render_optional_status_name(self.guest_ram_unmap_status)
        ));
        output.push_str(&format!(
            "Firmware deallocate status name: {}\n",
            render_optional_status_name(self.firmware_deallocate_status)
        ));
        output.push_str(&format!(
            "Vars deallocate status name: {}\n",
            render_optional_status_name(self.vars_deallocate_status)
        ));
        output.push_str(&format!(
            "Guest RAM deallocate status name: {}\n",
            render_optional_status_name(self.guest_ram_deallocate_status)
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UefiFirmwareFileVerification {
    pub(crate) bytes: u64,
    pub(crate) volume: UefiFirmwareVolumeMetadata,
}

pub fn probe_windows_11_arm_uefi_firmware_handoff(
    options: WindowsArmUefiFirmwareHandoffOptions,
) -> WindowsArmUefiFirmwareHandoffProbe {
    let mut blockers = Vec::new();
    let mut firmware_bytes = None;
    let mut firmware_volume = None;
    let mut firmware_verified = false;
    let mut vars_template_bytes = None;
    let mut vars_template_verified = false;
    let mut vars_bytes = None;
    let mut vars_created = false;
    let mut vars_reopened_for_verification = false;
    let mut vars_volume = None;
    let mut vars_verified = false;

    match verify_uefi_firmware_file(&options.firmware_path, WINDOWS_ARM_UEFI_SLOT_BYTES) {
        Ok(verification) => {
            firmware_bytes = Some(verification.bytes);
            firmware_volume = Some(verification.volume);
            firmware_verified = true;
        }
        Err(error) => blockers.push(format!("firmware verification failed: {error}")),
    }

    if let Some(template_path) = &options.vars_template_path {
        match verify_uefi_firmware_file(template_path, WINDOWS_ARM_UEFI_SLOT_BYTES) {
            Ok(verification) => {
                vars_template_bytes = Some(verification.bytes);
                vars_template_verified = true;
            }
            Err(error) => blockers.push(format!("vars template verification failed: {error}")),
        }
    }

    if options.create_vars {
        match (&options.vars_template_path, &options.vars_path) {
            (Some(template_path), Some(vars_path)) => {
                if vars_path.exists() {
                    blockers.push(format!(
                        "vars path already exists; refusing to overwrite {}",
                        vars_path.display()
                    ));
                } else if vars_template_verified {
                    match copy_uefi_vars_template(template_path, vars_path) {
                        Ok(()) => {
                            vars_created = true;
                            match verify_uefi_firmware_file(vars_path, WINDOWS_ARM_UEFI_SLOT_BYTES)
                            {
                                Ok(verification) => {
                                    vars_bytes = Some(verification.bytes);
                                    vars_volume = Some(verification.volume);
                                    vars_reopened_for_verification = true;
                                    vars_verified = true;
                                }
                                Err(error) => blockers.push(format!(
                                    "created vars store verification failed: {error}"
                                )),
                            }
                        }
                        Err(error) => blockers.push(format!("vars creation failed: {error}")),
                    }
                }
            }
            (None, _) => blockers.push(
                "--vars-template is required with --create-vars for a mutable UEFI variable store"
                    .to_string(),
            ),
            (_, None) => blockers.push(
                "--vars is required with --create-vars for a mutable UEFI variable store"
                    .to_string(),
            ),
        }
    } else if let Some(vars_path) = &options.vars_path {
        match verify_uefi_firmware_file(vars_path, WINDOWS_ARM_UEFI_SLOT_BYTES) {
            Ok(verification) => {
                vars_bytes = Some(verification.bytes);
                vars_volume = Some(verification.volume);
                vars_reopened_for_verification = true;
                vars_verified = true;
            }
            Err(error) => blockers.push(format!("vars store verification failed: {error}")),
        }
    } else if options.vars_template_path.is_some() {
        blockers.push(
            "vars template was verified, but no mutable --vars path was supplied".to_string(),
        );
    } else {
        blockers.push("UEFI variable store is required for Windows firmware handoff".to_string());
    }

    let planned_reset_vector_ipa =
        (firmware_verified && vars_verified).then_some(WINDOWS_ARM_UEFI_CODE_IPA);

    WindowsArmUefiFirmwareHandoffProbe {
        firmware_path: options.firmware_path,
        firmware_bytes,
        firmware_slot_ipa: WINDOWS_ARM_UEFI_CODE_IPA,
        firmware_slot_bytes: WINDOWS_ARM_UEFI_SLOT_BYTES,
        firmware_volume,
        firmware_verified,
        vars_template_path: options.vars_template_path,
        vars_template_bytes,
        vars_template_verified,
        vars_path: options.vars_path,
        vars_bytes,
        vars_slot_ipa: WINDOWS_ARM_UEFI_VARS_IPA,
        vars_slot_bytes: WINDOWS_ARM_UEFI_SLOT_BYTES,
        vars_created,
        vars_reopened_for_verification,
        vars_volume,
        vars_verified,
        planned_reset_vector_ipa,
        blockers,
    }
}

pub fn probe_windows_11_arm_uefi_pflash_map(
    options: WindowsArmUefiPflashMapOptions,
) -> WindowsArmUefiPflashMapProbe {
    let handoff =
        probe_windows_11_arm_uefi_firmware_handoff(WindowsArmUefiFirmwareHandoffOptions {
            firmware_path: options.firmware_path,
            vars_template_path: options.vars_template_path,
            vars_path: options.vars_path,
            create_vars: options.create_vars,
        });
    let mut blockers = handoff.blockers.clone();

    let firmware_slot = if handoff.firmware_verified {
        match load_uefi_pflash_slot(
            "code",
            &handoff.firmware_path,
            WINDOWS_ARM_UEFI_CODE_IPA,
            WINDOWS_ARM_UEFI_SLOT_BYTES,
            false,
        ) {
            Ok(slot) => Some(slot),
            Err(error) => {
                blockers.push(format!("firmware pflash load failed: {error}"));
                None
            }
        }
    } else {
        None
    };

    let vars_slot = if handoff.vars_verified {
        match &handoff.vars_path {
            Some(vars_path) => match load_uefi_pflash_slot(
                "vars",
                vars_path,
                WINDOWS_ARM_UEFI_VARS_IPA,
                WINDOWS_ARM_UEFI_SLOT_BYTES,
                true,
            ) {
                Ok(slot) => Some(slot),
                Err(error) => {
                    blockers.push(format!("vars pflash load failed: {error}"));
                    None
                }
            },
            None => {
                blockers.push("verified vars store has no path for pflash mapping".to_string());
                None
            }
        }
    } else {
        None
    };

    let firmware_slot_loaded = firmware_slot
        .as_ref()
        .is_some_and(|slot| slot.prefix_verified && slot.padding_zeroed);
    let vars_slot_loaded = vars_slot
        .as_ref()
        .is_some_and(|slot| slot.prefix_verified && slot.padding_zeroed);

    let pflash_slots_non_overlapping = match (&firmware_slot, &vars_slot) {
        (Some(firmware_slot), Some(vars_slot)) => {
            firmware_slot.ipa_start == WINDOWS_ARM_UEFI_CODE_IPA
                && firmware_slot.ipa_end_exclusive() == WINDOWS_ARM_UEFI_VARS_IPA
                && vars_slot.ipa_start == WINDOWS_ARM_UEFI_VARS_IPA
                && vars_slot.ipa_end_exclusive() == WINDOWS_ARM_DEVICE_MMIO_IPA
                && !ipa_ranges_overlap(
                    firmware_slot.ipa_start,
                    firmware_slot.slot_bytes,
                    vars_slot.ipa_start,
                    vars_slot.slot_bytes,
                )
        }
        _ => false,
    };
    let guest_ram_overlap_verified = [&firmware_slot, &vars_slot]
        .into_iter()
        .flatten()
        .all(|slot| slot.ipa_end_exclusive() <= WINDOWS_ARM_GUEST_RAM_IPA);
    let device_mmio_overlap_verified =
        [&firmware_slot, &vars_slot]
            .into_iter()
            .flatten()
            .all(|slot| {
                !ipa_ranges_overlap(
                    slot.ipa_start,
                    slot.slot_bytes,
                    WINDOWS_ARM_DEVICE_MMIO_IPA,
                    WINDOWS_ARM_DEVICE_MMIO_BYTES,
                )
            });
    let pflash_map_verified = firmware_slot_loaded
        && vars_slot_loaded
        && pflash_slots_non_overlapping
        && guest_ram_overlap_verified
        && device_mmio_overlap_verified;

    if (firmware_slot_loaded || vars_slot_loaded) && !pflash_slots_non_overlapping {
        blockers.push("pflash code/vars IPA range verification failed".to_string());
    }
    if (firmware_slot_loaded || vars_slot_loaded) && !guest_ram_overlap_verified {
        blockers.push("pflash slots overlap the planned guest RAM window".to_string());
    }
    if (firmware_slot_loaded || vars_slot_loaded) && !device_mmio_overlap_verified {
        blockers.push("pflash slots overlap the planned device MMIO window".to_string());
    }

    WindowsArmUefiPflashMapProbe {
        firmware_path: handoff.firmware_path,
        vars_path: handoff.vars_path,
        vars_created: handoff.vars_created,
        firmware_verified: handoff.firmware_verified,
        vars_verified: handoff.vars_verified,
        firmware_slot,
        vars_slot,
        pflash_region_start: WINDOWS_ARM_UEFI_CODE_IPA,
        pflash_region_bytes: WINDOWS_ARM_UEFI_PFLASH_BYTES,
        pflash_slots_non_overlapping,
        guest_ram_overlap_verified,
        device_mmio_overlap_verified,
        pflash_map_verified,
        planned_reset_vector_ipa: pflash_map_verified.then_some(WINDOWS_ARM_UEFI_CODE_IPA),
        blockers,
    }
}

pub fn probe_windows_11_arm_uefi_pflash_hvf_map(
    options: WindowsArmUefiPflashMapOptions,
    allow_map: bool,
) -> WindowsArmUefiPflashHvfMapProbe {
    let pflash_map = probe_windows_11_arm_uefi_pflash_map(options);
    let host = query_hvf_host_capabilities();
    platform::probe_windows_11_arm_uefi_pflash_hvf_map(allow_map, pflash_map, host)
}

pub fn probe_windows_11_arm_uefi_reset_vector_entry(
    options: WindowsArmUefiPflashMapOptions,
    allow_entry: bool,
) -> WindowsArmUefiResetVectorEntryProbe {
    let pflash_map = probe_windows_11_arm_uefi_pflash_map(options);
    let host = query_hvf_host_capabilities();
    platform::probe_windows_11_arm_uefi_reset_vector_entry(allow_entry, pflash_map, host)
}

pub fn probe_windows_11_arm_uefi_firmware_run_loop(
    options: WindowsArmUefiFirmwareRunLoopOptions,
) -> WindowsArmUefiFirmwareRunLoopProbe {
    let WindowsArmUefiFirmwareRunLoopOptions { pflash, execution } = options;
    let pflash_map = probe_windows_11_arm_uefi_pflash_map(pflash);
    let host = query_hvf_host_capabilities();
    platform::probe_windows_11_arm_uefi_firmware_run_loop(execution, pflash_map, host)
}

pub fn probe_windows_11_arm_uefi_firmware_device_discovery(
    options: WindowsArmUefiFirmwareRunLoopOptions,
) -> WindowsArmUefiFirmwareDeviceDiscoveryProbe {
    let mut options = options;
    options.execution.map_low_pflash_alias = true;
    options.execution.repair_low_vector_diagnostic_page = true;
    options.execution.continue_after_low_vector_repair = true;
    options.execution.wire_interrupt_timer = true;
    options.execution.stop_at_first_post_repair_device_boundary = true;
    WindowsArmUefiFirmwareDeviceDiscoveryProbe {
        run_loop: probe_windows_11_arm_uefi_firmware_run_loop(options),
    }
}

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

pub fn probe_windows_11_arm_platform_description(
    options: WindowsArmPlatformDescriptionOptions,
) -> WindowsArmPlatformDescriptionProbe {
    let fdt_blob = build_windows_arm_platform_fdt_blob(&options);
    let summary = inspect_windows_arm_platform_fdt_blob(&fdt_blob);
    let device_mmio_end_ipa =
        WINDOWS_ARM_DEVICE_MMIO_IPA.saturating_add(WINDOWS_ARM_DEVICE_MMIO_BYTES);
    let mmio_nodes = vec![
        WindowsArmFdtMmioNodeCheck {
            label: "PL011",
            node_name: "serial@10000000",
            base_ipa: summary.pl011.map(|range| range.base_ipa),
            bytes: summary.pl011.map(|range| range.bytes),
            inside_device_window: summary.pl011.is_some_and(fdt_range_inside_device_window),
        },
        WindowsArmFdtMmioNodeCheck {
            label: "PL031",
            node_name: "rtc@10001000",
            base_ipa: summary.pl031.map(|range| range.base_ipa),
            bytes: summary.pl031.map(|range| range.bytes),
            inside_device_window: summary.pl031.is_some_and(fdt_range_inside_device_window),
        },
        WindowsArmFdtMmioNodeCheck {
            label: "VirtIO-MMIO installer ISO",
            node_name: "virtio_mmio@10002000",
            base_ipa: summary.virtio_installer_iso.map(|range| range.base_ipa),
            bytes: summary.virtio_installer_iso.map(|range| range.bytes),
            inside_device_window: summary
                .virtio_installer_iso
                .is_some_and(fdt_range_inside_device_window),
        },
        WindowsArmFdtMmioNodeCheck {
            label: "VirtIO-MMIO target disk",
            node_name: "virtio_mmio@10003000",
            base_ipa: summary.virtio_target_disk.map(|range| range.base_ipa),
            bytes: summary.virtio_target_disk.map(|range| range.bytes),
            inside_device_window: summary
                .virtio_target_disk
                .is_some_and(fdt_range_inside_device_window),
        },
    ];
    let mmio_nodes_inside_device_window = mmio_nodes.iter().all(|node| node.inside_device_window);
    let gic_nodes_inside_device_window = summary
        .gic_distributor
        .is_some_and(fdt_range_inside_device_window)
        && summary
            .gic_redistributor
            .is_some_and(fdt_range_inside_device_window);
    let arch_timer_node_present = !summary.arch_timer_interrupts.is_empty();
    let arch_timer_interrupt_count = summary.arch_timer_interrupts.len();
    let interrupt_nodes = vec![
        WindowsArmFdtInterruptCheck {
            label: "PL011",
            node_name: "serial@10000000",
            interrupt_type: summary
                .pl011_interrupt
                .map(|interrupt| interrupt.interrupt_type),
            interrupt_number: summary
                .pl011_interrupt
                .map(|interrupt| interrupt.interrupt_number),
            trigger: summary.pl011_interrupt.map(|interrupt| interrupt.trigger),
            described: summary.pl011_interrupt.is_some(),
        },
        WindowsArmFdtInterruptCheck {
            label: "PL031",
            node_name: "rtc@10001000",
            interrupt_type: summary
                .pl031_interrupt
                .map(|interrupt| interrupt.interrupt_type),
            interrupt_number: summary
                .pl031_interrupt
                .map(|interrupt| interrupt.interrupt_number),
            trigger: summary.pl031_interrupt.map(|interrupt| interrupt.trigger),
            described: summary.pl031_interrupt.is_some(),
        },
        WindowsArmFdtInterruptCheck {
            label: "VirtIO-MMIO installer ISO",
            node_name: "virtio_mmio@10002000",
            interrupt_type: summary
                .virtio_installer_iso_interrupt
                .map(|interrupt| interrupt.interrupt_type),
            interrupt_number: summary
                .virtio_installer_iso_interrupt
                .map(|interrupt| interrupt.interrupt_number),
            trigger: summary
                .virtio_installer_iso_interrupt
                .map(|interrupt| interrupt.trigger),
            described: summary.virtio_installer_iso_interrupt.is_some(),
        },
        WindowsArmFdtInterruptCheck {
            label: "VirtIO-MMIO target disk",
            node_name: "virtio_mmio@10003000",
            interrupt_type: summary
                .virtio_target_disk_interrupt
                .map(|interrupt| interrupt.interrupt_type),
            interrupt_number: summary
                .virtio_target_disk_interrupt
                .map(|interrupt| interrupt.interrupt_number),
            trigger: summary
                .virtio_target_disk_interrupt
                .map(|interrupt| interrupt.trigger),
            described: summary.virtio_target_disk_interrupt.is_some(),
        },
    ];
    let interrupt_nodes_described = interrupt_nodes.iter().all(|node| node.described);
    let memory_node_at_guest_ram_base =
        summary.memory_node_base_ipa == Some(WINDOWS_ARM_GUEST_RAM_IPA);
    let cpu_count_verified = summary.cpu_count == options.vcpu_count;
    let mut blockers = summary.blockers;

    if options.guest_ram_bytes == 0 {
        blockers.push("guest RAM FDT reg size must be non-zero".to_string());
    }
    if options.vcpu_count == 0 {
        blockers.push("FDT CPU count must be non-zero for Windows Arm".to_string());
    }
    if summary.fdt_magic != FDT_MAGIC {
        blockers.push("FDT header magic did not match 0xd00dfeed".to_string());
    }
    if !memory_node_at_guest_ram_base {
        blockers.push("FDT memory node is not rooted at the Windows Arm guest RAM IPA".to_string());
    }
    if !cpu_count_verified {
        blockers.push("FDT CPU node count does not match requested vCPU count".to_string());
    }
    if !mmio_nodes_inside_device_window {
        blockers.push(
            "FDT PL011/PL031/VirtIO-MMIO installer ISO/target disk nodes are not fully inside the Windows device window"
                .to_string(),
        );
    }
    if summary.root_interrupt_parent != Some(WINDOWS_ARM_GIC_PHANDLE) {
        blockers.push("FDT root interrupt-parent does not point at the GIC phandle".to_string());
    }
    if summary.gic_phandle != Some(WINDOWS_ARM_GIC_PHANDLE) || !summary.gic_interrupt_controller {
        blockers.push("FDT GICv3 interrupt-controller node is incomplete".to_string());
    }
    if !gic_nodes_inside_device_window {
        blockers.push(
            "FDT GIC distributor/redistributor nodes are not fully inside the Windows device window"
                .to_string(),
        );
    }
    if arch_timer_interrupt_count != 4 {
        blockers.push("FDT ARM arch timer must describe four timer interrupts".to_string());
    }
    if !interrupt_nodes_described {
        blockers
            .push("FDT PL011/PL031/VirtIO-MMIO interrupt properties are incomplete".to_string());
    }

    WindowsArmPlatformDescriptionProbe {
        qemu_used: false,
        apple_vz_used: false,
        hvf_entered: false,
        format: "FDT",
        fdt_blob_bytes: fdt_blob.len(),
        fdt_blob,
        fdt_magic: summary.fdt_magic,
        fdt_magic_verified: summary.fdt_magic == FDT_MAGIC,
        memory_node_base_ipa: summary.memory_node_base_ipa,
        memory_node_at_guest_ram_base,
        requested_cpu_count: options.vcpu_count,
        cpu_count: summary.cpu_count,
        cpu_count_verified,
        device_mmio_start_ipa: WINDOWS_ARM_DEVICE_MMIO_IPA,
        device_mmio_end_ipa,
        mmio_nodes,
        mmio_nodes_inside_device_window,
        root_interrupt_parent: summary.root_interrupt_parent,
        gic_phandle: summary.gic_phandle,
        gic_distributor_base_ipa: summary.gic_distributor.map(|range| range.base_ipa),
        gic_distributor_bytes: summary.gic_distributor.map(|range| range.bytes),
        gic_redistributor_base_ipa: summary.gic_redistributor.map(|range| range.base_ipa),
        gic_redistributor_bytes: summary.gic_redistributor.map(|range| range.bytes),
        gic_nodes_inside_device_window,
        arch_timer_node_present,
        arch_timer_interrupt_count,
        interrupt_nodes,
        interrupt_nodes_described,
        acpi_implemented: false,
        fw_cfg_used: false,
        gic_status: "described/not emulated",
        gic_emulated: false,
        blockers,
    }
}

pub(crate) fn windows_arm_firmware_block_devices(
    installer_iso_path: Option<PathBuf>,
    writable_target_disk_path: Option<PathBuf>,
) -> Vec<WindowsArmVirtioBlockDeviceMetadata> {
    let installer_capacity_sectors =
        windows_arm_block_capacity_sectors(installer_iso_path.as_ref());
    let target_capacity_sectors =
        windows_arm_block_capacity_sectors(writable_target_disk_path.as_ref());
    vec![
        WindowsArmVirtioBlockDeviceMetadata {
            role: "installer-iso",
            label: "VirtIO-MMIO installer ISO",
            node_name: "virtio_mmio@10002000",
            base_ipa: WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA,
            bytes: VIRTIO_MMIO_REGISTER_WINDOW_BYTES,
            read_only: true,
            backing_kind: "host-iso-readonly",
            backing_path: installer_iso_path,
            device_features: VIRTIO_BLK_F_RO,
            capacity_sectors: installer_capacity_sectors,
        },
        WindowsArmVirtioBlockDeviceMetadata {
            role: "target-disk",
            label: "VirtIO-MMIO target disk",
            node_name: "virtio_mmio@10003000",
            base_ipa: WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA,
            bytes: VIRTIO_MMIO_REGISTER_WINDOW_BYTES,
            read_only: false,
            backing_kind: "host-file-writable",
            backing_path: writable_target_disk_path,
            device_features: VIRTIO_MMIO_BLOCK_DEVICE_FEATURES_VALUE,
            capacity_sectors: target_capacity_sectors,
        },
    ]
}

pub(crate) fn windows_arm_block_capacity_sectors(path: Option<&PathBuf>) -> u64 {
    path.and_then(|path| std::fs::metadata(path).ok())
        .map(|metadata| metadata.len() / VIRTIO_BLOCK_SECTOR_BYTES)
        .unwrap_or(VIRTIO_MMIO_BLOCK_CAPACITY_SECTORS)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FdtRegRange {
    pub(crate) base_ipa: u64,
    pub(crate) bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FdtInterruptSpec {
    pub(crate) interrupt_type: u32,
    pub(crate) interrupt_number: u32,
    pub(crate) trigger: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WindowsArmPlatformFdtSummary {
    pub(crate) fdt_magic: u32,
    pub(crate) memory_node_base_ipa: Option<u64>,
    pub(crate) cpu_count: u8,
    pub(crate) root_interrupt_parent: Option<u32>,
    pub(crate) gic_phandle: Option<u32>,
    pub(crate) gic_interrupt_controller: bool,
    pub(crate) gic_distributor: Option<FdtRegRange>,
    pub(crate) gic_redistributor: Option<FdtRegRange>,
    pub(crate) arch_timer_interrupts: Vec<FdtInterruptSpec>,
    pub(crate) pl011: Option<FdtRegRange>,
    pub(crate) pl011_interrupt: Option<FdtInterruptSpec>,
    pub(crate) pl031: Option<FdtRegRange>,
    pub(crate) pl031_interrupt: Option<FdtInterruptSpec>,
    pub(crate) virtio_installer_iso: Option<FdtRegRange>,
    pub(crate) virtio_installer_iso_interrupt: Option<FdtInterruptSpec>,
    pub(crate) virtio_target_disk: Option<FdtRegRange>,
    pub(crate) virtio_target_disk_interrupt: Option<FdtInterruptSpec>,
    pub(crate) blockers: Vec<String>,
}

#[derive(Default)]
pub(crate) struct FdtBlobBuilder {
    pub(crate) structure: Vec<u8>,
    pub(crate) strings: Vec<u8>,
}

impl FdtBlobBuilder {
    pub(crate) fn begin_node(&mut self, name: &str) {
        push_be_u32(&mut self.structure, FDT_BEGIN_NODE);
        self.structure.extend_from_slice(name.as_bytes());
        self.structure.push(0);
        pad_to_4(&mut self.structure);
    }

    pub(crate) fn end_node(&mut self) {
        push_be_u32(&mut self.structure, FDT_END_NODE);
    }

    pub(crate) fn prop_raw(&mut self, name: &str, data: &[u8]) {
        let name_offset = self.add_string(name);
        push_be_u32(&mut self.structure, FDT_PROP);
        push_be_u32(&mut self.structure, data.len() as u32);
        push_be_u32(&mut self.structure, name_offset);
        self.structure.extend_from_slice(data);
        pad_to_4(&mut self.structure);
    }

    pub(crate) fn prop_u32(&mut self, name: &str, value: u32) {
        self.prop_raw(name, &value.to_be_bytes());
    }

    pub(crate) fn prop_empty(&mut self, name: &str) {
        self.prop_raw(name, &[]);
    }

    pub(crate) fn prop_u32_list(&mut self, name: &str, values: &[u32]) {
        let mut data = Vec::with_capacity(values.len() * 4);
        for value in values {
            data.extend_from_slice(&value.to_be_bytes());
        }
        self.prop_raw(name, &data);
    }

    pub(crate) fn prop_string(&mut self, name: &str, value: &str) {
        let mut data = Vec::with_capacity(value.len() + 1);
        data.extend_from_slice(value.as_bytes());
        data.push(0);
        self.prop_raw(name, &data);
    }

    pub(crate) fn prop_string_list(&mut self, name: &str, values: &[&str]) {
        let mut data = Vec::new();
        for value in values {
            data.extend_from_slice(value.as_bytes());
            data.push(0);
        }
        self.prop_raw(name, &data);
    }

    pub(crate) fn prop_reg64(&mut self, base_ipa: u64, bytes: u64) {
        let mut data = Vec::with_capacity(16);
        data.extend_from_slice(&base_ipa.to_be_bytes());
        data.extend_from_slice(&bytes.to_be_bytes());
        self.prop_raw("reg", &data);
    }

    pub(crate) fn prop_reg64_pairs(&mut self, ranges: &[(u64, u64)]) {
        let mut data = Vec::with_capacity(ranges.len() * 16);
        for (base_ipa, bytes) in ranges {
            data.extend_from_slice(&base_ipa.to_be_bytes());
            data.extend_from_slice(&bytes.to_be_bytes());
        }
        self.prop_raw("reg", &data);
    }

    pub(crate) fn prop_gic_interrupt(
        &mut self,
        interrupt_type: u32,
        interrupt_number: u32,
        trigger: u32,
    ) {
        self.prop_u32_list("interrupts", &[interrupt_type, interrupt_number, trigger]);
    }

    pub(crate) fn add_string(&mut self, name: &str) -> u32 {
        let offset = self.strings.len() as u32;
        self.strings.extend_from_slice(name.as_bytes());
        self.strings.push(0);
        offset
    }

    pub(crate) fn finish(mut self) -> Vec<u8> {
        push_be_u32(&mut self.structure, FDT_END);
        pad_to_4(&mut self.structure);

        let header_bytes = 40_u32;
        let mem_rsvmap_bytes = 16_u32;
        let off_mem_rsvmap = header_bytes;
        let off_dt_struct = off_mem_rsvmap + mem_rsvmap_bytes;
        let off_dt_strings = off_dt_struct + self.structure.len() as u32;
        let totalsize = off_dt_strings + self.strings.len() as u32;

        let mut blob = Vec::with_capacity(totalsize as usize);
        push_be_u32(&mut blob, FDT_MAGIC);
        push_be_u32(&mut blob, totalsize);
        push_be_u32(&mut blob, off_dt_struct);
        push_be_u32(&mut blob, off_dt_strings);
        push_be_u32(&mut blob, off_mem_rsvmap);
        push_be_u32(&mut blob, 17);
        push_be_u32(&mut blob, 16);
        push_be_u32(&mut blob, 0);
        push_be_u32(&mut blob, self.strings.len() as u32);
        push_be_u32(&mut blob, self.structure.len() as u32);
        push_be_u64(&mut blob, 0);
        push_be_u64(&mut blob, 0);
        blob.extend_from_slice(&self.structure);
        blob.extend_from_slice(&self.strings);
        blob
    }
}

pub(crate) fn build_windows_arm_platform_fdt_blob(
    options: &WindowsArmPlatformDescriptionOptions,
) -> Vec<u8> {
    let mut builder = FdtBlobBuilder::default();

    builder.begin_node("");
    builder.prop_string("compatible", "bridgevm,windows-arm-hvf");
    builder.prop_string("model", "BridgeVM Windows 11 Arm HVF");
    builder.prop_u32("#address-cells", 2);
    builder.prop_u32("#size-cells", 2);
    builder.prop_u32("interrupt-parent", WINDOWS_ARM_GIC_PHANDLE);

    builder.begin_node("chosen");
    builder.end_node();

    builder.begin_node(&format!("memory@{:x}", WINDOWS_ARM_GUEST_RAM_IPA));
    builder.prop_string("device_type", "memory");
    builder.prop_reg64(WINDOWS_ARM_GUEST_RAM_IPA, options.guest_ram_bytes);
    builder.end_node();

    builder.begin_node("cpus");
    builder.prop_u32("#address-cells", 1);
    builder.prop_u32("#size-cells", 0);
    for cpu_index in 0..options.vcpu_count {
        builder.begin_node(&format!("cpu@{cpu_index:x}"));
        builder.prop_string("device_type", "cpu");
        builder.prop_string("compatible", "arm,arm-v8");
        builder.prop_u32("reg", u32::from(cpu_index));
        builder.end_node();
    }
    builder.end_node();

    builder.begin_node("timer");
    builder.prop_string("compatible", "arm,armv8-timer");
    builder.prop_u32_list(
        "interrupts",
        &[
            GIC_PPI,
            13,
            IRQ_TYPE_LEVEL_HIGH,
            GIC_PPI,
            14,
            IRQ_TYPE_LEVEL_HIGH,
            GIC_PPI,
            11,
            IRQ_TYPE_LEVEL_HIGH,
            GIC_PPI,
            10,
            IRQ_TYPE_LEVEL_HIGH,
        ],
    );
    builder.prop_empty("always-on");
    builder.end_node();

    builder.begin_node("intc@10010000");
    builder.prop_string("compatible", "arm,gic-v3");
    builder.prop_empty("interrupt-controller");
    builder.prop_u32("#interrupt-cells", 3);
    builder.prop_u32("#address-cells", 2);
    builder.prop_u32("#size-cells", 2);
    builder.prop_u32("phandle", WINDOWS_ARM_GIC_PHANDLE);
    builder.prop_reg64_pairs(&[
        (
            WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA,
            WINDOWS_ARM_GIC_DISTRIBUTOR_BYTES,
        ),
        (
            WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA,
            windows_arm_gic_redistributor_fdt_bytes(options.vcpu_count),
        ),
    ]);
    builder.end_node();

    builder.begin_node("serial@10000000");
    builder.prop_string_list("compatible", &["arm,pl011", "arm,primecell"]);
    builder.prop_reg64(WINDOWS_ARM_PL011_MMIO_IPA, PL011_REGISTER_WINDOW_BYTES);
    builder.prop_gic_interrupt(GIC_SPI, WINDOWS_ARM_PL011_SPI, IRQ_TYPE_LEVEL_HIGH);
    builder.end_node();

    builder.begin_node("rtc@10001000");
    builder.prop_string_list("compatible", &["arm,pl031", "arm,primecell"]);
    builder.prop_reg64(WINDOWS_ARM_PL031_MMIO_IPA, PL031_REGISTER_WINDOW_BYTES);
    builder.prop_gic_interrupt(GIC_SPI, WINDOWS_ARM_PL031_SPI, IRQ_TYPE_LEVEL_HIGH);
    builder.end_node();

    builder.begin_node("virtio_mmio@10002000");
    builder.prop_string("compatible", "virtio,mmio");
    builder.prop_reg64(
        WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA,
        VIRTIO_MMIO_REGISTER_WINDOW_BYTES,
    );
    builder.prop_gic_interrupt(
        GIC_SPI,
        WINDOWS_ARM_VIRTIO_INSTALLER_ISO_SPI,
        IRQ_TYPE_LEVEL_HIGH,
    );
    builder.end_node();

    builder.begin_node("virtio_mmio@10003000");
    builder.prop_string("compatible", "virtio,mmio");
    builder.prop_reg64(
        WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA,
        VIRTIO_MMIO_REGISTER_WINDOW_BYTES,
    );
    builder.prop_gic_interrupt(
        GIC_SPI,
        WINDOWS_ARM_VIRTIO_TARGET_DISK_SPI,
        IRQ_TYPE_LEVEL_HIGH,
    );
    builder.end_node();

    builder.end_node();
    builder.finish()
}

pub(crate) fn build_windows_arm_firmware_run_loop_fdt_blob(guest_ram_bytes: u64) -> Vec<u8> {
    build_windows_arm_platform_fdt_blob(&WindowsArmPlatformDescriptionOptions {
        guest_ram_bytes,
        vcpu_count: WINDOWS_ARM_FIRMWARE_RUN_LOOP_FDT_VCPU_COUNT,
    })
}

pub(crate) fn windows_arm_firmware_run_loop_dtb_metadata(
    guest_ram_bytes: u64,
) -> (usize, u32, bool) {
    let blob = build_windows_arm_firmware_run_loop_fdt_blob(guest_ram_bytes);
    let magic = read_be_u32(&blob, 0).unwrap_or(0);
    (blob.len(), magic, magic == FDT_MAGIC)
}

pub(crate) fn inspect_windows_arm_platform_fdt_blob(blob: &[u8]) -> WindowsArmPlatformFdtSummary {
    let mut blockers = Vec::new();
    let fdt_magic = read_be_u32(blob, 0).unwrap_or(0);
    let totalsize = read_be_u32(blob, 4).unwrap_or(0) as usize;
    let off_dt_struct = read_be_u32(blob, 8).unwrap_or(0) as usize;
    let off_dt_strings = read_be_u32(blob, 12).unwrap_or(0) as usize;
    let size_dt_strings = read_be_u32(blob, 32).unwrap_or(0) as usize;
    let size_dt_struct = read_be_u32(blob, 36).unwrap_or(0) as usize;
    let mut summary = WindowsArmPlatformFdtSummary {
        fdt_magic,
        memory_node_base_ipa: None,
        cpu_count: 0,
        root_interrupt_parent: None,
        gic_phandle: None,
        gic_interrupt_controller: false,
        gic_distributor: None,
        gic_redistributor: None,
        arch_timer_interrupts: Vec::new(),
        pl011: None,
        pl011_interrupt: None,
        pl031: None,
        pl031_interrupt: None,
        virtio_installer_iso: None,
        virtio_installer_iso_interrupt: None,
        virtio_target_disk: None,
        virtio_target_disk_interrupt: None,
        blockers: Vec::new(),
    };

    if blob.len() < 40 {
        summary
            .blockers
            .push("FDT blob is shorter than the header".to_string());
        return summary;
    }
    if totalsize > blob.len() {
        blockers.push("FDT totalsize exceeds blob length".to_string());
    }
    let Some(struct_end) = off_dt_struct.checked_add(size_dt_struct) else {
        blockers.push("FDT structure block range overflowed".to_string());
        summary.blockers = blockers;
        return summary;
    };
    let Some(strings_end) = off_dt_strings.checked_add(size_dt_strings) else {
        blockers.push("FDT strings block range overflowed".to_string());
        summary.blockers = blockers;
        return summary;
    };
    if struct_end > blob.len() || strings_end > blob.len() {
        blockers.push("FDT block offsets exceed blob length".to_string());
        summary.blockers = blockers;
        return summary;
    }

    let structure = &blob[off_dt_struct..struct_end];
    let strings = &blob[off_dt_strings..strings_end];
    let mut offset = 0_usize;
    let mut path: Vec<String> = Vec::new();

    while offset + 4 <= structure.len() {
        let Some(token) = read_be_u32(structure, offset) else {
            blockers.push("FDT structure token read failed".to_string());
            break;
        };
        offset += 4;

        match token {
            FDT_BEGIN_NODE => {
                let Some((name, next_offset)) = read_fdt_node_name(structure, offset) else {
                    blockers.push("FDT node name read failed".to_string());
                    break;
                };
                offset = next_offset;
                if !name.is_empty() {
                    if path.len() == 1 && path[0] == "cpus" && name.starts_with("cpu@") {
                        summary.cpu_count = summary.cpu_count.saturating_add(1);
                    }
                    path.push(name);
                }
            }
            FDT_END_NODE => {
                let _ = path.pop();
            }
            FDT_PROP => {
                if offset + 8 > structure.len() {
                    blockers.push("FDT property header is truncated".to_string());
                    break;
                }
                let len = read_be_u32(structure, offset).unwrap_or(0) as usize;
                let name_offset = read_be_u32(structure, offset + 4).unwrap_or(0) as usize;
                offset += 8;
                let Some(data_end) = offset.checked_add(len) else {
                    blockers.push("FDT property data range overflowed".to_string());
                    break;
                };
                if data_end > structure.len() {
                    blockers.push("FDT property data is truncated".to_string());
                    break;
                }
                let data = &structure[offset..data_end];
                offset = align_up_to_4(data_end);
                let Some(name) = read_fdt_string(strings, name_offset) else {
                    blockers.push("FDT property name offset is invalid".to_string());
                    continue;
                };
                match name {
                    "reg" => record_windows_arm_fdt_reg(&path, data, &mut summary),
                    "interrupt-parent" if path.is_empty() => {
                        summary.root_interrupt_parent = read_fdt_u32(data);
                    }
                    "phandle" if path.last().is_some_and(|node| node == "intc@10010000") => {
                        summary.gic_phandle = read_fdt_u32(data);
                    }
                    "interrupt-controller"
                        if path.last().is_some_and(|node| node == "intc@10010000") =>
                    {
                        summary.gic_interrupt_controller = true;
                    }
                    "interrupts" => {
                        record_windows_arm_fdt_interrupts(&path, data, &mut summary);
                    }
                    _ => {}
                }
            }
            FDT_END => break,
            _ => {
                blockers.push(format!("unsupported FDT structure token {token:#x}"));
                break;
            }
        }
    }

    if summary.memory_node_base_ipa.is_none() {
        blockers.push("FDT memory node reg was not found".to_string());
    }
    if summary.pl011.is_none() {
        blockers.push("FDT PL011 node reg was not found".to_string());
    }
    if summary.pl031.is_none() {
        blockers.push("FDT PL031 node reg was not found".to_string());
    }
    if summary.virtio_installer_iso.is_none() {
        blockers.push("FDT VirtIO-MMIO installer ISO node reg was not found".to_string());
    }
    if summary.virtio_target_disk.is_none() {
        blockers.push("FDT VirtIO-MMIO target disk node reg was not found".to_string());
    }
    summary.blockers = blockers;
    summary
}

pub(crate) fn record_windows_arm_fdt_reg(
    path: &[String],
    data: &[u8],
    summary: &mut WindowsArmPlatformFdtSummary,
) {
    let Some(node) = path.last().map(String::as_str) else {
        return;
    };
    if path.len() == 2 && path[0] == "cpus" && node.starts_with("cpu@") {
        return;
    }
    if node == "intc@10010000" {
        let ranges = read_fdt_reg64_pairs(data);
        summary.gic_distributor = ranges.first().copied();
        summary.gic_redistributor = ranges.get(1).copied();
        return;
    }
    let Some(range) = read_fdt_reg64(data) else {
        return;
    };

    match node {
        name if name.starts_with("memory@") => {
            summary.memory_node_base_ipa = Some(range.base_ipa);
        }
        "serial@10000000" => summary.pl011 = Some(range),
        "rtc@10001000" => summary.pl031 = Some(range),
        "virtio_mmio@10002000" => summary.virtio_installer_iso = Some(range),
        "virtio_mmio@10003000" => summary.virtio_target_disk = Some(range),
        _ => {}
    }
}

pub(crate) fn record_windows_arm_fdt_interrupts(
    path: &[String],
    data: &[u8],
    summary: &mut WindowsArmPlatformFdtSummary,
) {
    let Some(node) = path.last().map(String::as_str) else {
        return;
    };
    let interrupts = read_fdt_interrupts(data);
    match node {
        "timer" => summary.arch_timer_interrupts = interrupts,
        "serial@10000000" => summary.pl011_interrupt = interrupts.first().copied(),
        "rtc@10001000" => summary.pl031_interrupt = interrupts.first().copied(),
        "virtio_mmio@10002000" => {
            summary.virtio_installer_iso_interrupt = interrupts.first().copied();
        }
        "virtio_mmio@10003000" => {
            summary.virtio_target_disk_interrupt = interrupts.first().copied();
        }
        _ => {}
    }
}

pub(crate) fn fdt_range_inside_device_window(range: FdtRegRange) -> bool {
    let Some(end) = range.base_ipa.checked_add(range.bytes) else {
        return false;
    };
    range.base_ipa >= WINDOWS_ARM_DEVICE_MMIO_IPA
        && end <= WINDOWS_ARM_DEVICE_MMIO_IPA.saturating_add(WINDOWS_ARM_DEVICE_MMIO_BYTES)
}

pub(crate) fn read_fdt_reg64(data: &[u8]) -> Option<FdtRegRange> {
    Some(FdtRegRange {
        base_ipa: read_be_u64(data, 0)?,
        bytes: read_be_u64(data, 8)?,
    })
}

pub(crate) fn read_fdt_reg64_pairs(data: &[u8]) -> Vec<FdtRegRange> {
    let mut ranges = Vec::new();
    for chunk in data.chunks_exact(16) {
        if let Some(range) = read_fdt_reg64(chunk) {
            ranges.push(range);
        }
    }
    ranges
}

pub(crate) fn read_fdt_u32(data: &[u8]) -> Option<u32> {
    read_be_u32(data, 0)
}

pub(crate) fn read_fdt_interrupts(data: &[u8]) -> Vec<FdtInterruptSpec> {
    let mut interrupts = Vec::new();
    for chunk in data.chunks_exact(12) {
        if let (Some(interrupt_type), Some(interrupt_number), Some(trigger)) = (
            read_be_u32(chunk, 0),
            read_be_u32(chunk, 4),
            read_be_u32(chunk, 8),
        ) {
            interrupts.push(FdtInterruptSpec {
                interrupt_type,
                interrupt_number,
                trigger,
            });
        }
    }
    interrupts
}

pub(crate) fn read_fdt_node_name(data: &[u8], offset: usize) -> Option<(String, usize)> {
    let mut end = offset;
    while end < data.len() && data[end] != 0 {
        end += 1;
    }
    if end >= data.len() {
        return None;
    }
    let name = std::str::from_utf8(&data[offset..end]).ok()?.to_string();
    Some((name, align_up_to_4(end + 1)))
}

pub(crate) fn read_fdt_string(data: &[u8], offset: usize) -> Option<&str> {
    let mut end = offset;
    while end < data.len() && data[end] != 0 {
        end += 1;
    }
    if end >= data.len() {
        return None;
    }
    std::str::from_utf8(&data[offset..end]).ok()
}

pub(crate) fn read_be_u32(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

pub(crate) fn read_be_u64(data: &[u8], offset: usize) -> Option<u64> {
    let bytes = data.get(offset..offset.checked_add(8)?)?;
    Some(u64::from_be_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}

pub(crate) fn push_be_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_be_bytes());
}

pub(crate) fn push_be_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_be_bytes());
}

pub(crate) fn pad_to_4(output: &mut Vec<u8>) {
    while output.len() % 4 != 0 {
        output.push(0);
    }
}

pub(crate) fn align_up_to_4(value: usize) -> usize {
    (value + 3) & !3
}

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

pub(crate) fn verify_uefi_firmware_file(
    path: &PathBuf,
    slot_bytes: u64,
) -> Result<UefiFirmwareFileVerification, String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let bytes = file.metadata().map_err(|error| error.to_string())?.len();
    if bytes == 0 {
        return Err("file is empty".to_string());
    }
    if bytes > slot_bytes {
        return Err(format!(
            "file is larger than the planned pflash slot ({bytes:#x} > {slot_bytes:#x})"
        ));
    }
    let len: usize = bytes
        .try_into()
        .map_err(|_| "file is too large to inspect on this host".to_string())?;
    let mut contents = vec![0_u8; len];
    file.read_exact(&mut contents)
        .map_err(|error| error.to_string())?;
    let volume = detect_uefi_firmware_volume(&contents)?;
    Ok(UefiFirmwareFileVerification { bytes, volume })
}

pub(crate) fn load_uefi_pflash_slot(
    name: &'static str,
    path: &PathBuf,
    ipa_start: u64,
    slot_bytes: u64,
    writable: bool,
) -> Result<WindowsArmUefiPflashSlotMap, String> {
    let slot_len: usize = slot_bytes
        .try_into()
        .map_err(|_| "pflash slot is too large to allocate on this host".to_string())?;
    let source = media::read_bounded_file(path, slot_len).map_err(|error| error.to_string())?;
    if source.is_empty() {
        return Err("file is empty".to_string());
    }
    let source_bytes =
        u64::try_from(source.len()).map_err(|_| "file is too large to map".to_string())?;
    let mut slot = vec![0_u8; slot_len];
    slot[..source.len()].copy_from_slice(&source);
    let prefix_verified = slot[..source.len()] == source[..];
    let padding_zeroed = slot[source.len()..].iter().all(|byte| *byte == 0);

    Ok(WindowsArmUefiPflashSlotMap {
        name,
        path: path.clone(),
        ipa_start,
        slot_bytes,
        source_bytes,
        copied_bytes: source_bytes,
        zero_padding_bytes: slot_bytes - source_bytes,
        writable,
        prefix_verified,
        padding_zeroed,
    })
}

pub(crate) fn copy_uefi_vars_template(
    template_path: &PathBuf,
    vars_path: &PathBuf,
) -> Result<(), String> {
    std::fs::copy(template_path, vars_path).map_err(|error| error.to_string())?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(vars_path)
        .map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())
}

pub(crate) fn detect_uefi_firmware_volume(
    bytes: &[u8],
) -> Result<UefiFirmwareVolumeMetadata, String> {
    if bytes.len() < UEFI_FV_MIN_HEADER_BYTES {
        return Err("file is too small for a UEFI firmware volume header".to_string());
    }
    let search_end = bytes.len().min(64 * 1024);
    for signature_offset in UEFI_FV_SIGNATURE_OFFSET..search_end.saturating_sub(4) {
        if &bytes[signature_offset..signature_offset + 4] != UEFI_FV_SIGNATURE {
            continue;
        }
        let offset = signature_offset - UEFI_FV_SIGNATURE_OFFSET;
        if offset + UEFI_FV_MIN_HEADER_BYTES > bytes.len() {
            continue;
        }
        let length_bytes = u64_from_le(bytes, offset + UEFI_FV_LENGTH_OFFSET)?;
        let header_length = u16_from_le(bytes, offset + UEFI_FV_HEADER_LENGTH_OFFSET)?;
        let header_length_usize = usize::from(header_length);
        if header_length_usize < UEFI_FV_MIN_HEADER_BYTES {
            return Err("UEFI firmware volume header length is too small".to_string());
        }
        if header_length_usize % 2 != 0 {
            return Err("UEFI firmware volume header length is not 16-bit aligned".to_string());
        }
        if offset + header_length_usize > bytes.len() {
            return Err("UEFI firmware volume header extends past the file".to_string());
        }
        let length_usize: usize = length_bytes
            .try_into()
            .map_err(|_| "UEFI firmware volume length is too large to inspect".to_string())?;
        if length_usize < header_length_usize {
            return Err("UEFI firmware volume length is smaller than its header".to_string());
        }
        if offset + length_usize > bytes.len() {
            return Err("UEFI firmware volume length extends past the file".to_string());
        }
        let header = &bytes[offset..offset + header_length_usize];
        if uefi_checksum16(header) != 0 {
            return Err("UEFI firmware volume header checksum verification failed".to_string());
        }
        return Ok(UefiFirmwareVolumeMetadata {
            offset: offset as u64,
            length_bytes,
            header_length,
            checksum_verified: true,
        });
    }
    Err("UEFI firmware volume signature _FVH was not found".to_string())
}

pub(crate) fn render_uefi_volume_metadata(
    label: &str,
    volume: &Option<UefiFirmwareVolumeMetadata>,
    output: &mut String,
) {
    match volume {
        Some(volume) => {
            output.push_str(&format!("{label} detected: true\n"));
            output.push_str(&format!("{label} offset: {:#x}\n", volume.offset));
            output.push_str(&format!(
                "{label} length bytes: {:#x}\n",
                volume.length_bytes
            ));
            output.push_str(&format!(
                "{label} header length: {:#x}\n",
                volume.header_length
            ));
            output.push_str(&format!(
                "{label} checksum verified: {}\n",
                volume.checksum_verified
            ));
        }
        None => output.push_str(&format!("{label} detected: false\n")),
    }
}

pub(crate) fn render_uefi_pflash_slot(
    label: &str,
    slot: &Option<WindowsArmUefiPflashSlotMap>,
    output: &mut String,
) {
    match slot {
        Some(slot) => {
            output.push_str(&format!("{label} loaded: true\n"));
            output.push_str(&format!("{label} name: {}\n", slot.name));
            output.push_str(&format!("{label} path: {}\n", slot.path.display()));
            output.push_str(&format!(
                "{label} IPA range: {:#x}..{:#x}\n",
                slot.ipa_start,
                slot.ipa_end_exclusive()
            ));
            output.push_str(&format!("{label} slot bytes: {:#x}\n", slot.slot_bytes));
            output.push_str(&format!("{label} source bytes: {:#x}\n", slot.source_bytes));
            output.push_str(&format!("{label} copied bytes: {:#x}\n", slot.copied_bytes));
            output.push_str(&format!(
                "{label} zero padding bytes: {:#x}\n",
                slot.zero_padding_bytes
            ));
            output.push_str(&format!("{label} writable: {}\n", slot.writable));
            output.push_str(&format!(
                "{label} prefix verified: {}\n",
                slot.prefix_verified
            ));
            output.push_str(&format!(
                "{label} padding zeroed: {}\n",
                slot.padding_zeroed
            ));
        }
        None => output.push_str(&format!("{label} loaded: false\n")),
    }
}

pub(crate) fn ipa_ranges_overlap(
    left_start: u64,
    left_size: u64,
    right_start: u64,
    right_size: u64,
) -> bool {
    let left_end = left_start.saturating_add(left_size);
    let right_end = right_start.saturating_add(right_size);
    left_start < right_end && right_start < left_end
}

pub(crate) fn decode_gpt_partition_name(bytes: &[u8]) -> String {
    let mut units = Vec::new();
    for chunk in bytes.chunks_exact(2) {
        let unit = u16::from_le_bytes([chunk[0], chunk[1]]);
        if unit == 0 {
            break;
        }
        units.push(unit);
    }
    String::from_utf16_lossy(&units)
}

pub(crate) fn stable_guid_bytes(path: &Path, label: &str, disk_size_bytes: u64) -> [u8; 16] {
    let mut first = fnv1a64(0xcbf2_9ce4_8422_2325, label.as_bytes());
    first = fnv1a64(first, path.to_string_lossy().as_bytes());
    first = fnv1a64(first, &disk_size_bytes.to_le_bytes());
    let mut second = fnv1a64(0x8422_2325_cbf2_9ce4, path.to_string_lossy().as_bytes());
    second = fnv1a64(second, label.as_bytes());
    second = fnv1a64(second, &disk_size_bytes.to_be_bytes());
    let mut output = [0_u8; 16];
    output[0..8].copy_from_slice(&first.to_le_bytes());
    output[8..16].copy_from_slice(&second.to_le_bytes());
    output[6] = (output[6] & 0x0f) | 0x40;
    output[8] = (output[8] & 0x3f) | 0x80;
    output
}

pub(crate) fn fnv1a64(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

pub(crate) fn write_all_at(file: &mut File, offset: u64, bytes: &[u8]) -> Result<(), String> {
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| error.to_string())?;
    file.write_all(bytes).map_err(|error| error.to_string())
}

pub(crate) fn read_exact_at(file: &mut File, offset: u64, len: usize) -> Result<Vec<u8>, String> {
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| error.to_string())?;
    let mut bytes = vec![0_u8; len];
    file.read_exact(&mut bytes)
        .map_err(|error| error.to_string())?;
    Ok(bytes)
}

pub(crate) fn u32_from_le(bytes: &[u8], offset: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or_else(|| "u32 field out of range".to_string())?
            .try_into()
            .map_err(|_| "u32 field parse failed".to_string())?,
    ))
}

pub(crate) fn u16_from_le(bytes: &[u8], offset: usize) -> Result<u16, String> {
    Ok(u16::from_le_bytes(
        bytes
            .get(offset..offset + 2)
            .ok_or_else(|| "u16 field out of range".to_string())?
            .try_into()
            .map_err(|_| "u16 field parse failed".to_string())?,
    ))
}

pub(crate) fn u64_from_le(bytes: &[u8], offset: usize) -> Result<u64, String> {
    Ok(u64::from_le_bytes(
        bytes
            .get(offset..offset + 8)
            .ok_or_else(|| "u64 field out of range".to_string())?
            .try_into()
            .map_err(|_| "u64 field parse failed".to_string())?,
    ))
}

pub(crate) fn uefi_checksum16(bytes: &[u8]) -> u16 {
    let mut sum = 0_u16;
    for chunk in bytes.chunks_exact(2) {
        sum = sum.wrapping_add(u16::from_le_bytes([chunk[0], chunk[1]]));
    }
    sum
}

pub(crate) fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

pub(crate) fn append_low_vector_post_repair_exit_telemetry(
    output: &mut String,
    label: &str,
    telemetry: &LowVectorPostRepairExitTelemetry,
    kind_label: &str,
    context_exit: Option<&WindowsArmUefiFirmwareRunLoopExit>,
) {
    output.push_str(&format!("{label} observed: {}\n", telemetry.observed));
    output.push_str(&format!(
        "{label}: {}\n",
        render_optional_intid(telemetry.index)
    ));
    output.push_str(&format!(
        "{label} reason name: {}\n",
        render_optional_exit_reason_name(telemetry.reason)
    ));
    output.push_str(&format!(
        "{label} classification: {}\n",
        telemetry.diagnosis
    ));
    output.push_str(&format!(
        "{label} PC: {}\n",
        render_optional_u64(telemetry.pc)
    ));
    output.push_str(&format!(
        "{label} instruction: {}\n",
        render_optional_instruction_word(
            context_exit.and_then(|exit| exit.instruction_word_after_exit)
        )
    ));
    output.push_str(&format!(
        "{label} instruction hint: {}\n",
        context_exit
            .map(|exit| exit.instruction_hint_after_exit)
            .unwrap_or("not observed")
    ));
    output.push_str(&format!(
        "{label} VBAR_EL1: {}\n",
        render_optional_u64(context_exit.and_then(|exit| exit.vbar_el1_after_exit))
    ));
    output.push_str(&format!(
        "{label} ELR_EL1: {}\n",
        render_optional_u64(context_exit.and_then(|exit| exit.elr_el1_after_exit))
    ));
    output.push_str(&format!(
        "{label} ESR_EL1: {}\n",
        render_optional_u64(context_exit.and_then(|exit| exit.esr_el1_after_exit))
    ));
    output.push_str(&format!(
        "{label} FAR_EL1: {}\n",
        render_optional_u64(context_exit.and_then(|exit| exit.far_el1_after_exit))
    ));
    output.push_str(&format!(
        "{label} SPSR_EL1: {}\n",
        render_optional_u64(context_exit.and_then(|exit| exit.spsr_el1_after_exit))
    ));
    output.push_str(&format!("{label} access kind: {}\n", telemetry.access.kind));
    output.push_str(&format!(
        "{label} access direction: {}\n",
        telemetry.access.direction
    ));
    output.push_str(&format!(
        "{label} access address: {}\n",
        render_optional_u64(telemetry.access.address)
    ));
    output.push_str(&format!(
        "{label} access sysreg: {}\n",
        render_optional_u16_hex(telemetry.access.sysreg)
    ));
    output.push_str(&format!(
        "{label} access syndrome: {}\n",
        render_optional_u64(telemetry.access.syndrome)
    ));
    output.push_str(&format!("{kind_label}: {}\n", telemetry.interaction_kind));
}

pub(crate) fn append_low_vector_post_repair_unhandled_access_telemetry(
    output: &mut String,
    label: &str,
    telemetry: &LowVectorPostRepairUnhandledAccessTelemetry,
) {
    output.push_str(&format!("{label} observed: {}\n", telemetry.observed));
    output.push_str(&format!(
        "{label}: {}\n",
        render_optional_intid(telemetry.index)
    ));
    output.push_str(&format!(
        "{label} reason name: {}\n",
        render_optional_exit_reason_name(telemetry.reason)
    ));
    output.push_str(&format!(
        "{label} classification: {}\n",
        telemetry.diagnosis
    ));
    output.push_str(&format!(
        "{label} PC: {}\n",
        render_optional_u64(telemetry.pc)
    ));
    output.push_str(&format!(
        "{label} syndrome: {}\n",
        render_optional_u64(telemetry.syndrome)
    ));
    output.push_str(&format!("{label} kind: {}\n", telemetry.kind));
    output.push_str(&format!("{label} direction: {}\n", telemetry.access));
    output.push_str(&format!(
        "{label} register: {}\n",
        render_optional_u8(telemetry.register)
    ));
    output.push_str(&format!(
        "{label} value: {}\n",
        render_optional_u64(telemetry.value)
    ));
    output.push_str(&format!(
        "{label} handler result: {}\n",
        telemetry.handler_result
    ));
    output.push_str(&format!(
        "{label} MMIO IPA: {}\n",
        render_optional_u64(telemetry.mmio_ipa)
    ));
    output.push_str(&format!(
        "{label} MMIO width: {}\n",
        render_optional_u8(telemetry.mmio_width)
    ));
    output.push_str(&format!(
        "{label} MMIO device kind: {}\n",
        telemetry.mmio_device_kind
    ));
    output.push_str(&format!(
        "{label} sysreg: {}\n",
        render_optional_u16_hex(telemetry.sysreg)
    ));
    output.push_str(&format!("{label} sysreg name: {}\n", telemetry.sysreg_name));
    output.push_str(&format!(
        "{label} sysreg op0: {}\n",
        render_optional_u8(telemetry.sysreg_op0)
    ));
    output.push_str(&format!(
        "{label} sysreg op1: {}\n",
        render_optional_u8(telemetry.sysreg_op1)
    ));
    output.push_str(&format!(
        "{label} sysreg crn: {}\n",
        render_optional_u8(telemetry.sysreg_crn)
    ));
    output.push_str(&format!(
        "{label} sysreg crm: {}\n",
        render_optional_u8(telemetry.sysreg_crm)
    ));
    output.push_str(&format!(
        "{label} sysreg op2: {}\n",
        render_optional_u8(telemetry.sysreg_op2)
    ));
}

pub(crate) fn low_vector_post_repair_context_exit(
    exits: &[WindowsArmUefiFirmwareRunLoopExit],
    index: Option<u32>,
) -> Option<&WindowsArmUefiFirmwareRunLoopExit> {
    let index = index?;
    exits.iter().find(|exit| exit.index == index)
}

pub(crate) fn render_optional_u32(value: Option<u32>) -> String {
    value.map_or_else(|| "unknown".to_string(), |value| value.to_string())
}

pub(crate) fn render_optional_u16_hex(value: Option<u16>) -> String {
    value.map_or_else(|| "not observed".to_string(), |value| format!("{value:#x}"))
}

pub(crate) fn render_optional_intid(value: Option<u32>) -> String {
    value.map_or_else(|| "not observed".to_string(), |value| value.to_string())
}

pub(crate) fn render_optional_gic_intid(value: Option<u32>) -> String {
    value.map_or_else(
        || "not observed".to_string(),
        |value| match value {
            GICV3_SPURIOUS_INTERRUPT_ID => "spurious".to_string(),
            value => value.to_string(),
        },
    )
}

pub(crate) fn render_optional_u64(value: Option<u64>) -> String {
    value.map_or_else(|| "not observed".to_string(), |value| format!("{value:#x}"))
}

pub(crate) fn render_optional_u8(value: Option<u8>) -> String {
    value.map_or_else(|| "not observed".to_string(), |value| format!("{value:#x}"))
}

pub(crate) fn render_optional_bool(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "true",
        Some(false) => "false",
        None => "not observed",
    }
}

pub(crate) fn render_optional_instruction_word(value: Option<u32>) -> String {
    value.map_or_else(
        || "not observed".to_string(),
        |value| format!("{value:#010x}"),
    )
}

pub(crate) fn render_hex_bytes(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "not observed".to_string();
    }
    let mut output = String::from("0x");
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

pub(crate) fn render_optional_status(value: Option<i32>) -> String {
    value.map_or_else(
        || "not attempted".to_string(),
        |status| format!("{status:#x}"),
    )
}

pub(crate) fn render_optional_status_name(value: Option<i32>) -> &'static str {
    value.map_or("not attempted", hv_return_name)
}

pub(crate) fn render_optional_exit_reason(value: Option<u32>) -> String {
    value.map_or_else(
        || "not observed".to_string(),
        |reason| format!("{reason:#x}"),
    )
}

pub(crate) fn render_optional_exit_reason_name(value: Option<u32>) -> &'static str {
    value.map_or("not observed", hv_exit_reason_name)
}

pub(crate) fn render_optional_exception_class_name(value: Option<u64>) -> &'static str {
    value.map_or("not observed", arm_exception_class_name)
}

pub(crate) fn render_optional_esr_exception_class_name(value: Option<u64>) -> &'static str {
    value.map_or("not observed", |esr| {
        arm_exception_class_name(arm_exception_class(esr))
    })
}

pub(crate) fn render_optional_sctlr_mmu_enabled(value: Option<u64>) -> &'static str {
    match value {
        Some(sctlr) if sctlr & 1 == 1 => "true",
        Some(_) => "false",
        None => "not observed",
    }
}

pub(crate) fn windows_arm_initial_sp_el1_ipa(guest_ram_bytes: u64) -> u64 {
    WINDOWS_ARM_GUEST_RAM_IPA
        .saturating_add(guest_ram_bytes)
        .saturating_sub(16)
        & !0xf
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowsArmFirmwareRunLoopDiagnosis {
    DiagnosticVectorHvcExit,
    DiagnosticVectorContinuationHvcExit,
    GuestRamDiagnosticVectorHvcExit,
    GuestRamDiagnosticVectorContinuationHvcExit,
    ExecutableDiagnosticVectorHvcExit,
    ExecutableDiagnosticVectorContinuationHvcExit,
    ExecutableDiagnosticVectorEretLandingHvcExit,
    LowVectorDiagnosticPageHvcExit,
    LowVectorDiagnosticPageEretLandingHvcExit,
    DiagnosticVectorStage1XnPermissionFault,
    GuestRamDiagnosticVectorStage1XnPermissionFault,
    ExecutableDiagnosticVectorStage1XnPermissionFault,
    DiagnosticVectorMmuInstructionAbort,
    GuestRamDiagnosticVectorMmuInstructionAbort,
    ExecutableDiagnosticVectorMmuInstructionAbort,
    RecommendedVectorBaseEmptySyncSlot,
    El1LowVectorMmuTranslationFault,
    ErasedPflashExecution,
    NotClassified,
}

impl WindowsArmFirmwareRunLoopDiagnosis {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::DiagnosticVectorHvcExit => "diagnostic-vector-hvc-exit",
            Self::DiagnosticVectorContinuationHvcExit => "diagnostic-vector-continuation-hvc-exit",
            Self::GuestRamDiagnosticVectorHvcExit => "guest-ram-diagnostic-vector-hvc-exit",
            Self::GuestRamDiagnosticVectorContinuationHvcExit => {
                "guest-ram-diagnostic-vector-continuation-hvc-exit"
            }
            Self::ExecutableDiagnosticVectorHvcExit => "executable-diagnostic-vector-hvc-exit",
            Self::ExecutableDiagnosticVectorContinuationHvcExit => {
                "executable-diagnostic-vector-continuation-hvc-exit"
            }
            Self::ExecutableDiagnosticVectorEretLandingHvcExit => {
                "executable-diagnostic-vector-eret-landing-hvc-exit"
            }
            Self::LowVectorDiagnosticPageHvcExit => "low-vector-diagnostic-page-hvc-exit",
            Self::LowVectorDiagnosticPageEretLandingHvcExit => {
                "low-vector-diagnostic-page-eret-landing-hvc-exit"
            }
            Self::DiagnosticVectorStage1XnPermissionFault => {
                "diagnostic-vector-stage1-xn-permission-fault"
            }
            Self::GuestRamDiagnosticVectorStage1XnPermissionFault => {
                "guest-ram-diagnostic-vector-stage1-xn-permission-fault"
            }
            Self::ExecutableDiagnosticVectorStage1XnPermissionFault => {
                "executable-diagnostic-vector-stage1-xn-permission-fault"
            }
            Self::DiagnosticVectorMmuInstructionAbort => "diagnostic-vector-mmu-instruction-abort",
            Self::GuestRamDiagnosticVectorMmuInstructionAbort => {
                "guest-ram-diagnostic-vector-mmu-instruction-abort"
            }
            Self::ExecutableDiagnosticVectorMmuInstructionAbort => {
                "executable-diagnostic-vector-mmu-instruction-abort"
            }
            Self::RecommendedVectorBaseEmptySyncSlot => "recommended-vector-base-empty-sync-slot",
            Self::El1LowVectorMmuTranslationFault => "el1-low-vector-mmu-translation-fault",
            Self::ErasedPflashExecution => "erased-pflash-execution",
            Self::NotClassified => "not classified",
        }
    }
}

pub(crate) fn windows_arm_firmware_run_loop_exit_diagnosis(
    exit: &WindowsArmUefiFirmwareRunLoopExit,
) -> &'static str {
    windows_arm_firmware_run_loop_exit_diagnosis_kind(exit).as_str()
}

pub(crate) fn recommended_vector_base_vbar_initial_reason(
    requested: bool,
    diagnostic_vector_seed_requested: bool,
    repair_low_vector_diagnostic_page: bool,
) -> &'static str {
    if !requested {
        "not requested"
    } else if diagnostic_vector_seed_requested {
        "ignored-diagnostic-vector-seed"
    } else if repair_low_vector_diagnostic_page {
        "ignored-low-vector-repair"
    } else {
        "not selected"
    }
}

pub(crate) fn windows_arm_firmware_run_loop_exit_diagnosis_kind(
    exit: &WindowsArmUefiFirmwareRunLoopExit,
) -> WindowsArmFirmwareRunLoopDiagnosis {
    let mmu_enabled = exit
        .sctlr_el1_after_exit
        .map(|sctlr| sctlr & 1 == 1)
        .unwrap_or(false);
    let esr_is_instruction_abort_same_el = exit
        .esr_el1_after_exit
        .map(|esr| arm_exception_class(esr) == 0x21)
        .unwrap_or(false);
    let esr_is_translation_fault_level_3 = exit
        .esr_el1_after_exit
        .map(|esr| arm_abort_fault_status(esr) == 0x07)
        .unwrap_or(false);
    let pflash_diagnostic_vector_sync_pc = WINDOWS_ARM_DIAGNOSTIC_VECTOR_IPA
        + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64;
    let guest_ram_diagnostic_vector_sync_pc = WINDOWS_ARM_GUEST_RAM_DIAGNOSTIC_VECTOR_IPA
        + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64;
    let executable_diagnostic_vector_sync_pc = WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA
        + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64;
    let pflash_diagnostic_vector_hvc_exit_pc = pflash_diagnostic_vector_sync_pc + 4;
    let guest_ram_diagnostic_vector_hvc_exit_pc = guest_ram_diagnostic_vector_sync_pc + 4;
    let executable_diagnostic_vector_hvc_exit_pc = executable_diagnostic_vector_sync_pc + 4;
    let pflash_diagnostic_vector_continuation_hvc_exit_pc = pflash_diagnostic_vector_sync_pc + 8;
    let guest_ram_diagnostic_vector_continuation_hvc_exit_pc =
        guest_ram_diagnostic_vector_sync_pc + 8;
    let executable_diagnostic_vector_continuation_hvc_exit_pc =
        executable_diagnostic_vector_sync_pc + 8;
    let executable_diagnostic_vector_eret_landing_hvc_exit_pc =
        executable_diagnostic_vector_sync_pc + 12;
    let low_vector_diagnostic_page_hvc_exit_pc =
        WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64 + 4;
    let low_vector_diagnostic_page_eret_landing_hvc_exit_pc =
        WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64 + 12;
    let low_vector_diagnostic_page_is_mapped = exit.pc_stage1_leaf_descriptor_after_exit
        == Some(WINDOWS_ARM_LOW_VECTOR_DIAGNOSTIC_PAGE_DESCRIPTOR);
    if exit.vbar_el1_after_exit == Some(WINDOWS_ARM_DIAGNOSTIC_VECTOR_IPA)
        && (exit.pc_after_exit == Some(pflash_diagnostic_vector_sync_pc)
            || exit.pc_after_exit == Some(pflash_diagnostic_vector_hvc_exit_pc))
        && exit.exit_exception_class == Some(0x16)
    {
        WindowsArmFirmwareRunLoopDiagnosis::DiagnosticVectorHvcExit
    } else if exit.vbar_el1_after_exit == Some(WINDOWS_ARM_DIAGNOSTIC_VECTOR_IPA)
        && exit.pc_after_exit == Some(pflash_diagnostic_vector_continuation_hvc_exit_pc)
        && exit.exit_exception_class == Some(0x16)
    {
        WindowsArmFirmwareRunLoopDiagnosis::DiagnosticVectorContinuationHvcExit
    } else if exit.vbar_el1_after_exit == Some(WINDOWS_ARM_GUEST_RAM_DIAGNOSTIC_VECTOR_IPA)
        && (exit.pc_after_exit == Some(guest_ram_diagnostic_vector_sync_pc)
            || exit.pc_after_exit == Some(guest_ram_diagnostic_vector_hvc_exit_pc))
        && exit.exit_exception_class == Some(0x16)
    {
        WindowsArmFirmwareRunLoopDiagnosis::GuestRamDiagnosticVectorHvcExit
    } else if exit.vbar_el1_after_exit == Some(WINDOWS_ARM_GUEST_RAM_DIAGNOSTIC_VECTOR_IPA)
        && exit.pc_after_exit == Some(guest_ram_diagnostic_vector_continuation_hvc_exit_pc)
        && exit.exit_exception_class == Some(0x16)
    {
        WindowsArmFirmwareRunLoopDiagnosis::GuestRamDiagnosticVectorContinuationHvcExit
    } else if exit.vbar_el1_after_exit == Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA)
        && (exit.pc_after_exit == Some(executable_diagnostic_vector_sync_pc)
            || exit.pc_after_exit == Some(executable_diagnostic_vector_hvc_exit_pc))
        && exit.exit_exception_class == Some(0x16)
    {
        WindowsArmFirmwareRunLoopDiagnosis::ExecutableDiagnosticVectorHvcExit
    } else if exit.vbar_el1_after_exit == Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA)
        && exit.pc_after_exit == Some(executable_diagnostic_vector_continuation_hvc_exit_pc)
        && exit.exit_exception_class == Some(0x16)
    {
        WindowsArmFirmwareRunLoopDiagnosis::ExecutableDiagnosticVectorContinuationHvcExit
    } else if exit.vbar_el1_after_exit == Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA)
        && exit.pc_after_exit == Some(executable_diagnostic_vector_eret_landing_hvc_exit_pc)
        && exit.exit_exception_class == Some(0x16)
    {
        WindowsArmFirmwareRunLoopDiagnosis::ExecutableDiagnosticVectorEretLandingHvcExit
    } else if (exit.vbar_el1_after_exit == Some(0) || low_vector_diagnostic_page_is_mapped)
        && exit.pc_after_exit == Some(low_vector_diagnostic_page_hvc_exit_pc)
        && exit.exit_exception_class == Some(0x16)
        && exit.instruction_word_after_exit == Some(AARCH64_ERET_INSTRUCTION)
    {
        WindowsArmFirmwareRunLoopDiagnosis::LowVectorDiagnosticPageHvcExit
    } else if (exit.vbar_el1_after_exit == Some(0) || low_vector_diagnostic_page_is_mapped)
        && exit.pc_after_exit == Some(low_vector_diagnostic_page_eret_landing_hvc_exit_pc)
        && exit.exit_exception_class == Some(0x16)
    {
        WindowsArmFirmwareRunLoopDiagnosis::LowVectorDiagnosticPageEretLandingHvcExit
    } else if exit.vbar_el1_after_exit == Some(WINDOWS_ARM_DIAGNOSTIC_VECTOR_IPA)
        && exit.pc_after_exit == Some(pflash_diagnostic_vector_sync_pc)
        && mmu_enabled
        && esr_is_instruction_abort_same_el
        && (exit.pc_stage1_leaf_pxn_after_exit == Some(true)
            || exit.pc_stage1_leaf_uxn_after_exit == Some(true))
    {
        WindowsArmFirmwareRunLoopDiagnosis::DiagnosticVectorStage1XnPermissionFault
    } else if exit.vbar_el1_after_exit == Some(WINDOWS_ARM_GUEST_RAM_DIAGNOSTIC_VECTOR_IPA)
        && exit.pc_after_exit == Some(guest_ram_diagnostic_vector_sync_pc)
        && mmu_enabled
        && esr_is_instruction_abort_same_el
        && (exit.pc_stage1_leaf_pxn_after_exit == Some(true)
            || exit.pc_stage1_leaf_uxn_after_exit == Some(true))
    {
        WindowsArmFirmwareRunLoopDiagnosis::GuestRamDiagnosticVectorStage1XnPermissionFault
    } else if exit.vbar_el1_after_exit == Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA)
        && exit.pc_after_exit == Some(executable_diagnostic_vector_sync_pc)
        && mmu_enabled
        && esr_is_instruction_abort_same_el
        && (exit.pc_stage1_leaf_pxn_after_exit == Some(true)
            || exit.pc_stage1_leaf_uxn_after_exit == Some(true))
    {
        WindowsArmFirmwareRunLoopDiagnosis::ExecutableDiagnosticVectorStage1XnPermissionFault
    } else if exit.vbar_el1_after_exit == Some(WINDOWS_ARM_DIAGNOSTIC_VECTOR_IPA)
        && exit.pc_after_exit == Some(pflash_diagnostic_vector_sync_pc)
        && mmu_enabled
        && esr_is_instruction_abort_same_el
    {
        WindowsArmFirmwareRunLoopDiagnosis::DiagnosticVectorMmuInstructionAbort
    } else if exit.vbar_el1_after_exit == Some(WINDOWS_ARM_GUEST_RAM_DIAGNOSTIC_VECTOR_IPA)
        && exit.pc_after_exit == Some(guest_ram_diagnostic_vector_sync_pc)
        && mmu_enabled
        && esr_is_instruction_abort_same_el
    {
        WindowsArmFirmwareRunLoopDiagnosis::GuestRamDiagnosticVectorMmuInstructionAbort
    } else if exit.vbar_el1_after_exit == Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA)
        && exit.pc_after_exit == Some(executable_diagnostic_vector_sync_pc)
        && mmu_enabled
        && esr_is_instruction_abort_same_el
    {
        WindowsArmFirmwareRunLoopDiagnosis::ExecutableDiagnosticVectorMmuInstructionAbort
    } else if exit.vbar_el1_after_exit == Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA)
        && exit.pc_after_exit == Some(executable_diagnostic_vector_sync_pc)
        && exit.instruction_word_after_exit == Some(0)
        && exit.pc_stage1_leaf_pxn_after_exit == Some(false)
        && exit.pc_stage1_leaf_uxn_after_exit == Some(false)
    {
        WindowsArmFirmwareRunLoopDiagnosis::RecommendedVectorBaseEmptySyncSlot
    } else if exit.vbar_el1_after_exit == Some(0)
        && exit.pc_after_exit == Some(0x200)
        && exit.elr_el1_after_exit == Some(0x200)
        && exit.far_el1_after_exit == Some(0x200)
        && mmu_enabled
        && esr_is_instruction_abort_same_el
        && esr_is_translation_fault_level_3
    {
        WindowsArmFirmwareRunLoopDiagnosis::El1LowVectorMmuTranslationFault
    } else if exit.instruction_word_after_exit == Some(0xffff_ffff) {
        WindowsArmFirmwareRunLoopDiagnosis::ErasedPflashExecution
    } else {
        WindowsArmFirmwareRunLoopDiagnosis::NotClassified
    }
}

pub(crate) fn render_optional_abort_iss(value: Option<u64>) -> String {
    value.map_or_else(
        || "not observed".to_string(),
        |syndrome| format!("{:#x}", arm_abort_iss(syndrome)),
    )
}

pub(crate) fn render_optional_abort_fault_status(value: Option<u64>) -> String {
    value.map_or_else(
        || "not observed".to_string(),
        |syndrome| format!("{:#x}", arm_abort_fault_status(syndrome)),
    )
}

pub(crate) fn render_optional_abort_fault_status_name(value: Option<u64>) -> &'static str {
    value.map_or("not observed", |syndrome| {
        arm_abort_fault_status_name(arm_abort_fault_status(syndrome))
    })
}

pub(crate) fn arm_exception_class(syndrome: u64) -> u64 {
    syndrome >> 26
}

pub(crate) fn arm_abort_iss(syndrome: u64) -> u64 {
    syndrome & 0x01ff_ffff
}

pub(crate) fn arm_abort_fault_status(syndrome: u64) -> u64 {
    syndrome & 0x3f
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DecodedMmioDataAbort {
    pub(crate) is_write: bool,
    pub(crate) register: u8,
    pub(crate) width: u8,
}

impl DecodedMmioDataAbort {
    pub(crate) fn access_name(self) -> &'static str {
        if self.is_write {
            "write"
        } else {
            "read"
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DecodedSystemRegisterAccess {
    pub(crate) is_read: bool,
    pub(crate) register: u8,
    pub(crate) sys_reg: u16,
    pub(crate) op0: u8,
    pub(crate) op1: u8,
    pub(crate) crn: u8,
    pub(crate) crm: u8,
    pub(crate) op2: u8,
}

impl DecodedSystemRegisterAccess {
    pub(crate) fn access_name(self) -> &'static str {
        if self.is_read {
            "read"
        } else {
            "write"
        }
    }
}

pub(crate) fn aarch64_sys_reg_encoding(op0: u8, op1: u8, crn: u8, crm: u8, op2: u8) -> u16 {
    (u16::from(op0) << 14)
        | (u16::from(op1) << 11)
        | (u16::from(crn) << 7)
        | (u16::from(crm) << 3)
        | u16::from(op2)
}

pub(crate) fn decode_system_register_trap(syndrome: u64) -> Option<DecodedSystemRegisterAccess> {
    if arm_exception_class(syndrome) != AARCH64_SYSREG_TRAP_EXCEPTION_CLASS {
        return None;
    }
    let iss = arm_abort_iss(syndrome);
    let op0 = ((iss >> 20) & 0x3) as u8;
    let op2 = ((iss >> 17) & 0x7) as u8;
    let op1 = ((iss >> 14) & 0x7) as u8;
    let crn = ((iss >> 10) & 0xf) as u8;
    let register = ((iss >> 5) & 0x1f) as u8;
    let crm = ((iss >> 1) & 0xf) as u8;
    let is_read = (iss & 1) != 0;
    Some(DecodedSystemRegisterAccess {
        is_read,
        register,
        sys_reg: aarch64_sys_reg_encoding(op0, op1, crn, crm, op2),
        op0,
        op1,
        crn,
        crm,
        op2,
    })
}

pub(crate) fn decode_mmio_data_abort(syndrome: u64) -> Option<DecodedMmioDataAbort> {
    if !matches!(arm_exception_class(syndrome), 0x24 | 0x25) {
        return None;
    }
    let iss = arm_abort_iss(syndrome);
    if ((iss >> 24) & 1) == 0 {
        return None;
    }
    if ((iss >> 21) & 1) != 0 {
        return None;
    }
    let register = ((iss >> 16) & 0x1f) as u8;
    if register == 31 {
        return None;
    }
    let width = match (iss >> 22) & 0x3 {
        0 => 1,
        1 => 2,
        2 => 4,
        3 => 8,
        _ => unreachable!("masked two-bit access size"),
    };
    Some(DecodedMmioDataAbort {
        is_write: ((iss >> 6) & 1) != 0,
        register,
        width,
    })
}

pub(crate) fn arm_abort_fault_status_name(status: u64) -> &'static str {
    match status {
        0x00 => "address size fault level 0",
        0x01 => "address size fault level 1",
        0x02 => "address size fault level 2",
        0x03 => "address size fault level 3",
        0x04 => "translation fault level 0",
        0x05 => "translation fault level 1",
        0x06 => "translation fault level 2",
        0x07 => "translation fault level 3",
        0x09 => "access flag fault level 1",
        0x0a => "access flag fault level 2",
        0x0b => "access flag fault level 3",
        0x0d => "permission fault level 1",
        0x0e => "permission fault level 2",
        0x0f => "permission fault level 3",
        0x10 => "synchronous external abort",
        0x14 => "synchronous external abort on translation table walk level 0",
        0x15 => "synchronous external abort on translation table walk level 1",
        0x16 => "synchronous external abort on translation table walk level 2",
        0x17 => "synchronous external abort on translation table walk level 3",
        0x18 => "synchronous parity or ECC error",
        0x1c => "synchronous parity or ECC error on translation table walk level 0",
        0x1d => "synchronous parity or ECC error on translation table walk level 1",
        0x1e => "synchronous parity or ECC error on translation table walk level 2",
        0x1f => "synchronous parity or ECC error on translation table walk level 3",
        0x21 => "alignment fault",
        0x22 => "debug event",
        0x30 => "TLB conflict abort",
        0x3d => "unsupported atomic hardware update fault",
        _ => "unknown",
    }
}

pub(crate) fn windows_arm_guest_region_name(
    address: Option<u64>,
    guest_ram_bytes: u64,
) -> &'static str {
    let Some(address) = address else {
        return "not observed";
    };
    if address >= WINDOWS_ARM_UEFI_CODE_IPA
        && address < WINDOWS_ARM_UEFI_CODE_IPA.saturating_add(WINDOWS_ARM_UEFI_SLOT_BYTES)
    {
        "firmware pflash slot"
    } else if address
        < WINDOWS_ARM_UEFI_LOW_CODE_ALIAS_IPA.saturating_add(WINDOWS_ARM_UEFI_SLOT_BYTES)
    {
        "low firmware pflash alias"
    } else if address >= WINDOWS_ARM_UEFI_LOW_VARS_ALIAS_IPA
        && address < WINDOWS_ARM_UEFI_LOW_VARS_ALIAS_IPA.saturating_add(WINDOWS_ARM_UEFI_SLOT_BYTES)
    {
        "low vars pflash alias"
    } else if address >= WINDOWS_ARM_UEFI_VARS_IPA
        && address < WINDOWS_ARM_UEFI_VARS_IPA.saturating_add(WINDOWS_ARM_UEFI_SLOT_BYTES)
    {
        "vars pflash slot"
    } else if address >= WINDOWS_ARM_DEVICE_MMIO_IPA
        && address < WINDOWS_ARM_DEVICE_MMIO_IPA.saturating_add(WINDOWS_ARM_DEVICE_MMIO_BYTES)
    {
        "Windows device MMIO window"
    } else if address >= WINDOWS_ARM_GUEST_RAM_IPA
        && address < WINDOWS_ARM_GUEST_RAM_IPA.saturating_add(guest_ram_bytes)
    {
        "guest RAM"
    } else {
        "unmapped or unknown"
    }
}

pub(crate) fn aarch64_instruction_hint(instruction: u32) -> &'static str {
    match instruction {
        0xffff_ffff => "erased-pflash",
        0xd400_0002 => "hvc-0",
        0xd400_0022 => "hvc-1",
        0xd69f_03e0 => "eret",
        0xd503_201f => "nop",
        0xd503_203f => "yield",
        0xd503_205f => "wfe",
        0xd503_207f => "wfi",
        0xd503_209f => "sev",
        0xd503_20bf => "sevl",
        _ => "unknown",
    }
}

pub(crate) fn arm_exception_class_name(class: u64) -> &'static str {
    match class {
        0x00 => "unknown reason",
        0x01 => "trapped WFI/WFE",
        0x07 => "trapped SVE/SIMD/FP",
        0x11 => "SVC AArch32",
        0x15 => "SVC AArch64",
        0x16 => "HVC AArch64",
        0x17 => "SMC AArch64",
        0x20 => "instruction abort lower EL",
        0x21 => "instruction abort same EL",
        0x22 => "PC alignment fault",
        0x24 => "data abort lower EL",
        0x25 => "data abort same EL",
        0x26 => "SP alignment fault",
        0x2c => "trapped floating point",
        0x2f => "SError interrupt",
        0x30 => "breakpoint lower EL",
        0x31 => "breakpoint same EL",
        0x32 => "software step lower EL",
        0x33 => "software step same EL",
        0x34 => "watchpoint lower EL",
        0x35 => "watchpoint same EL",
        0x3c => "BRK AArch64",
        _ => "unknown",
    }
}
