//! 64-bit guest-physical read/write, stage-1 descriptor patching, and IPA/pflash offset mapping.
//!
//! Split out of the single 12,111-line apple.rs backend.

use super::*;
use crate::*;

pub(crate) fn read_known_guest_phys_u64(
    ipa: u64,
    firmware_memory: *const c_void,
    vars_memory: *const c_void,
    guest_ram_memory: *const c_void,
    guest_ram_bytes: usize,
) -> Option<u64> {
    let (memory, offset, bytes) = guest_phys_memory_offset(
        ipa,
        firmware_memory,
        vars_memory,
        guest_ram_memory,
        guest_ram_bytes,
    )?;
    if memory.is_null() || offset.checked_add(8)? > bytes {
        return None;
    }
    let raw = unsafe { std::slice::from_raw_parts(memory.cast::<u8>().add(offset), 8) };
    Some(u64::from_le_bytes(raw.try_into().ok()?))
}

pub(crate) fn write_known_guest_phys_u64(
    ipa: u64,
    value: u64,
    firmware_memory: *mut c_void,
    vars_memory: *mut c_void,
    guest_ram_memory: *mut c_void,
    guest_ram_bytes: usize,
) -> bool {
    let (memory, offset, bytes) = match guest_phys_memory_offset(
        ipa,
        firmware_memory.cast_const(),
        vars_memory.cast_const(),
        guest_ram_memory.cast_const(),
        guest_ram_bytes,
    ) {
        Some(location) => location,
        None => return false,
    };
    if memory.is_null() || offset.saturating_add(8) > bytes {
        return false;
    }
    let raw = value.to_le_bytes();
    unsafe {
        ptr::copy_nonoverlapping(raw.as_ptr(), memory.cast_mut().cast::<u8>().add(offset), 8);
    }
    true
}

pub(crate) fn stage1_page_descriptor_for_output_address(
    output_address: u64,
    template_descriptor: u64,
) -> Option<u64> {
    if output_address & !AARCH64_STAGE1_PAGE_OUTPUT_ADDRESS_MASK != 0 {
        return None;
    }
    Some(
        (template_descriptor & !AARCH64_STAGE1_PAGE_OUTPUT_ADDRESS_MASK)
            | (output_address & AARCH64_STAGE1_PAGE_OUTPUT_ADDRESS_MASK),
    )
}

pub(crate) fn patch_low_vector_stage1_page_descriptor(
    tcr_el1: Option<u64>,
    ttbr0_el1: Option<u64>,
    descriptor: u64,
    firmware_memory: *mut c_void,
    vars_memory: *mut c_void,
    guest_ram_memory: *mut c_void,
    guest_ram_bytes: usize,
) -> Option<(u64, u64)> {
    let tcr = tcr_el1?;
    let ttbr0 = ttbr0_el1?;
    let tg0 = (tcr >> 14) & 0x3;
    if tg0 != 0 {
        return None;
    }
    let t0sz = tcr & 0x3f;
    if t0sz > 48 {
        return None;
    }
    let va = WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64;
    let va_bits = 64 - t0sz;
    let start_level = match va_bits {
        40..=64 => 0,
        31..=39 => 1,
        22..=30 => 2,
        _ => 3,
    };
    let mut table_ipa = ttbr0 & 0x0000_ffff_ffff_f000;
    for level in start_level..=3 {
        let shift = 39u32.saturating_sub(level as u32 * 9);
        let index = (va >> shift) & 0x1ff;
        let entry_ipa = table_ipa.checked_add(index.checked_mul(8)?)?;
        if level == 3 {
            let previous = read_known_guest_phys_u64(
                entry_ipa,
                firmware_memory.cast_const(),
                vars_memory.cast_const(),
                guest_ram_memory.cast_const(),
                guest_ram_bytes,
            )?;
            if write_known_guest_phys_u64(
                entry_ipa,
                descriptor,
                firmware_memory,
                vars_memory,
                guest_ram_memory,
                guest_ram_bytes,
            ) {
                return Some((entry_ipa, previous));
            }
            return None;
        }
        let descriptor = read_known_guest_phys_u64(
            entry_ipa,
            firmware_memory.cast_const(),
            vars_memory.cast_const(),
            guest_ram_memory.cast_const(),
            guest_ram_bytes,
        )?;
        if stage1_descriptor_kind(descriptor, level as u8) != "table" {
            return None;
        }
        table_ipa = descriptor & 0x0000_ffff_ffff_f000;
    }
    None
}

pub(crate) fn patch_low_vector_diagnostic_page_descriptor(
    tcr_el1: Option<u64>,
    ttbr0_el1: Option<u64>,
    firmware_memory: *mut c_void,
    vars_memory: *mut c_void,
    guest_ram_memory: *mut c_void,
    guest_ram_bytes: usize,
) -> Option<(u64, u64)> {
    patch_low_vector_stage1_page_descriptor(
        tcr_el1,
        ttbr0_el1,
        WINDOWS_ARM_LOW_VECTOR_DIAGNOSTIC_PAGE_DESCRIPTOR,
        firmware_memory,
        vars_memory,
        guest_ram_memory,
        guest_ram_bytes,
    )
}

