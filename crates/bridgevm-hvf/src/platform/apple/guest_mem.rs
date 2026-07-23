//! Guest-memory reads: instruction words and 32-bit physical reads.
//!
//! Split out of the single 12,111-line apple.rs backend.

use super::*;

pub(crate) fn read_guest_instruction_word(
    pc: Option<u64>,
    firmware_memory: *const c_void,
    vars_memory: *const c_void,
    guest_ram_memory: *const c_void,
    guest_ram_bytes: usize,
) -> Option<u32> {
    read_known_guest_phys_u32(
        pc?,
        firmware_memory,
        vars_memory,
        guest_ram_memory,
        guest_ram_bytes,
    )
}

pub(crate) fn read_known_guest_phys_u32(
    ipa: u64,
    firmware_memory: *const c_void,
    vars_memory: *const c_void,
    guest_ram_memory: *const c_void,
    guest_ram_bytes: usize,
) -> Option<u32> {
    let (memory, offset, bytes) = guest_phys_memory_offset(
        ipa,
        firmware_memory,
        vars_memory,
        guest_ram_memory,
        guest_ram_bytes,
    )?;
    if memory.is_null() || offset.checked_add(4)? > bytes {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(memory.cast::<u8>().add(offset), 4) };
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DiagnosticVectorRoute {
    pub(crate) vbar_el1: u64,
    pub(crate) sync_pc: u64,
}

impl DiagnosticVectorRoute {
    pub(crate) fn eret_pc(self) -> u64 {
        self.sync_pc + 4
    }

    pub(crate) fn landing_pc(self) -> u64 {
        self.sync_pc + 8
    }

    pub(crate) fn stop_pc(self) -> u64 {
        self.sync_pc + 12
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DiagnosticVectorEretRouteStatus {
    pub(crate) elr_status: HvReturn,
    pub(crate) pc_status: HvReturn,
}

impl DiagnosticVectorEretRouteStatus {
    pub(crate) fn succeeded(self) -> bool {
        self.elr_status == HV_SUCCESS && self.pc_status == HV_SUCCESS
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DiagnosticVectorOriginalContextResumeStatus {
    pub(crate) elr_status: HvReturn,
    pub(crate) vbar_status: Option<HvReturn>,
    pub(crate) spsr_status: HvReturn,
    pub(crate) pc_status: HvReturn,
}

impl DiagnosticVectorOriginalContextResumeStatus {
    pub(crate) fn vbar_effective_status(self) -> HvReturn {
        Self::effective_vbar_status(self.elr_status, self.vbar_status)
    }

    pub(crate) fn effective_vbar_status(
        elr_status: HvReturn,
        vbar_status: Option<HvReturn>,
    ) -> HvReturn {
        vbar_status.unwrap_or({
            if elr_status == HV_SUCCESS {
                HV_SUCCESS
            } else {
                elr_status
            }
        })
    }

    pub(crate) fn succeeded(self) -> bool {
        self.elr_status == HV_SUCCESS
            && self.vbar_effective_status() == HV_SUCCESS
            && self.spsr_status == HV_SUCCESS
            && self.pc_status == HV_SUCCESS
    }
}
