//! Raw Hypervisor.framework FFI: type aliases, HV_* constants, the ABI
//! `#[repr(C)]` exit structures, and the single `extern "C"` declaration block.
//!
//! Split out of the single 12,111-line apple.rs backend.

use super::*;
use crate::*;

pub(crate) use std::{
    ffi::c_void,
    ptr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

pub(crate) type HvReturn = i32;
pub(crate) type HvVmConfig = *mut c_void;
pub(crate) type HvVcpuConfig = *mut c_void;
pub(crate) type HvVcpu = u64;
pub(crate) type HvSysReg = u16;
pub(crate) type HvInterruptType = u32;
pub(crate) const HV_SUCCESS: HvReturn = 0;
pub(crate) const HV_EXIT_REASON_CANCELED: u32 = 0;
pub(crate) const HV_EXIT_REASON_EXCEPTION: u32 = 1;
pub(crate) const HV_EXIT_REASON_VTIMER_ACTIVATED: u32 = 2;
pub(crate) const HV_INTERRUPT_TYPE_IRQ: HvInterruptType = 0;
pub(crate) const HV_ALLOCATE_DEFAULT: u64 = 0;
pub(crate) const HV_MEMORY_READ: u64 = 1 << 0;
pub(crate) const HV_MEMORY_WRITE: u64 = 1 << 1;
pub(crate) const HV_MEMORY_EXEC: u64 = 1 << 2;
pub(crate) const PROBE_IPA_START: u64 = 0x4000_0000;
pub(crate) const PROBE_MMIO_IPA: u64 = 0x5000_0000;
pub(crate) const PROBE_BYTES: usize = 16 * 1024;
pub(crate) const HV_REG_X0: u32 = 0;
pub(crate) const HV_REG_X1: u32 = 1;
pub(crate) const HV_REG_X2: u32 = 2;
pub(crate) const HV_REG_X3: u32 = 3;
pub(crate) const HV_REG_X4: u32 = 4;
pub(crate) const HV_REG_PC: u32 = 31;
pub(crate) const HV_REG_CPSR: u32 = 34;
pub(crate) const HV_SYS_REG_SCTLR_EL1: HvSysReg = 0xc080;
pub(crate) const HV_SYS_REG_TTBR0_EL1: HvSysReg = 0xc100;
pub(crate) const HV_SYS_REG_TTBR1_EL1: HvSysReg = 0xc101;
pub(crate) const HV_SYS_REG_TCR_EL1: HvSysReg = 0xc102;
pub(crate) const HV_SYS_REG_SPSR_EL1: HvSysReg = 0xc200;
pub(crate) const HV_SYS_REG_ELR_EL1: HvSysReg = 0xc201;
pub(crate) const HV_SYS_REG_ESR_EL1: HvSysReg = 0xc290;
pub(crate) const HV_SYS_REG_FAR_EL1: HvSysReg = 0xc300;
pub(crate) const HV_SYS_REG_MAIR_EL1: HvSysReg = 0xc510;
pub(crate) const HV_SYS_REG_VBAR_EL1: HvSysReg = 0xc600;
pub(crate) const HV_SYS_REG_CNTV_CTL_EL0: HvSysReg = 0xdf19;
pub(crate) const HV_SYS_REG_CNTV_CVAL_EL0: HvSysReg = 0xdf1a;
pub(crate) const HV_SYS_REG_SP_EL1: HvSysReg = 0xe208;
pub(crate) const AARCH64_PSTATE_EL1H_DAIF_MASKED: u64 = 0x3c5;
pub(crate) const AARCH64_HVC_0: u32 = crate::AARCH64_HVC_0_INSTRUCTION;
pub(crate) const AARCH64_HVC_1: u32 = crate::AARCH64_HVC_1_INSTRUCTION;
pub(crate) const AARCH64_ERET: u32 = crate::AARCH64_ERET_INSTRUCTION;
pub(crate) const DIAGNOSTIC_EXCEPTION_VECTOR_SLOT_BYTES: usize = 12;
pub(crate) const AARCH64_WFI: u32 = 0xd503_207f;
pub(crate) const AARCH64_LDR_X0_FROM_X1: u32 = 0xf940_0020;
pub(crate) const AARCH64_LDR_X0_FROM_X2: u32 = 0xf940_0040;
pub(crate) const AARCH64_LDR_W0_FROM_X1: u32 = 0xb940_0020;
pub(crate) const AARCH64_LDR_W0_FROM_X2: u32 = 0xb940_0040;
pub(crate) const AARCH64_LDR_W0_FROM_X3: u32 = 0xb940_0060;
pub(crate) const AARCH64_LDR_W0_FROM_X4: u32 = 0xb940_0080;
pub(crate) const AARCH64_STR_X0_TO_X1: u32 = 0xf900_0020;
pub(crate) const AARCH64_STR_W0_TO_X1: u32 = 0xb900_0020;
pub(crate) const AARCH64_HVC_0_SYNDROME: u64 = 0x5a00_0000;
pub(crate) const AARCH64_HVC_1_SYNDROME: u64 = 0x5a00_0001;
pub(crate) const EMULATED_MMIO_READ_VALUE: u64 = 0x1234_5678_9abc_def0;
pub(crate) const EMULATED_MMIO_WRITE_VALUE: u64 = 0x0fed_cba9_8765_4321;
pub(crate) const SERIAL_MMIO_DATA_IPA: u64 = PROBE_MMIO_IPA;
pub(crate) const SERIAL_MMIO_STATUS_IPA: u64 = PROBE_MMIO_IPA + PL011_FR_OFFSET;
pub(crate) const SERIAL_MMIO_WRITE_VALUE: u64 = 0x41;
pub(crate) const SERIAL_MMIO_STATUS_VALUE: u64 = 0x90;
pub(crate) const RTC_MMIO_IPA: u64 = PROBE_MMIO_IPA + 0x1000;
pub(crate) const RTC_MMIO_READ_VALUE: u64 = 0x2026_0618;
pub(crate) const BLOCK_MMIO_IPA: u64 = PROBE_MMIO_IPA + 0x2000;
pub(crate) const WINDOWS_ARM_STAGE1_EXECUTABLE_SCAN_STEP: u64 = 2 * 1024 * 1024;
pub(crate) const WINDOWS_ARM_STAGE1_EXECUTABLE_SCAN_MAX_CANDIDATES: usize = 16;
pub(crate) const WINDOWS_ARM_VECTOR_BASE_SCAN_ALIGNMENT: u64 = WINDOWS_ARM_DIAGNOSTIC_VECTOR_BYTES;
pub(crate) const WINDOWS_ARM_VECTOR_BASE_SCAN_MAX_PER_LEAF: usize = 8;
pub(crate) const WINDOWS_ARM_VTIMER_OFFSET_VALUE: u64 = 0x1000;
pub(crate) const WINDOWS_ARM_FIRMWARE_VTIMER_DEADLINE_TICKS: u64 = 50_000_000;

#[repr(C)]
pub(crate) struct HvVcpuExitException {
    pub(crate) syndrome: u64,
    pub(crate) virtual_address: u64,
    pub(crate) physical_address: u64,
}

#[repr(C)]
pub(crate) struct HvVcpuExit {
    pub(crate) reason: u32,
    pub(crate) exception: HvVcpuExitException,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Stage1LeafDescriptor {
    pub(crate) level: u8,
    pub(crate) descriptor: u64,
    pub(crate) kind: &'static str,
    pub(crate) output_address: Option<u64>,
    pub(crate) attr_index: u8,
    pub(crate) access_permissions: u8,
    pub(crate) shareability: u8,
    pub(crate) access_flag: bool,
    pub(crate) pxn: bool,
    pub(crate) uxn: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct WindowsArmKnownGuestMemory {
    pub(crate) firmware_memory: *const c_void,
    pub(crate) vars_memory: *const c_void,
    pub(crate) guest_ram_memory: *const c_void,
    pub(crate) guest_ram_bytes: usize,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Stage1TranslationContext {
    pub(crate) tcr_el1: Option<u64>,
    pub(crate) ttbr0_el1: Option<u64>,
    pub(crate) memory: WindowsArmKnownGuestMemory,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Stage1ExitAddresses {
    pub(crate) pc: Option<u64>,
    pub(crate) vbar_el1: Option<u64>,
    pub(crate) elr_el1: Option<u64>,
    pub(crate) far_el1: Option<u64>,
    pub(crate) sp_el1: Option<u64>,
}

impl WindowsArmKnownGuestMemory {
    pub(crate) fn read_u32(self, ipa: u64) -> Option<u32> {
        read_known_guest_phys_u32(
            ipa,
            self.firmware_memory,
            self.vars_memory,
            self.guest_ram_memory,
            self.guest_ram_bytes,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WindowsArmUefiVectorSyncProbe {
    pub(crate) virtual_address: Option<u64>,
    pub(crate) physical_address: Option<u64>,
    pub(crate) instruction_word: Option<u32>,
    pub(crate) instruction_hint: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Stage1VectorSlotInstructions {
    pub(crate) current_el_sp0_sync_instruction_word: Option<u32>,
    pub(crate) current_el_spx_sync_instruction_word: Option<u32>,
    pub(crate) lower_aarch64_sync_instruction_word: Option<u32>,
    pub(crate) lower_aarch32_sync_instruction_word: Option<u32>,
}

impl Stage1VectorSlotInstructions {
    pub(crate) fn populated_slot_count(self) -> u8 {
        [
            self.current_el_sp0_sync_instruction_word,
            self.current_el_spx_sync_instruction_word,
            self.lower_aarch64_sync_instruction_word,
            self.lower_aarch32_sync_instruction_word,
        ]
        .into_iter()
        .filter(|word| vector_slot_instruction_is_populated(*word))
        .count() as u8
    }

    pub(crate) fn current_el_spx_sync_instruction_hint(self) -> &'static str {
        self.current_el_spx_sync_instruction_word
            .map(aarch64_instruction_hint)
            .unwrap_or("not observed")
    }
}

#[link(name = "Hypervisor", kind = "framework")]
extern "C" {
    pub(crate) fn hv_vm_create(config: HvVmConfig) -> HvReturn;
    pub(crate) fn hv_vm_destroy() -> HvReturn;
    pub(crate) fn hv_vcpu_create(
        vcpu: *mut HvVcpu,
        exit: *mut *mut HvVcpuExit,
        config: HvVcpuConfig,
    ) -> HvReturn;
    pub(crate) fn hv_vcpu_destroy(vcpu: HvVcpu) -> HvReturn;
    pub(crate) fn hv_vcpu_get_reg(vcpu: HvVcpu, reg: u32, value: *mut u64) -> HvReturn;
    pub(crate) fn hv_vcpu_set_reg(vcpu: HvVcpu, reg: u32, value: u64) -> HvReturn;
    pub(crate) fn hv_vcpu_get_sys_reg(vcpu: HvVcpu, reg: HvSysReg, value: *mut u64) -> HvReturn;
    pub(crate) fn hv_vcpu_set_sys_reg(vcpu: HvVcpu, reg: HvSysReg, value: u64) -> HvReturn;
    pub(crate) fn mach_absolute_time() -> u64;
    pub(crate) fn hv_vcpu_get_pending_interrupt(
        vcpu: HvVcpu,
        interrupt_type: HvInterruptType,
        pending: *mut bool,
    ) -> HvReturn;
    pub(crate) fn hv_vcpu_set_pending_interrupt(
        vcpu: HvVcpu,
        interrupt_type: HvInterruptType,
        pending: bool,
    ) -> HvReturn;
    pub(crate) fn hv_vcpu_get_vtimer_mask(vcpu: HvVcpu, vtimer_is_masked: *mut bool) -> HvReturn;
    pub(crate) fn hv_vcpu_set_vtimer_mask(vcpu: HvVcpu, vtimer_is_masked: bool) -> HvReturn;
    pub(crate) fn hv_vcpu_get_vtimer_offset(vcpu: HvVcpu, vtimer_offset: *mut u64) -> HvReturn;
    pub(crate) fn hv_vcpu_set_vtimer_offset(vcpu: HvVcpu, vtimer_offset: u64) -> HvReturn;
    pub(crate) fn hv_vcpu_run(vcpu: HvVcpu) -> HvReturn;
    pub(crate) fn hv_vcpus_exit(vcpus: *mut HvVcpu, vcpu_count: u32) -> HvReturn;
    pub(crate) fn hv_vm_allocate(uvap: *mut *mut c_void, size: usize, flags: u64) -> HvReturn;
    pub(crate) fn hv_vm_deallocate(uva: *mut c_void, size: usize) -> HvReturn;
    pub(crate) fn hv_vm_map(addr: *mut c_void, ipa: u64, size: usize, flags: u64) -> HvReturn;
    pub(crate) fn hv_vm_unmap(ipa: u64, size: usize) -> HvReturn;
    pub(crate) fn hv_vm_config_get_default_ipa_size(ipa_bit_length: *mut u32) -> HvReturn;
    pub(crate) fn hv_vm_config_get_max_ipa_size(ipa_bit_length: *mut u32) -> HvReturn;
    pub(crate) fn hv_vm_config_get_el2_supported(el2_supported: *mut bool) -> HvReturn;
}
