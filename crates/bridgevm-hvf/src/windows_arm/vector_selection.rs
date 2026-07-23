//! Split out of windows_arm.rs by responsibility.

use super::*;

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