pub(crate) fn patch_low_vector_recommended_vector_descriptor(
    recommendation: &WindowsArmUefiVectorBaseRecommendation,
    tcr_el1: Option<u64>,
    ttbr0_el1: Option<u64>,
    firmware_memory: *mut c_void,
    vars_memory: *mut c_void,
    guest_ram_memory: *mut c_void,
    guest_ram_bytes: usize,
) -> Option<(u64, u64, u64)> {
    let descriptor = stage1_page_descriptor_for_output_address(
        recommendation.base_physical_address?,
        WINDOWS_ARM_LOW_VECTOR_DIAGNOSTIC_PAGE_DESCRIPTOR,
    )?;
    let (entry_ipa, previous_descriptor) = patch_low_vector_stage1_page_descriptor(
        tcr_el1,
        ttbr0_el1,
        descriptor,
        firmware_memory,
        vars_memory,
        guest_ram_memory,
        guest_ram_bytes,
    )?;
    Some((entry_ipa, previous_descriptor, descriptor))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LowVectorDiagnosticPageRepairPreparation {
    pub(crate) diagnostic_slot_snapshot: Option<DiagnosticExceptionVectorSlotSnapshot>,
    pub(crate) patched_descriptor: Option<(u64, u64)>,
}

impl LowVectorDiagnosticPageRepairPreparation {
    pub(crate) fn vector_populated(self) -> bool {
        self.diagnostic_slot_snapshot.is_some()
    }
}

pub(crate) struct LowVectorDiagnosticPageRepairRequest<'a> {
    pub(crate) firmware_memory: *mut c_void,
    pub(crate) vars_memory: *mut c_void,
    pub(crate) guest_ram_memory: *mut c_void,
    pub(crate) slot_bytes: usize,
    pub(crate) guest_ram_bytes: usize,
    pub(crate) tcr_el1: Option<u64>,
    pub(crate) ttbr0_el1: Option<u64>,
    pub(crate) location: &'a str,
    pub(crate) blockers: &'a mut Vec<String>,
}

pub(crate) fn prepare_low_vector_diagnostic_page_repair(
    request: LowVectorDiagnosticPageRepairRequest<'_>,
) -> LowVectorDiagnosticPageRepairPreparation {
    let diagnostic_slot_snapshot = install_diagnostic_exception_vector_slot_preserving(
        request.firmware_memory,
        request.slot_bytes,
        0,
        request.location,
        request.blockers,
    );
    let patched_descriptor = patch_low_vector_diagnostic_page_descriptor(
        request.tcr_el1,
        request.ttbr0_el1,
        request.firmware_memory,
        request.vars_memory,
        request.guest_ram_memory,
        request.guest_ram_bytes,
    );
    LowVectorDiagnosticPageRepairPreparation {
        diagnostic_slot_snapshot,
        patched_descriptor,
    }
}

pub(crate) fn guest_phys_memory_offset(
    ipa: u64,
    firmware_memory: *const c_void,
    vars_memory: *const c_void,
    guest_ram_memory: *const c_void,
    guest_ram_bytes: usize,
) -> Option<(*const c_void, usize, usize)> {
    let slot_bytes: usize = WINDOWS_ARM_UEFI_SLOT_BYTES.try_into().ok()?;
    if let Some(offset) = pflash_slot_offset(ipa, WINDOWS_ARM_UEFI_CODE_IPA)
        .or_else(|| pflash_slot_offset(ipa, WINDOWS_ARM_UEFI_LOW_CODE_ALIAS_IPA))
    {
        return Some((firmware_memory, offset, slot_bytes));
    }
    if let Some(offset) = pflash_slot_offset(ipa, WINDOWS_ARM_UEFI_VARS_IPA)
        .or_else(|| pflash_slot_offset(ipa, WINDOWS_ARM_UEFI_LOW_VARS_ALIAS_IPA))
    {
        return Some((vars_memory, offset, slot_bytes));
    }
    if ipa >= WINDOWS_ARM_GUEST_RAM_IPA
        && ipa < WINDOWS_ARM_GUEST_RAM_IPA.saturating_add(guest_ram_bytes as u64)
    {
        let offset = ipa
            .checked_sub(WINDOWS_ARM_GUEST_RAM_IPA)?
            .try_into()
            .ok()?;
        return Some((guest_ram_memory, offset, guest_ram_bytes));
    }
    None
}

pub(crate) fn pflash_slot_offset(address: u64, slot_ipa: u64) -> Option<usize> {
    if address >= slot_ipa && address < slot_ipa.saturating_add(WINDOWS_ARM_UEFI_SLOT_BYTES) {
        address.checked_sub(slot_ipa)?.try_into().ok()
    } else {
        None
    }
}

pub(crate) struct VcpuRunObservation {
    pub(crate) run_status: HvReturn,
    pub(crate) exit_reason: Option<u32>,
    pub(crate) exit_syndrome: Option<u64>,
    pub(crate) exit_virtual_address: Option<u64>,
    pub(crate) exit_physical_address: Option<u64>,
    pub(crate) watchdog_cancel_status: Option<HvReturn>,
}
