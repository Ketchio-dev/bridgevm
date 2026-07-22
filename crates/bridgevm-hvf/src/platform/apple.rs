//! Apple Hypervisor.framework backend for the diagnostic probes.
//!
//! Moved verbatim out of the inline `mod platform` block in lib.rs; this file
//! holds the crate's entire `extern "C"` Hypervisor.framework surface and every
//! `unsafe` site. Selected by the same cfg predicate as before.

use crate::{
    aarch64_instruction_hint, arm_exception_class, arm_exception_class_name,
    build_windows_arm_firmware_run_loop_fdt_blob, complete_probe_virtio_block_request,
    complete_probe_virtio_block_writable_file_requests,
    complete_windows_arm_firmware_block_queue_notify, decode_mmio_data_abort,
    decode_system_register_trap, hv_exit_reason_name, mask_mmio_value, read_be_u32,
    recommended_vector_base_vbar_initial_reason, refresh_windows_arm_firmware_device_irq_pending,
    seed_synthetic_virtio_block_read_request, set_windows_arm_firmware_vtimer_ppi_pending,
    windows_arm_device_mmio_contains, windows_arm_diagnostic_vector_selection,
    windows_arm_firmware_block_devices, windows_arm_firmware_block_irq_source_may_change,
    windows_arm_firmware_block_queue_notify_ipa,
    windows_arm_firmware_gicd_pending_clear_may_need_source_refresh,
    windows_arm_firmware_mmio_bus_with_block_devices, windows_arm_firmware_mmio_device_kind,
    windows_arm_firmware_run_loop_dtb_metadata, windows_arm_firmware_run_loop_exit_diagnosis,
    windows_arm_firmware_run_loop_exit_diagnosis_kind, windows_arm_guest_region_name,
    windows_arm_initial_sp_el1_ipa, GicV3CpuInterfaceAction, GicV3CpuInterfaceIrqLineSnapshot,
    GicV3CpuInterfaceState, HvfGuestEntryProbe, HvfGuestExitLoopProbe, HvfHostCapabilities,
    HvfInterruptTimerProbe, HvfMemoryMapProbe, HvfMmioBlockDeviceProbe, HvfMmioBlockQueueProbe,
    HvfMmioBlockQueueStepProbe, HvfMmioBlockRegisterProbe, HvfMmioReadEmulationProbe,
    HvfMmioReadExitProbe, HvfMmioRtcDeviceProbe, HvfMmioSerialDeviceProbe,
    HvfMmioWriteEmulationProbe, HvfVcpuCreateProbe, HvfVcpuRunProbe, HvfVmCreateProbe,
    HvfVtimerExitProbe, LowVectorDiagnosticPageResumeTelemetry, LowVectorPostRepairTelemetry,
    MmioAccess, MmioAction, MmioBus, Pl011UartDevice, Pl031RtcDevice, VirtioBlockProbeBackingRef,
    VirtioBlockQueueProbeCompletion, VirtioBlockRequestError, VirtioGuestMemory,
    VirtioMmioBlockDevice, WindowsArmFirmwareMmioDeviceKind, WindowsArmFirmwareRunLoopDiagnosis,
    WindowsArmUefiFirmwareRunLoopExecutionOptions, WindowsArmUefiFirmwareRunLoopExit,
    WindowsArmUefiFirmwareRunLoopProbe, WindowsArmUefiPflashHvfMapProbe,
    WindowsArmUefiPflashMapProbe, WindowsArmUefiPflashSlotMap, WindowsArmUefiResetVectorEntryProbe,
    WindowsArmUefiStage1DescriptorSample, WindowsArmUefiStage1ExecutableCandidate,
    WindowsArmUefiStage1WalkEntry, WindowsArmUefiVectorBaseCandidate,
    WindowsArmUefiVectorBaseCandidateScan, WindowsArmUefiVectorBaseRecommendation,
    AARCH64_STAGE1_PAGE_OUTPUT_ADDRESS_MASK, BLOCK_QUEUE_MMIO_DEVICE_MODELS,
    BOOT_MMIO_DEVICE_MODELS, FDT_MAGIC, ICC_DIR_EL1_SYSREG, ICC_EOIR1_EL1_SYSREG,
    ICC_IAR1_EL1_SYSREG, PL011_FR_OFFSET, PL011_UART_MODEL, VIRTIO_MMIO_BLOCK_CAPACITY_HIGH_OFFSET,
    VIRTIO_MMIO_BLOCK_CAPACITY_LOW_OFFSET, VIRTIO_MMIO_BLOCK_CAPACITY_SECTORS,
    VIRTIO_MMIO_BLOCK_CONFIG_GENERATION_VALUE, VIRTIO_MMIO_BLOCK_DEVICE_FEATURES_VALUE,
    VIRTIO_MMIO_BLOCK_DEVICE_ID_VALUE, VIRTIO_MMIO_BLOCK_DRIVER_FEATURES_VALUE,
    VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS, VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS,
    VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS, VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
    VIRTIO_MMIO_BLOCK_QUEUE_NUM_MAX_VALUE, VIRTIO_MMIO_BLOCK_QUEUE_NUM_VALUE,
    VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE, VIRTIO_MMIO_BLOCK_QUEUE_SEL_VALUE,
    VIRTIO_MMIO_BLOCK_STATUS_ACK_VALUE, VIRTIO_MMIO_BLOCK_STATUS_DRIVER_VALUE,
    VIRTIO_MMIO_BLOCK_STATUS_FEATURES_OK_VALUE, VIRTIO_MMIO_BLOCK_STATUS_VALUE,
    VIRTIO_MMIO_CONFIG_GENERATION_OFFSET, VIRTIO_MMIO_DEVICE_FEATURES_OFFSET,
    VIRTIO_MMIO_DEVICE_ID_OFFSET, VIRTIO_MMIO_DRIVER_FEATURES_OFFSET,
    VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET, VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE,
    VIRTIO_MMIO_MAGIC_VALUE, VIRTIO_MMIO_MAGIC_VALUE_OFFSET, VIRTIO_MMIO_QUEUE_DESC_HIGH_OFFSET,
    VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET, VIRTIO_MMIO_QUEUE_DEVICE_HIGH_OFFSET,
    VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET, VIRTIO_MMIO_QUEUE_DRIVER_HIGH_OFFSET,
    VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET, VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET,
    VIRTIO_MMIO_QUEUE_NUM_MAX_OFFSET, VIRTIO_MMIO_QUEUE_NUM_OFFSET, VIRTIO_MMIO_QUEUE_READY_OFFSET,
    VIRTIO_MMIO_QUEUE_SEL_OFFSET, VIRTIO_MMIO_STATUS_OFFSET, VIRTIO_MMIO_VENDOR_ID_OFFSET,
    VIRTIO_MMIO_VENDOR_ID_VALUE, VIRTIO_MMIO_VERSION_OFFSET, VIRTIO_MMIO_VERSION_VALUE,
    WINDOWS_ARM_DIAGNOSTIC_VECTOR_BYTES, WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET,
    WINDOWS_ARM_DIAGNOSTIC_VECTOR_IPA, WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA,
    WINDOWS_ARM_GUEST_RAM_DIAGNOSTIC_VECTOR_IPA, WINDOWS_ARM_GUEST_RAM_IPA,
    WINDOWS_ARM_LOW_VECTOR_DIAGNOSTIC_PAGE_DESCRIPTOR, WINDOWS_ARM_PLATFORM_DTB_GUEST_RAM_OFFSET,
    WINDOWS_ARM_PLATFORM_DTB_IPA, WINDOWS_ARM_UEFI_CODE_IPA, WINDOWS_ARM_UEFI_LOW_CODE_ALIAS_IPA,
    WINDOWS_ARM_UEFI_LOW_VARS_ALIAS_IPA, WINDOWS_ARM_UEFI_SLOT_BYTES, WINDOWS_ARM_UEFI_VARS_IPA,
};
use std::{
    ffi::c_void,
    path::PathBuf,
    ptr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

type HvReturn = i32;
type HvVmConfig = *mut c_void;
type HvVcpuConfig = *mut c_void;
type HvVcpu = u64;
type HvSysReg = u16;
type HvInterruptType = u32;
const HV_SUCCESS: HvReturn = 0;
const HV_EXIT_REASON_CANCELED: u32 = 0;
const HV_EXIT_REASON_EXCEPTION: u32 = 1;
const HV_EXIT_REASON_VTIMER_ACTIVATED: u32 = 2;
const HV_INTERRUPT_TYPE_IRQ: HvInterruptType = 0;
const HV_ALLOCATE_DEFAULT: u64 = 0;
const HV_MEMORY_READ: u64 = 1 << 0;
const HV_MEMORY_WRITE: u64 = 1 << 1;
const HV_MEMORY_EXEC: u64 = 1 << 2;
const PROBE_IPA_START: u64 = 0x4000_0000;
const PROBE_MMIO_IPA: u64 = 0x5000_0000;
const PROBE_BYTES: usize = 16 * 1024;
const HV_REG_X0: u32 = 0;
const HV_REG_X1: u32 = 1;
const HV_REG_X2: u32 = 2;
const HV_REG_X3: u32 = 3;
const HV_REG_X4: u32 = 4;
const HV_REG_PC: u32 = 31;
const HV_REG_CPSR: u32 = 34;
const HV_SYS_REG_SCTLR_EL1: HvSysReg = 0xc080;
const HV_SYS_REG_TTBR0_EL1: HvSysReg = 0xc100;
const HV_SYS_REG_TTBR1_EL1: HvSysReg = 0xc101;
const HV_SYS_REG_TCR_EL1: HvSysReg = 0xc102;
const HV_SYS_REG_SPSR_EL1: HvSysReg = 0xc200;
const HV_SYS_REG_ELR_EL1: HvSysReg = 0xc201;
const HV_SYS_REG_ESR_EL1: HvSysReg = 0xc290;
const HV_SYS_REG_FAR_EL1: HvSysReg = 0xc300;
const HV_SYS_REG_MAIR_EL1: HvSysReg = 0xc510;
const HV_SYS_REG_VBAR_EL1: HvSysReg = 0xc600;
const HV_SYS_REG_CNTV_CTL_EL0: HvSysReg = 0xdf19;
const HV_SYS_REG_CNTV_CVAL_EL0: HvSysReg = 0xdf1a;
const HV_SYS_REG_SP_EL1: HvSysReg = 0xe208;
const AARCH64_PSTATE_EL1H_DAIF_MASKED: u64 = 0x3c5;
const AARCH64_HVC_0: u32 = crate::AARCH64_HVC_0_INSTRUCTION;
const AARCH64_HVC_1: u32 = crate::AARCH64_HVC_1_INSTRUCTION;
const AARCH64_ERET: u32 = crate::AARCH64_ERET_INSTRUCTION;
const DIAGNOSTIC_EXCEPTION_VECTOR_SLOT_BYTES: usize = 12;
const AARCH64_WFI: u32 = 0xd503_207f;
const AARCH64_LDR_X0_FROM_X1: u32 = 0xf940_0020;
const AARCH64_LDR_X0_FROM_X2: u32 = 0xf940_0040;
const AARCH64_LDR_W0_FROM_X1: u32 = 0xb940_0020;
const AARCH64_LDR_W0_FROM_X2: u32 = 0xb940_0040;
const AARCH64_LDR_W0_FROM_X3: u32 = 0xb940_0060;
const AARCH64_LDR_W0_FROM_X4: u32 = 0xb940_0080;
const AARCH64_STR_X0_TO_X1: u32 = 0xf900_0020;
const AARCH64_STR_W0_TO_X1: u32 = 0xb900_0020;
const AARCH64_HVC_0_SYNDROME: u64 = 0x5a00_0000;
const AARCH64_HVC_1_SYNDROME: u64 = 0x5a00_0001;
const EMULATED_MMIO_READ_VALUE: u64 = 0x1234_5678_9abc_def0;
const EMULATED_MMIO_WRITE_VALUE: u64 = 0x0fed_cba9_8765_4321;
const SERIAL_MMIO_DATA_IPA: u64 = PROBE_MMIO_IPA;
const SERIAL_MMIO_STATUS_IPA: u64 = PROBE_MMIO_IPA + PL011_FR_OFFSET;
const SERIAL_MMIO_WRITE_VALUE: u64 = 0x41;
const SERIAL_MMIO_STATUS_VALUE: u64 = 0x90;
const RTC_MMIO_IPA: u64 = PROBE_MMIO_IPA + 0x1000;
const RTC_MMIO_READ_VALUE: u64 = 0x2026_0618;
const BLOCK_MMIO_IPA: u64 = PROBE_MMIO_IPA + 0x2000;
const WINDOWS_ARM_STAGE1_EXECUTABLE_SCAN_STEP: u64 = 2 * 1024 * 1024;
const WINDOWS_ARM_STAGE1_EXECUTABLE_SCAN_MAX_CANDIDATES: usize = 16;
const WINDOWS_ARM_VECTOR_BASE_SCAN_ALIGNMENT: u64 = WINDOWS_ARM_DIAGNOSTIC_VECTOR_BYTES;
const WINDOWS_ARM_VECTOR_BASE_SCAN_MAX_PER_LEAF: usize = 8;
const WINDOWS_ARM_VTIMER_OFFSET_VALUE: u64 = 0x1000;
const WINDOWS_ARM_FIRMWARE_VTIMER_DEADLINE_TICKS: u64 = 50_000_000;

#[repr(C)]
struct HvVcpuExitException {
    syndrome: u64,
    virtual_address: u64,
    physical_address: u64,
}

#[repr(C)]
struct HvVcpuExit {
    reason: u32,
    exception: HvVcpuExitException,
}

#[derive(Debug, Clone, Copy)]
struct Stage1LeafDescriptor {
    level: u8,
    descriptor: u64,
    kind: &'static str,
    output_address: Option<u64>,
    attr_index: u8,
    access_permissions: u8,
    shareability: u8,
    access_flag: bool,
    pxn: bool,
    uxn: bool,
}

#[derive(Debug, Clone, Copy)]
struct WindowsArmKnownGuestMemory {
    firmware_memory: *const c_void,
    vars_memory: *const c_void,
    guest_ram_memory: *const c_void,
    guest_ram_bytes: usize,
}

#[derive(Debug, Clone, Copy)]
struct Stage1TranslationContext {
    tcr_el1: Option<u64>,
    ttbr0_el1: Option<u64>,
    memory: WindowsArmKnownGuestMemory,
}

#[derive(Debug, Clone, Copy)]
struct Stage1ExitAddresses {
    pc: Option<u64>,
    vbar_el1: Option<u64>,
    elr_el1: Option<u64>,
    far_el1: Option<u64>,
    sp_el1: Option<u64>,
}

impl WindowsArmKnownGuestMemory {
    fn read_u32(self, ipa: u64) -> Option<u32> {
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
struct WindowsArmUefiVectorSyncProbe {
    virtual_address: Option<u64>,
    physical_address: Option<u64>,
    instruction_word: Option<u32>,
    instruction_hint: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stage1VectorSlotInstructions {
    current_el_sp0_sync_instruction_word: Option<u32>,
    current_el_spx_sync_instruction_word: Option<u32>,
    lower_aarch64_sync_instruction_word: Option<u32>,
    lower_aarch32_sync_instruction_word: Option<u32>,
}

impl Stage1VectorSlotInstructions {
    fn populated_slot_count(self) -> u8 {
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

    fn current_el_spx_sync_instruction_hint(self) -> &'static str {
        self.current_el_spx_sync_instruction_word
            .map(aarch64_instruction_hint)
            .unwrap_or("not observed")
    }
}

#[link(name = "Hypervisor", kind = "framework")]
extern "C" {
    fn hv_vm_create(config: HvVmConfig) -> HvReturn;
    fn hv_vm_destroy() -> HvReturn;
    fn hv_vcpu_create(
        vcpu: *mut HvVcpu,
        exit: *mut *mut HvVcpuExit,
        config: HvVcpuConfig,
    ) -> HvReturn;
    fn hv_vcpu_destroy(vcpu: HvVcpu) -> HvReturn;
    fn hv_vcpu_get_reg(vcpu: HvVcpu, reg: u32, value: *mut u64) -> HvReturn;
    fn hv_vcpu_set_reg(vcpu: HvVcpu, reg: u32, value: u64) -> HvReturn;
    fn hv_vcpu_get_sys_reg(vcpu: HvVcpu, reg: HvSysReg, value: *mut u64) -> HvReturn;
    fn hv_vcpu_set_sys_reg(vcpu: HvVcpu, reg: HvSysReg, value: u64) -> HvReturn;
    fn mach_absolute_time() -> u64;
    fn hv_vcpu_get_pending_interrupt(
        vcpu: HvVcpu,
        interrupt_type: HvInterruptType,
        pending: *mut bool,
    ) -> HvReturn;
    fn hv_vcpu_set_pending_interrupt(
        vcpu: HvVcpu,
        interrupt_type: HvInterruptType,
        pending: bool,
    ) -> HvReturn;
    fn hv_vcpu_get_vtimer_mask(vcpu: HvVcpu, vtimer_is_masked: *mut bool) -> HvReturn;
    fn hv_vcpu_set_vtimer_mask(vcpu: HvVcpu, vtimer_is_masked: bool) -> HvReturn;
    fn hv_vcpu_get_vtimer_offset(vcpu: HvVcpu, vtimer_offset: *mut u64) -> HvReturn;
    fn hv_vcpu_set_vtimer_offset(vcpu: HvVcpu, vtimer_offset: u64) -> HvReturn;
    fn hv_vcpu_run(vcpu: HvVcpu) -> HvReturn;
    fn hv_vcpus_exit(vcpus: *mut HvVcpu, vcpu_count: u32) -> HvReturn;
    fn hv_vm_allocate(uvap: *mut *mut c_void, size: usize, flags: u64) -> HvReturn;
    fn hv_vm_deallocate(uva: *mut c_void, size: usize) -> HvReturn;
    fn hv_vm_map(addr: *mut c_void, ipa: u64, size: usize, flags: u64) -> HvReturn;
    fn hv_vm_unmap(ipa: u64, size: usize) -> HvReturn;
    fn hv_vm_config_get_default_ipa_size(ipa_bit_length: *mut u32) -> HvReturn;
    fn hv_vm_config_get_max_ipa_size(ipa_bit_length: *mut u32) -> HvReturn;
    fn hv_vm_config_get_el2_supported(el2_supported: *mut bool) -> HvReturn;
}

pub fn query_hvf_host_capabilities() -> HvfHostCapabilities {
    let mut blockers = Vec::new();
    let default_ipa_bits = query_u32(
        hv_vm_config_get_default_ipa_size,
        "default IPA size",
        &mut blockers,
    );
    let max_ipa_bits = query_u32(hv_vm_config_get_max_ipa_size, "max IPA size", &mut blockers);
    let el2_supported = query_bool(hv_vm_config_get_el2_supported, "EL2 support", &mut blockers);
    HvfHostCapabilities {
        available: blockers.is_empty() || default_ipa_bits.is_some() || max_ipa_bits.is_some(),
        host: "macos-aarch64",
        default_ipa_bits,
        max_ipa_bits,
        el2_supported,
        blockers,
    }
}

fn query_u32(
    query: unsafe extern "C" fn(*mut u32) -> HvReturn,
    label: &str,
    blockers: &mut Vec<String>,
) -> Option<u32> {
    let mut value = 0;
    let status = unsafe { query(&mut value) };
    if status == HV_SUCCESS {
        Some(value)
    } else {
        blockers.push(format!(
            "Hypervisor.framework {label} query failed: {status:#x}"
        ));
        None
    }
}

fn query_bool(
    query: unsafe extern "C" fn(*mut bool) -> HvReturn,
    label: &str,
    blockers: &mut Vec<String>,
) -> Option<bool> {
    let mut value = false;
    let status = unsafe { query(&mut value) };
    if status == HV_SUCCESS {
        Some(value)
    } else {
        blockers.push(format!(
            "Hypervisor.framework {label} query failed: {status:#x}"
        ));
        None
    }
}

fn read_vcpu_reg(vcpu: HvVcpu, register: u32) -> Option<u64> {
    let mut value = 0;
    let status = unsafe { hv_vcpu_get_reg(vcpu, register, &mut value) };
    (status == HV_SUCCESS).then_some(value)
}

fn read_vcpu_sys_reg(vcpu: HvVcpu, register: HvSysReg) -> Option<u64> {
    let mut value = 0;
    let status = unsafe { hv_vcpu_get_sys_reg(vcpu, register, &mut value) };
    (status == HV_SUCCESS).then_some(value)
}

fn firmware_vtimer_deadline(offset: u64) -> u64 {
    unsafe { mach_absolute_time() }
        .saturating_sub(offset)
        .saturating_add(WINDOWS_ARM_FIRMWARE_VTIMER_DEADLINE_TICKS)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WindowsArmFirmwareIrqLineDelivery {
    irq_line_snapshot: GicV3CpuInterfaceIrqLineSnapshot,
    irq_line_should_assert: bool,
    pending_irq_status: Option<HvReturn>,
    next_device_irq_line_asserted: bool,
    device_irq_injected: bool,
    device_irq_cleared: bool,
}

impl WindowsArmFirmwareIrqLineDelivery {
    fn succeeded(self) -> bool {
        match self.pending_irq_status {
            Some(status) => status == HV_SUCCESS,
            None => true,
        }
    }

    fn failure_blocker(self, exit_index: u32) -> String {
        format!(
            "firmware run-loop GIC CPU-interface IRQ line refresh failed on exit {exit_index}: desired_pending={}, gic_group1_enabled={}, gic_priority_mask={:#x}, gic_running_priority={:#x}, gic_priority_threshold={:#x}, gic_pending_intid={}, hv_vcpu_set_pending_interrupt={:#x}",
            self.irq_line_should_assert,
            self.irq_line_snapshot.group1_enabled,
            self.irq_line_snapshot.priority_mask,
            self.irq_line_snapshot.running_priority,
            self.irq_line_snapshot.priority_threshold,
            self.irq_line_snapshot.pending_intid,
            self.pending_irq_status.unwrap_or(HV_SUCCESS)
        )
    }
}

fn service_windows_arm_firmware_gic_irq_line_delivery(
    vcpu: HvVcpu,
    bus: &mut MmioBus,
    block_devices: &[crate::WindowsArmVirtioBlockDeviceMetadata],
    gic_cpu_interface: &GicV3CpuInterfaceState,
    device_irq_line_asserted: bool,
    refresh_level_sources: bool,
) -> WindowsArmFirmwareIrqLineDelivery {
    if refresh_level_sources {
        let _ = refresh_windows_arm_firmware_device_irq_pending(bus, block_devices);
    }

    let irq_line_snapshot = gic_cpu_interface.irq_line_snapshot(bus);
    let irq_line_should_assert = irq_line_snapshot.irq_line_should_assert;
    let mut next_device_irq_line_asserted = device_irq_line_asserted;
    let mut device_irq_injected = false;
    let mut device_irq_cleared = false;
    let pending_irq_status = if irq_line_should_assert != device_irq_line_asserted {
        let status = unsafe {
            hv_vcpu_set_pending_interrupt(vcpu, HV_INTERRUPT_TYPE_IRQ, irq_line_should_assert)
        };
        if status == HV_SUCCESS {
            next_device_irq_line_asserted = irq_line_should_assert;
            if irq_line_should_assert {
                device_irq_injected = true;
            } else {
                device_irq_cleared = true;
            }
        }
        Some(status)
    } else {
        None
    };

    WindowsArmFirmwareIrqLineDelivery {
        irq_line_snapshot,
        irq_line_should_assert,
        pending_irq_status,
        next_device_irq_line_asserted,
        device_irq_injected,
        device_irq_cleared,
    }
}

fn record_windows_arm_firmware_irq_line_delivery(
    delivery: WindowsArmFirmwareIrqLineDelivery,
    device_irq_line_asserted: &mut bool,
    last_device_irq_set_status: &mut Option<HvReturn>,
    last_device_irq_clear_status: &mut Option<HvReturn>,
    device_irq_injected_count: &mut u32,
    device_irq_cleared_count: &mut u32,
) {
    if let Some(status) = delivery.pending_irq_status {
        if delivery.irq_line_should_assert {
            *last_device_irq_set_status = Some(status);
        } else {
            *last_device_irq_clear_status = Some(status);
        }
    }
    if delivery.device_irq_injected {
        *device_irq_injected_count += 1;
    }
    if delivery.device_irq_cleared {
        *device_irq_cleared_count += 1;
    }
    *device_irq_line_asserted = delivery.next_device_irq_line_asserted;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WindowsArmFirmwareVtimerDelivery {
    rearm_cval_value: u64,
    rearm_cval_status: HvReturn,
    ppi_pending_recorded: bool,
    irq_line_snapshot: GicV3CpuInterfaceIrqLineSnapshot,
    irq_line_should_assert: bool,
    pending_irq_status: Option<HvReturn>,
    unmask_status: Option<HvReturn>,
    next_device_irq_line_asserted: bool,
    device_irq_injected: bool,
    device_irq_cleared: bool,
}

impl WindowsArmFirmwareVtimerDelivery {
    fn irq_effective_status(self) -> HvReturn {
        self.pending_irq_status.unwrap_or(HV_SUCCESS)
    }

    fn pending_irq_injected(self) -> bool {
        self.irq_line_should_assert
    }

    fn succeeded(self) -> bool {
        self.rearm_cval_status == HV_SUCCESS
            && self.ppi_pending_recorded
            && self.irq_effective_status() == HV_SUCCESS
            && self.unmask_status.unwrap_or(HV_SUCCESS) == HV_SUCCESS
    }

    fn failure_blocker(self, exit_index: u32) -> String {
        format!(
            "firmware run-loop failed to service VTimer exit {exit_index}: hv_vcpu_set_sys_reg(CNTV_CVAL_EL0)={:#x}, timer_ppi_pending_recorded={}, gic_group1_enabled={}, gic_priority_mask={:#x}, gic_running_priority={:#x}, gic_priority_threshold={:#x}, gic_pending_intid={}, hv_vcpu_set_pending_interrupt(IRQ={})={:#x}, hv_vcpu_set_vtimer_mask(false)={}",
            self.rearm_cval_status,
            self.ppi_pending_recorded,
            self.irq_line_snapshot.group1_enabled,
            self.irq_line_snapshot.priority_mask,
            self.irq_line_snapshot.running_priority,
            self.irq_line_snapshot.priority_threshold,
            self.irq_line_snapshot.pending_intid,
            self.irq_line_should_assert,
            self.irq_effective_status(),
            crate::render_optional_status(self.unmask_status)
        )
    }
}

fn service_windows_arm_firmware_vtimer_delivery(
    vcpu: HvVcpu,
    bus: &mut MmioBus,
    gic_cpu_interface: &GicV3CpuInterfaceState,
    device_irq_line_asserted: bool,
    unmask_without_assertable_irq: bool,
) -> WindowsArmFirmwareVtimerDelivery {
    let rearm_cval_value = firmware_vtimer_deadline(WINDOWS_ARM_VTIMER_OFFSET_VALUE);
    let rearm_cval_status =
        unsafe { hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_CNTV_CVAL_EL0, rearm_cval_value) };
    let ppi_pending_recorded = set_windows_arm_firmware_vtimer_ppi_pending(bus, true);
    let irq_line_snapshot = gic_cpu_interface.irq_line_snapshot(bus);
    let irq_line_should_assert = irq_line_snapshot.irq_line_should_assert;

    let mut next_device_irq_line_asserted = device_irq_line_asserted;
    let mut device_irq_injected = false;
    let device_irq_cleared = false;
    let pending_irq_status =
        if irq_line_should_assert && irq_line_should_assert != device_irq_line_asserted {
            let status = unsafe {
                hv_vcpu_set_pending_interrupt(vcpu, HV_INTERRUPT_TYPE_IRQ, irq_line_should_assert)
            };
            if status == HV_SUCCESS {
                next_device_irq_line_asserted = true;
                device_irq_injected = true;
            }
            Some(status)
        } else {
            None
        };
    let unmask_status = if irq_line_should_assert || unmask_without_assertable_irq {
        let status = unsafe { hv_vcpu_set_vtimer_mask(vcpu, false) };
        Some(status)
    } else {
        None
    };

    WindowsArmFirmwareVtimerDelivery {
        rearm_cval_value,
        rearm_cval_status,
        ppi_pending_recorded,
        irq_line_snapshot,
        irq_line_should_assert,
        pending_irq_status,
        unmask_status,
        next_device_irq_line_asserted,
        device_irq_injected,
        device_irq_cleared,
    }
}

fn read_guest_instruction_word(
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

fn read_known_guest_phys_u32(
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
struct DiagnosticVectorRoute {
    vbar_el1: u64,
    sync_pc: u64,
}

impl DiagnosticVectorRoute {
    fn eret_pc(self) -> u64 {
        self.sync_pc + 4
    }

    fn landing_pc(self) -> u64 {
        self.sync_pc + 8
    }

    fn stop_pc(self) -> u64 {
        self.sync_pc + 12
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DiagnosticVectorEretRouteStatus {
    elr_status: HvReturn,
    pc_status: HvReturn,
}

impl DiagnosticVectorEretRouteStatus {
    fn succeeded(self) -> bool {
        self.elr_status == HV_SUCCESS && self.pc_status == HV_SUCCESS
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DiagnosticVectorOriginalContextResumeStatus {
    elr_status: HvReturn,
    vbar_status: Option<HvReturn>,
    spsr_status: HvReturn,
    pc_status: HvReturn,
}

impl DiagnosticVectorOriginalContextResumeStatus {
    fn vbar_effective_status(self) -> HvReturn {
        Self::effective_vbar_status(self.elr_status, self.vbar_status)
    }

    fn effective_vbar_status(elr_status: HvReturn, vbar_status: Option<HvReturn>) -> HvReturn {
        vbar_status.unwrap_or({
            if elr_status == HV_SUCCESS {
                HV_SUCCESS
            } else {
                elr_status
            }
        })
    }

    fn succeeded(self) -> bool {
        self.elr_status == HV_SUCCESS
            && self.vbar_effective_status() == HV_SUCCESS
            && self.spsr_status == HV_SUCCESS
            && self.pc_status == HV_SUCCESS
    }
}

fn route_diagnostic_hvc_exit_through_eret_landing(
    vcpu: HvVcpu,
    eret_pc: u64,
    landing_pc: u64,
) -> DiagnosticVectorEretRouteStatus {
    let elr_status = unsafe { hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_ELR_EL1, landing_pc) };
    let pc_status = if elr_status == HV_SUCCESS {
        unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, eret_pc) }
    } else {
        elr_status
    };
    DiagnosticVectorEretRouteStatus {
        elr_status,
        pc_status,
    }
}

fn resume_diagnostic_eret_to_original_context(
    vcpu: HvVcpu,
    original_elr_el1: u64,
    original_spsr_el1: u64,
    eret_pc: u64,
    reset_vbar_el1: bool,
) -> DiagnosticVectorOriginalContextResumeStatus {
    let elr_status = unsafe { hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_ELR_EL1, original_elr_el1) };
    let vbar_status = if reset_vbar_el1 && elr_status == HV_SUCCESS {
        Some(unsafe { hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_VBAR_EL1, 0) })
    } else {
        None
    };
    let vbar_effective_status =
        DiagnosticVectorOriginalContextResumeStatus::effective_vbar_status(elr_status, vbar_status);
    let spsr_status = if elr_status == HV_SUCCESS && vbar_effective_status == HV_SUCCESS {
        unsafe { hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_SPSR_EL1, original_spsr_el1) }
    } else {
        vbar_effective_status
    };
    let pc_status = if elr_status == HV_SUCCESS
        && vbar_effective_status == HV_SUCCESS
        && spsr_status == HV_SUCCESS
    {
        unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, eret_pc) }
    } else {
        spsr_status
    };
    DiagnosticVectorOriginalContextResumeStatus {
        elr_status,
        vbar_status,
        spsr_status,
        pc_status,
    }
}

fn arm_diagnostic_eret_resume(
    vcpu: HvVcpu,
    resume: &mut LowVectorDiagnosticPageResumeTelemetry,
    original_elr_el1: u64,
    original_spsr_el1: u64,
    eret_pc: u64,
) -> DiagnosticVectorOriginalContextResumeStatus {
    let status = resume_diagnostic_eret_to_original_context(
        vcpu,
        original_elr_el1,
        original_spsr_el1,
        eret_pc,
        false,
    );
    resume.record_eret_resume_status(status.elr_status, status.spsr_status, status.pc_status);
    if status.succeeded() {
        resume.mark_armed();
    }
    status
}

#[cfg(test)]
mod diagnostic_vector_resume_tests {
    use super::*;

    #[test]
    fn original_context_resume_status_treats_unrequested_vbar_as_success() {
        let status = DiagnosticVectorOriginalContextResumeStatus {
            elr_status: HV_SUCCESS,
            vbar_status: None,
            spsr_status: HV_SUCCESS,
            pc_status: HV_SUCCESS,
        };

        assert_eq!(status.vbar_effective_status(), HV_SUCCESS);
        assert!(status.succeeded());
    }

    #[test]
    fn original_context_resume_status_reports_elr_and_vbar_failures() {
        let vbar_failed_status = DiagnosticVectorOriginalContextResumeStatus {
            elr_status: HV_SUCCESS,
            vbar_status: Some(0x2),
            spsr_status: HV_SUCCESS,
            pc_status: HV_SUCCESS,
        };
        let elr_failed_status = DiagnosticVectorOriginalContextResumeStatus {
            elr_status: -1,
            vbar_status: None,
            spsr_status: -1,
            pc_status: -1,
        };

        assert_eq!(vbar_failed_status.vbar_effective_status(), 0x2);
        assert!(!vbar_failed_status.succeeded());
        assert_eq!(elr_failed_status.vbar_effective_status(), -1);
        assert!(!elr_failed_status.succeeded());
    }
}

fn executable_diagnostic_vector_route() -> DiagnosticVectorRoute {
    DiagnosticVectorRoute {
        vbar_el1: WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA,
        sync_pc: WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA
            + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64,
    }
}

fn low_vector_diagnostic_page_route() -> DiagnosticVectorRoute {
    DiagnosticVectorRoute {
        vbar_el1: 0,
        sync_pc: WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64,
    }
}

fn recommended_vector_base_diagnostic_route(vbar_el1: u64) -> DiagnosticVectorRoute {
    DiagnosticVectorRoute {
        vbar_el1,
        sync_pc: vbar_el1 + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64,
    }
}

fn diagnostic_vector_hvc_eret_recovery_target(
    exit: &WindowsArmUefiFirmwareRunLoopExit,
    route: DiagnosticVectorRoute,
) -> Option<(u64, u64)> {
    let eret_pc = route.eret_pc();
    let landing_pc = route.landing_pc();
    let vbar_matches = exit.vbar_el1_after_exit == Some(route.vbar_el1)
        || (route.vbar_el1 == 0
            && exit.pc_stage1_leaf_descriptor_after_exit
                == Some(WINDOWS_ARM_LOW_VECTOR_DIAGNOSTIC_PAGE_DESCRIPTOR));
    (exit.run_status == Some(HV_SUCCESS)
        && exit.exit_reason == Some(HV_EXIT_REASON_EXCEPTION)
        && exit.exit_syndrome == Some(AARCH64_HVC_1_SYNDROME)
        && vbar_matches
        && exit.pc_after_exit == Some(eret_pc)
        && exit.instruction_word_after_exit == Some(AARCH64_ERET))
    .then_some((eret_pc, landing_pc))
}

fn diagnostic_vector_eret_landing_stop(
    exit: &WindowsArmUefiFirmwareRunLoopExit,
    route: DiagnosticVectorRoute,
) -> bool {
    let vbar_matches = exit.vbar_el1_after_exit == Some(route.vbar_el1)
        || (route.vbar_el1 == 0
            && exit.pc_stage1_leaf_descriptor_after_exit
                == Some(WINDOWS_ARM_LOW_VECTOR_DIAGNOSTIC_PAGE_DESCRIPTOR));
    exit.run_status == Some(HV_SUCCESS)
        && exit.exit_reason == Some(HV_EXIT_REASON_EXCEPTION)
        && exit.exit_syndrome == Some(AARCH64_HVC_0_SYNDROME)
        && vbar_matches
        && exit.pc_after_exit == Some(route.stop_pc())
}

fn executable_diagnostic_vector_hvc_eret_recovery_target(
    exit: &WindowsArmUefiFirmwareRunLoopExit,
) -> Option<(u64, u64)> {
    diagnostic_vector_hvc_eret_recovery_target(exit, executable_diagnostic_vector_route())
}

fn executable_diagnostic_vector_eret_landing_stop(
    exit: &WindowsArmUefiFirmwareRunLoopExit,
) -> bool {
    diagnostic_vector_eret_landing_stop(exit, executable_diagnostic_vector_route())
}

fn low_vector_diagnostic_page_hvc_eret_recovery_target(
    exit: &WindowsArmUefiFirmwareRunLoopExit,
) -> Option<(u64, u64)> {
    diagnostic_vector_hvc_eret_recovery_target(exit, low_vector_diagnostic_page_route())
}

fn low_vector_diagnostic_page_eret_landing_stop(exit: &WindowsArmUefiFirmwareRunLoopExit) -> bool {
    diagnostic_vector_eret_landing_stop(exit, low_vector_diagnostic_page_route())
}

fn read_stage1_leaf_descriptor(
    va: Option<u64>,
    tcr_el1: Option<u64>,
    ttbr0_el1: Option<u64>,
    firmware_memory: *const c_void,
    vars_memory: *const c_void,
    guest_ram_memory: *const c_void,
    guest_ram_bytes: usize,
) -> Option<Stage1LeafDescriptor> {
    let va = va?;
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
        let descriptor = read_known_guest_phys_u64(
            entry_ipa,
            firmware_memory,
            vars_memory,
            guest_ram_memory,
            guest_ram_bytes,
        )?;
        let kind = stage1_descriptor_kind(descriptor, level as u8);
        if kind == "table" {
            table_ipa = descriptor & 0x0000_ffff_ffff_f000;
            continue;
        }
        return Some(Stage1LeafDescriptor {
            level: level as u8,
            descriptor,
            kind,
            output_address: stage1_descriptor_output_address(descriptor, level as u8, kind),
            attr_index: ((descriptor >> 2) & 0x7) as u8,
            access_permissions: ((descriptor >> 6) & 0x3) as u8,
            shareability: ((descriptor >> 8) & 0x3) as u8,
            access_flag: descriptor & (1 << 10) != 0,
            pxn: descriptor & (1 << 53) != 0,
            uxn: descriptor & (1 << 54) != 0,
        });
    }
    None
}

fn collect_stage1_descriptor_samples(
    addresses: Stage1ExitAddresses,
    translation: Stage1TranslationContext,
) -> Vec<WindowsArmUefiStage1DescriptorSample> {
    let mut requests = vec![
        ("low-vector-base", Some(WINDOWS_ARM_UEFI_LOW_CODE_ALIAS_IPA)),
        (
            "low-vector-sync-slot",
            Some(
                WINDOWS_ARM_UEFI_LOW_CODE_ALIAS_IPA
                    + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64,
            ),
        ),
        ("firmware-reset-vector", Some(WINDOWS_ARM_UEFI_CODE_IPA)),
        (
            "pflash-diagnostic-vector-sync-slot",
            Some(
                WINDOWS_ARM_DIAGNOSTIC_VECTOR_IPA
                    + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64,
            ),
        ),
        (
            "guest-ram-diagnostic-vector-sync-slot",
            Some(
                WINDOWS_ARM_GUEST_RAM_DIAGNOSTIC_VECTOR_IPA
                    + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64,
            ),
        ),
        (
            "executable-diagnostic-vector-sync-slot",
            Some(
                WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA
                    + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64,
            ),
        ),
        ("pc-after-exit", addresses.pc),
        ("vbar-el1", addresses.vbar_el1),
        ("elr-el1", addresses.elr_el1),
        ("far-el1", addresses.far_el1),
        ("sp-el1", addresses.sp_el1),
    ];
    requests.retain(|(_, va)| va.is_some());
    requests
        .into_iter()
        .map(|(label, va)| {
            stage1_descriptor_sample(
                label,
                va.expect("stage-1 descriptor sample VA is retained as Some"),
                translation,
            )
        })
        .collect()
}

fn collect_stage1_walk_entries(
    addresses: Stage1ExitAddresses,
    translation: Stage1TranslationContext,
) -> Vec<WindowsArmUefiStage1WalkEntry> {
    let mut requests = vec![
        (
            "low-vector-sync-slot",
            Some(
                WINDOWS_ARM_UEFI_LOW_CODE_ALIAS_IPA
                    + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64,
            ),
        ),
        ("pc-after-exit", addresses.pc),
        ("vbar-el1", addresses.vbar_el1),
        ("elr-el1", addresses.elr_el1),
        ("far-el1", addresses.far_el1),
        ("sp-el1", addresses.sp_el1),
        (
            "executable-diagnostic-vector-sync-slot",
            Some(
                WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA
                    + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64,
            ),
        ),
    ];
    requests.retain(|(_, va)| va.is_some());
    let mut entries = Vec::new();
    for (label, va) in requests {
        entries.extend(stage1_walk_entries_for_address(
            label,
            va.expect("stage-1 walk VA is retained as Some"),
            translation,
        ));
    }
    entries
}

fn stage1_walk_entries_for_address(
    label: &'static str,
    virtual_address: u64,
    translation: Stage1TranslationContext,
) -> Vec<WindowsArmUefiStage1WalkEntry> {
    let Some(tcr) = translation.tcr_el1 else {
        return Vec::new();
    };
    let Some(ttbr0) = translation.ttbr0_el1 else {
        return Vec::new();
    };
    let tg0 = (tcr >> 14) & 0x3;
    if tg0 != 0 {
        return Vec::new();
    }
    let t0sz = tcr & 0x3f;
    if t0sz > 48 {
        return Vec::new();
    }
    let va_bits = 64 - t0sz;
    let start_level = match va_bits {
        40..=64 => 0,
        31..=39 => 1,
        22..=30 => 2,
        _ => 3,
    };
    let mut table_ipa = ttbr0 & 0x0000_ffff_ffff_f000;
    let mut entries = Vec::new();
    for level in start_level..=3 {
        let shift = 39u32.saturating_sub(level as u32 * 9);
        let index = (virtual_address >> shift) & 0x1ff;
        let Some(entry_ipa) = table_ipa.checked_add(index.saturating_mul(8)) else {
            break;
        };
        let descriptor = read_known_guest_phys_u64(
            entry_ipa,
            translation.memory.firmware_memory,
            translation.memory.vars_memory,
            translation.memory.guest_ram_memory,
            translation.memory.guest_ram_bytes,
        );
        let descriptor_kind = descriptor
            .map(|descriptor| stage1_descriptor_kind(descriptor, level as u8))
            .unwrap_or("not observed");
        let next_table_ipa = descriptor
            .filter(|_| descriptor_kind == "table")
            .map(|descriptor| descriptor & 0x0000_ffff_ffff_f000);
        entries.push(WindowsArmUefiStage1WalkEntry {
            label,
            virtual_address,
            region: windows_arm_guest_region_name(
                Some(virtual_address),
                translation.memory.guest_ram_bytes as u64,
            ),
            level: level as u8,
            table_ipa,
            index,
            entry_ipa,
            descriptor,
            descriptor_kind,
            next_table_ipa,
            output_address: descriptor.and_then(|descriptor| {
                stage1_descriptor_output_address(descriptor, level as u8, descriptor_kind)
            }),
            attr_index: descriptor.map(|descriptor| ((descriptor >> 2) & 0x7) as u8),
            access_permissions: descriptor.map(|descriptor| ((descriptor >> 6) & 0x3) as u8),
            shareability: descriptor.map(|descriptor| ((descriptor >> 8) & 0x3) as u8),
            access_flag: descriptor.map(|descriptor| descriptor & (1 << 10) != 0),
            pxn: descriptor.map(|descriptor| descriptor & (1 << 53) != 0),
            uxn: descriptor.map(|descriptor| descriptor & (1 << 54) != 0),
        });
        if let Some(next_table_ipa) = next_table_ipa {
            table_ipa = next_table_ipa;
            continue;
        }
        break;
    }
    entries
}

fn stage1_descriptor_sample(
    label: &'static str,
    virtual_address: u64,
    translation: Stage1TranslationContext,
) -> WindowsArmUefiStage1DescriptorSample {
    let leaf = read_stage1_leaf_descriptor(
        Some(virtual_address),
        translation.tcr_el1,
        translation.ttbr0_el1,
        translation.memory.firmware_memory,
        translation.memory.vars_memory,
        translation.memory.guest_ram_memory,
        translation.memory.guest_ram_bytes,
    );
    WindowsArmUefiStage1DescriptorSample {
        label,
        virtual_address,
        region: windows_arm_guest_region_name(
            Some(virtual_address),
            translation.memory.guest_ram_bytes as u64,
        ),
        level: leaf.map(|leaf| leaf.level),
        descriptor: leaf.map(|leaf| leaf.descriptor),
        descriptor_kind: leaf.map(|leaf| leaf.kind).unwrap_or("not observed"),
        output_address: leaf.and_then(|leaf| leaf.output_address),
        attr_index: leaf.map(|leaf| leaf.attr_index),
        access_permissions: leaf.map(|leaf| leaf.access_permissions),
        shareability: leaf.map(|leaf| leaf.shareability),
        access_flag: leaf.map(|leaf| leaf.access_flag),
        pxn: leaf.map(|leaf| leaf.pxn),
        uxn: leaf.map(|leaf| leaf.uxn),
    }
}

fn collect_stage1_executable_candidates(
    tcr_el1_after_exit: Option<u64>,
    ttbr0_el1_after_exit: Option<u64>,
    firmware_memory: *const c_void,
    vars_memory: *const c_void,
    guest_ram_memory: *const c_void,
    guest_ram_bytes: usize,
) -> Vec<WindowsArmUefiStage1ExecutableCandidate> {
    let memory = WindowsArmKnownGuestMemory {
        firmware_memory,
        vars_memory,
        guest_ram_memory,
        guest_ram_bytes,
    };
    let mut candidates = Vec::new();
    for (start, bytes) in stage1_executable_scan_ranges(guest_ram_bytes) {
        let Some(end) = start.checked_add(bytes) else {
            continue;
        };
        let mut va = start;
        while va < end && candidates.len() < WINDOWS_ARM_STAGE1_EXECUTABLE_SCAN_MAX_CANDIDATES {
            if let Some(leaf) = read_stage1_leaf_descriptor(
                Some(va),
                tcr_el1_after_exit,
                ttbr0_el1_after_exit,
                memory.firmware_memory,
                memory.vars_memory,
                memory.guest_ram_memory,
                guest_ram_bytes,
            ) {
                if stage1_leaf_is_el1_executable(leaf)
                    && !stage1_executable_leaf_already_reported(&candidates, leaf)
                {
                    candidates.push(build_stage1_executable_candidate(leaf, va, memory));
                }
            }
            va = match va.checked_add(WINDOWS_ARM_STAGE1_EXECUTABLE_SCAN_STEP) {
                Some(next) => next,
                None => break,
            };
        }
        if candidates.len() >= WINDOWS_ARM_STAGE1_EXECUTABLE_SCAN_MAX_CANDIDATES {
            break;
        }
    }
    candidates
}

fn stage1_executable_scan_ranges(guest_ram_bytes: usize) -> [(u64, u64); 5] {
    [
        (
            WINDOWS_ARM_UEFI_LOW_CODE_ALIAS_IPA,
            WINDOWS_ARM_UEFI_SLOT_BYTES,
        ),
        (
            WINDOWS_ARM_UEFI_LOW_VARS_ALIAS_IPA,
            WINDOWS_ARM_UEFI_SLOT_BYTES,
        ),
        (WINDOWS_ARM_UEFI_CODE_IPA, WINDOWS_ARM_UEFI_SLOT_BYTES),
        (WINDOWS_ARM_UEFI_VARS_IPA, WINDOWS_ARM_UEFI_SLOT_BYTES),
        (WINDOWS_ARM_GUEST_RAM_IPA, guest_ram_bytes as u64),
    ]
}

fn stage1_leaf_is_el1_executable(leaf: Stage1LeafDescriptor) -> bool {
    matches!(leaf.kind, "block" | "page") && !leaf.pxn
}

fn stage1_executable_leaf_already_reported(
    candidates: &[WindowsArmUefiStage1ExecutableCandidate],
    leaf: Stage1LeafDescriptor,
) -> bool {
    candidates.iter().any(|candidate| {
        candidate.descriptor == leaf.descriptor && candidate.output_address == leaf.output_address
    })
}

fn build_stage1_executable_candidate(
    leaf: Stage1LeafDescriptor,
    virtual_address: u64,
    memory: WindowsArmKnownGuestMemory,
) -> WindowsArmUefiStage1ExecutableCandidate {
    let vector_sync = collect_stage1_vector_sync_probe_for_leaf(leaf, virtual_address, memory);
    let vector_base_scan =
        collect_stage1_vector_base_candidates_for_leaf(leaf, virtual_address, memory);
    let recommended_vector_base_candidate =
        recommend_stage1_vector_base_candidate(&vector_base_scan.candidates).or_else(|| {
            recommend_stage1_executable_leaf_base_vector(leaf, virtual_address, vector_sync)
        });
    WindowsArmUefiStage1ExecutableCandidate {
        virtual_address,
        region: windows_arm_guest_region_name(Some(virtual_address), memory.guest_ram_bytes as u64),
        level: leaf.level,
        descriptor: leaf.descriptor,
        descriptor_kind: leaf.kind,
        output_address: leaf.output_address,
        span_bytes: stage1_descriptor_span_bytes(leaf.level, leaf.kind),
        vector_sync_virtual_address: vector_sync.virtual_address,
        vector_sync_physical_address: vector_sync.physical_address,
        vector_sync_instruction_word: vector_sync.instruction_word,
        vector_sync_instruction_hint: vector_sync.instruction_hint,
        vector_base_scan_scanned_count: vector_base_scan.scanned_count,
        vector_base_scan_suppressed_count: vector_base_scan.suppressed_count,
        vector_base_scan_limit_reached: vector_base_scan.limit_reached,
        recommended_vector_base_candidate,
        vector_base_candidates: vector_base_scan.candidates,
        attr_index: leaf.attr_index,
        access_permissions: leaf.access_permissions,
        shareability: leaf.shareability,
        access_flag: leaf.access_flag,
        pxn: leaf.pxn,
        uxn: leaf.uxn,
    }
}

fn collect_stage1_vector_sync_probe_for_leaf(
    leaf: Stage1LeafDescriptor,
    leaf_sample_virtual_address: u64,
    memory: WindowsArmKnownGuestMemory,
) -> WindowsArmUefiVectorSyncProbe {
    let virtual_address = leaf_sample_virtual_address
        .checked_add(WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64);
    let physical_address = virtual_address.and_then(|sync_va| {
        translate_stage1_leaf_virtual_address(leaf, leaf_sample_virtual_address, sync_va)
    });
    let instruction_word = physical_address.and_then(|sync_ipa| memory.read_u32(sync_ipa));
    WindowsArmUefiVectorSyncProbe {
        virtual_address,
        physical_address,
        instruction_word,
        instruction_hint: instruction_word
            .map(aarch64_instruction_hint)
            .unwrap_or("not observed"),
    }
}

fn recommend_stage1_executable_leaf_base_vector(
    leaf: Stage1LeafDescriptor,
    leaf_sample_virtual_address: u64,
    vector_sync: WindowsArmUefiVectorSyncProbe,
) -> Option<WindowsArmUefiVectorBaseRecommendation> {
    let span_bytes = stage1_descriptor_span_bytes(leaf.level, leaf.kind)?;
    if span_bytes == 0 || !span_bytes.is_power_of_two() {
        return None;
    }
    let base_virtual_address = leaf_sample_virtual_address & !(span_bytes - 1);
    Some(WindowsArmUefiVectorBaseRecommendation {
        base_virtual_address,
        base_physical_address: translate_stage1_leaf_virtual_address(
            leaf,
            leaf_sample_virtual_address,
            base_virtual_address,
        ),
        current_el_spx_sync_instruction_word: vector_sync.instruction_word,
        current_el_spx_sync_instruction_hint: vector_sync.instruction_hint,
        reason: "fallback-el1-executable-leaf-base-empty-vector-scan",
    })
}

fn collect_stage1_vector_base_candidates_for_leaf(
    leaf: Stage1LeafDescriptor,
    leaf_sample_virtual_address: u64,
    memory: WindowsArmKnownGuestMemory,
) -> WindowsArmUefiVectorBaseCandidateScan {
    let empty_scan = || WindowsArmUefiVectorBaseCandidateScan {
        scanned_count: 0,
        suppressed_count: 0,
        limit_reached: false,
        candidates: Vec::new(),
    };
    let Some(span_bytes) = stage1_descriptor_span_bytes(leaf.level, leaf.kind) else {
        return empty_scan();
    };
    if span_bytes == 0 || !span_bytes.is_power_of_two() {
        return empty_scan();
    }

    let leaf_base_virtual_address = leaf_sample_virtual_address & !(span_bytes - 1);
    let Some(leaf_end_virtual_address) = leaf_base_virtual_address.checked_add(span_bytes) else {
        return empty_scan();
    };
    let mut candidates = Vec::new();
    let mut scanned_count = 0_u32;
    let mut suppressed_count = 0_u32;
    let mut limit_reached = false;
    let mut base_virtual_address = leaf_base_virtual_address;
    while base_virtual_address < leaf_end_virtual_address {
        scanned_count = scanned_count.saturating_add(1);
        let base_physical_address = translate_stage1_leaf_virtual_address(
            leaf,
            leaf_sample_virtual_address,
            base_virtual_address,
        );
        let slots = read_stage1_vector_slot_instructions(
            leaf,
            leaf_sample_virtual_address,
            base_virtual_address,
            memory,
        );
        let populated_slot_count = slots.populated_slot_count();
        if populated_slot_count > 0 {
            candidates.push(WindowsArmUefiVectorBaseCandidate {
                base_virtual_address,
                base_physical_address,
                current_el_sp0_sync_instruction_word: slots.current_el_sp0_sync_instruction_word,
                current_el_spx_sync_instruction_word: slots.current_el_spx_sync_instruction_word,
                lower_aarch64_sync_instruction_word: slots.lower_aarch64_sync_instruction_word,
                lower_aarch32_sync_instruction_word: slots.lower_aarch32_sync_instruction_word,
                current_el_spx_sync_instruction_hint: slots.current_el_spx_sync_instruction_hint(),
                populated_slot_count,
            });
            if candidates.len() >= WINDOWS_ARM_VECTOR_BASE_SCAN_MAX_PER_LEAF {
                limit_reached = true;
                break;
            }
        } else {
            suppressed_count = suppressed_count.saturating_add(1);
        }
        base_virtual_address =
            match base_virtual_address.checked_add(WINDOWS_ARM_VECTOR_BASE_SCAN_ALIGNMENT) {
                Some(next) => next,
                None => break,
            };
    }
    WindowsArmUefiVectorBaseCandidateScan {
        scanned_count,
        suppressed_count,
        limit_reached,
        candidates,
    }
}

fn recommend_stage1_vector_base_candidate(
    candidates: &[WindowsArmUefiVectorBaseCandidate],
) -> Option<WindowsArmUefiVectorBaseRecommendation> {
    if let Some(candidate) = candidates.iter().find(|candidate| {
        vector_slot_instruction_is_non_diagnostic_populated(
            candidate.current_el_spx_sync_instruction_word,
        )
    }) {
        return Some(vector_base_recommendation(
            candidate,
            "current-el-spx-populated-non-diagnostic",
        ));
    }

    if let Some(candidate) = candidates
        .iter()
        .find(|candidate| vector_base_candidate_has_non_diagnostic_populated_slot(candidate))
    {
        return Some(vector_base_recommendation(
            candidate,
            "any-vector-slot-populated-non-diagnostic",
        ));
    }

    candidates
        .iter()
        .find(|candidate| candidate.populated_slot_count > 0)
        .map(|candidate| {
            vector_base_recommendation(candidate, "fallback-first-populated-vector-base")
        })
}

fn vector_base_recommendation(
    candidate: &WindowsArmUefiVectorBaseCandidate,
    reason: &'static str,
) -> WindowsArmUefiVectorBaseRecommendation {
    WindowsArmUefiVectorBaseRecommendation {
        base_virtual_address: candidate.base_virtual_address,
        base_physical_address: candidate.base_physical_address,
        current_el_spx_sync_instruction_word: candidate.current_el_spx_sync_instruction_word,
        current_el_spx_sync_instruction_hint: candidate.current_el_spx_sync_instruction_hint,
        reason,
    }
}

fn vector_base_candidate_has_non_diagnostic_populated_slot(
    candidate: &WindowsArmUefiVectorBaseCandidate,
) -> bool {
    [
        candidate.current_el_sp0_sync_instruction_word,
        candidate.current_el_spx_sync_instruction_word,
        candidate.lower_aarch64_sync_instruction_word,
        candidate.lower_aarch32_sync_instruction_word,
    ]
    .into_iter()
    .any(vector_slot_instruction_is_non_diagnostic_populated)
}

fn vector_slot_instruction_is_non_diagnostic_populated(word: Option<u32>) -> bool {
    crate::windows_arm_vector_slot_instruction_is_non_diagnostic_populated(word)
}

fn read_stage1_vector_slot_instructions(
    leaf: Stage1LeafDescriptor,
    leaf_sample_virtual_address: u64,
    base_virtual_address: u64,
    memory: WindowsArmKnownGuestMemory,
) -> Stage1VectorSlotInstructions {
    Stage1VectorSlotInstructions {
        current_el_sp0_sync_instruction_word: read_stage1_vector_slot_instruction_word(
            leaf,
            leaf_sample_virtual_address,
            base_virtual_address,
            0x000,
            memory,
        ),
        current_el_spx_sync_instruction_word: read_stage1_vector_slot_instruction_word(
            leaf,
            leaf_sample_virtual_address,
            base_virtual_address,
            0x200,
            memory,
        ),
        lower_aarch64_sync_instruction_word: read_stage1_vector_slot_instruction_word(
            leaf,
            leaf_sample_virtual_address,
            base_virtual_address,
            0x400,
            memory,
        ),
        lower_aarch32_sync_instruction_word: read_stage1_vector_slot_instruction_word(
            leaf,
            leaf_sample_virtual_address,
            base_virtual_address,
            0x600,
            memory,
        ),
    }
}

fn read_stage1_vector_slot_instruction_word(
    leaf: Stage1LeafDescriptor,
    leaf_sample_virtual_address: u64,
    base_virtual_address: u64,
    slot_offset: u64,
    memory: WindowsArmKnownGuestMemory,
) -> Option<u32> {
    let slot_virtual_address = base_virtual_address.checked_add(slot_offset)?;
    let slot_physical_address = translate_stage1_leaf_virtual_address(
        leaf,
        leaf_sample_virtual_address,
        slot_virtual_address,
    )?;
    memory.read_u32(slot_physical_address)
}

fn vector_slot_instruction_is_populated(word: Option<u32>) -> bool {
    crate::windows_arm_vector_slot_instruction_is_populated(word)
}

#[cfg(test)]
mod stage1_vector_base_candidate_tests {
    use super::*;

    fn write_pflash_word(firmware_memory: &mut [u8], ipa: u64, word: u32) {
        let offset = ipa
            .checked_sub(WINDOWS_ARM_UEFI_LOW_CODE_ALIAS_IPA)
            .expect("test IPA is in low pflash alias") as usize;
        firmware_memory[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
    }

    #[test]
    fn vector_base_scan_filters_erased_slots_and_caps_reported_candidates() {
        let mut firmware_memory = vec![0; WINDOWS_ARM_UEFI_SLOT_BYTES as usize];
        let leaf_sample_virtual_address = WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA;
        let leaf = Stage1LeafDescriptor {
            level: 2,
            descriptor: 0x200f8d,
            kind: "block",
            output_address: Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA),
            attr_index: 3,
            access_permissions: 0,
            shareability: 3,
            access_flag: true,
            pxn: false,
            uxn: false,
        };

        write_pflash_word(
            &mut firmware_memory,
            leaf_sample_virtual_address
                + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64,
            0xffff_ffff,
        );
        for index in 1..=12_u64 {
            write_pflash_word(
                &mut firmware_memory,
                leaf_sample_virtual_address
                    + index * WINDOWS_ARM_VECTOR_BASE_SCAN_ALIGNMENT
                    + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64,
                AARCH64_HVC_0,
            );
        }
        write_pflash_word(
            &mut firmware_memory,
            leaf_sample_virtual_address
                + 2 * WINDOWS_ARM_VECTOR_BASE_SCAN_ALIGNMENT
                + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64,
            AARCH64_WFI,
        );

        let scan = collect_stage1_vector_base_candidates_for_leaf(
            leaf,
            leaf_sample_virtual_address,
            WindowsArmKnownGuestMemory {
                firmware_memory: firmware_memory.as_ptr().cast(),
                vars_memory: ptr::null(),
                guest_ram_memory: ptr::null(),
                guest_ram_bytes: 0,
            },
        );

        assert_eq!(
            scan.candidates.len(),
            WINDOWS_ARM_VECTOR_BASE_SCAN_MAX_PER_LEAF
        );
        assert_eq!(scan.scanned_count, 9);
        assert_eq!(scan.suppressed_count, 1);
        assert!(scan.limit_reached);

        let first = &scan.candidates[0];
        assert_eq!(
            first.base_virtual_address,
            leaf_sample_virtual_address + WINDOWS_ARM_VECTOR_BASE_SCAN_ALIGNMENT
        );
        assert_eq!(
            first.base_physical_address,
            Some(first.base_virtual_address)
        );
        assert_eq!(
            first.current_el_spx_sync_instruction_word,
            Some(AARCH64_HVC_0)
        );
        assert_eq!(first.current_el_spx_sync_instruction_hint, "hvc-0");
        assert_eq!(first.populated_slot_count, 1);
        assert!(scan.candidates.iter().all(|candidate| {
            candidate.base_virtual_address % WINDOWS_ARM_VECTOR_BASE_SCAN_ALIGNMENT == 0
                && candidate.current_el_spx_sync_instruction_word != Some(0xffff_ffff)
        }));

        let recommendation = recommend_stage1_vector_base_candidate(&scan.candidates)
            .expect("non-diagnostic vector candidate should be recommended");
        assert_eq!(
            recommendation.base_virtual_address,
            leaf_sample_virtual_address + 2 * WINDOWS_ARM_VECTOR_BASE_SCAN_ALIGNMENT
        );
        assert_eq!(
            recommendation.base_physical_address,
            Some(recommendation.base_virtual_address)
        );
        assert_eq!(
            recommendation.current_el_spx_sync_instruction_word,
            Some(AARCH64_WFI)
        );
        assert_eq!(recommendation.current_el_spx_sync_instruction_hint, "wfi");
        assert_eq!(
            recommendation.reason,
            "current-el-spx-populated-non-diagnostic"
        );
    }
}

fn stage1_descriptor_kind(descriptor: u64, level: u8) -> &'static str {
    match (descriptor & 0x3, level) {
        (0, _) => "invalid",
        (1, 0..=2) => "block",
        (1, _) => "reserved",
        (3, 0..=2) => "table",
        (3, _) => "page",
        _ => "reserved",
    }
}

fn stage1_descriptor_output_address(descriptor: u64, level: u8, kind: &'static str) -> Option<u64> {
    let shift = match (kind, level) {
        ("block", 0) => 39,
        ("block", 1) => 30,
        ("block", 2) => 21,
        ("page", 3) => 12,
        _ => return None,
    };
    let address_bits_mask = 0x0000_ffff_ffff_ffffu64;
    Some(descriptor & address_bits_mask & !((1u64 << shift) - 1))
}

fn stage1_descriptor_span_bytes(level: u8, kind: &'static str) -> Option<u64> {
    let shift = match (kind, level) {
        ("block", 0) => 39,
        ("block", 1) => 30,
        ("block", 2) => 21,
        ("page", 3) => 12,
        _ => return None,
    };
    Some(1u64 << shift)
}

fn translate_stage1_leaf_virtual_address(
    leaf: Stage1LeafDescriptor,
    leaf_sample_virtual_address: u64,
    virtual_address: u64,
) -> Option<u64> {
    let output_address = leaf.output_address?;
    let span_bytes = stage1_descriptor_span_bytes(leaf.level, leaf.kind)?;
    if span_bytes == 0 || !span_bytes.is_power_of_two() {
        return None;
    }
    let leaf_base_virtual_address = leaf_sample_virtual_address & !(span_bytes - 1);
    let leaf_end_virtual_address = leaf_base_virtual_address.checked_add(span_bytes)?;
    if virtual_address < leaf_base_virtual_address || virtual_address >= leaf_end_virtual_address {
        return None;
    }
    output_address.checked_add(virtual_address - leaf_base_virtual_address)
}

fn read_known_guest_phys_u64(
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

fn write_known_guest_phys_u64(
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

fn stage1_page_descriptor_for_output_address(
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

fn patch_low_vector_stage1_page_descriptor(
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

fn patch_low_vector_diagnostic_page_descriptor(
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

fn patch_low_vector_recommended_vector_descriptor(
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
struct LowVectorDiagnosticPageRepairPreparation {
    diagnostic_slot_snapshot: Option<DiagnosticExceptionVectorSlotSnapshot>,
    patched_descriptor: Option<(u64, u64)>,
}

impl LowVectorDiagnosticPageRepairPreparation {
    fn vector_populated(self) -> bool {
        self.diagnostic_slot_snapshot.is_some()
    }
}

struct LowVectorDiagnosticPageRepairRequest<'a> {
    firmware_memory: *mut c_void,
    vars_memory: *mut c_void,
    guest_ram_memory: *mut c_void,
    slot_bytes: usize,
    guest_ram_bytes: usize,
    tcr_el1: Option<u64>,
    ttbr0_el1: Option<u64>,
    location: &'a str,
    blockers: &'a mut Vec<String>,
}

fn prepare_low_vector_diagnostic_page_repair(
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

fn guest_phys_memory_offset(
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

fn pflash_slot_offset(address: u64, slot_ipa: u64) -> Option<usize> {
    if address >= slot_ipa && address < slot_ipa.saturating_add(WINDOWS_ARM_UEFI_SLOT_BYTES) {
        address.checked_sub(slot_ipa)?.try_into().ok()
    } else {
        None
    }
}

struct VcpuRunObservation {
    run_status: HvReturn,
    exit_reason: Option<u32>,
    exit_syndrome: Option<u64>,
    exit_virtual_address: Option<u64>,
    exit_physical_address: Option<u64>,
    watchdog_cancel_status: Option<HvReturn>,
}

fn run_vcpu_once_with_watchdog(vcpu: HvVcpu, exit: *mut HvVcpuExit) -> VcpuRunObservation {
    run_vcpu_once_with_watchdog_timeout(vcpu, exit, 100)
}

fn run_vcpu_once_with_watchdog_timeout(
    vcpu: HvVcpu,
    exit: *mut HvVcpuExit,
    watchdog_timeout_ms: u64,
) -> VcpuRunObservation {
    let done = Arc::new(AtomicBool::new(false));
    let watchdog_done = Arc::clone(&done);
    let vcpu_for_watchdog = vcpu;
    let watchdog_timeout_ms = watchdog_timeout_ms.max(1);
    let watchdog = thread::spawn(move || {
        for _ in 0..watchdog_timeout_ms {
            if watchdog_done.load(Ordering::SeqCst) {
                return None;
            }
            thread::sleep(Duration::from_millis(1));
        }
        let mut vcpu = vcpu_for_watchdog;
        Some(unsafe { hv_vcpus_exit(&mut vcpu, 1) })
    });

    let run_status = unsafe { hv_vcpu_run(vcpu) };
    done.store(true, Ordering::SeqCst);
    let watchdog_cancel_status = watchdog.join().ok().flatten();

    let mut exit_reason = None;
    let mut exit_syndrome = None;
    let mut exit_virtual_address = None;
    let mut exit_physical_address = None;
    if run_status == HV_SUCCESS && !exit.is_null() {
        let exit_info = unsafe { &*exit };
        exit_reason = Some(exit_info.reason);
        exit_syndrome = Some(exit_info.exception.syndrome);
        exit_virtual_address = Some(exit_info.exception.virtual_address);
        exit_physical_address = Some(exit_info.exception.physical_address);
    }

    VcpuRunObservation {
        run_status,
        exit_reason,
        exit_syndrome,
        exit_virtual_address,
        exit_physical_address,
        watchdog_cancel_status,
    }
}

fn recommended_vector_base_vbar_redirect_target(
    exit: &WindowsArmUefiFirmwareRunLoopExit,
) -> Option<&WindowsArmUefiVectorBaseRecommendation> {
    exit.stage1_executable_candidates_after_exit
        .iter()
        .find_map(|candidate| candidate.recommended_vector_base_candidate.as_ref())
}

fn low_vector_recommended_vector_remap_target(
    exit: &WindowsArmUefiFirmwareRunLoopExit,
) -> Option<&WindowsArmUefiVectorBaseRecommendation> {
    recommended_vector_base_vbar_redirect_target(exit)
        .filter(|recommendation| recommendation.is_populated_low_vector_remap_target())
}

fn exception_class(syndrome: u64) -> u64 {
    syndrome >> 26
}

fn is_data_abort_syndrome(syndrome: u64) -> bool {
    matches!(exception_class(syndrome), 0x24 | 0x25)
}

pub fn probe_hvf_vm_create(allow_create: bool, host: HvfHostCapabilities) -> HvfVmCreateProbe {
    let mut blockers = Vec::new();

    if !allow_create {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_VM_CREATE=1 or pass --allow-create to create and destroy an empty HVF VM".to_string(),
        );
        return HvfVmCreateProbe {
            allowed: false,
            attempted: false,
            created: false,
            destroyed: false,
            host,
            create_status: None,
            destroy_status: None,
            blockers,
        };
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return HvfVmCreateProbe {
            allowed: true,
            attempted: false,
            created: false,
            destroyed: false,
            host,
            create_status: None,
            destroy_status: None,
            blockers,
        };
    }

    let create_status = unsafe { hv_vm_create(ptr::null_mut()) };
    let created = create_status == HV_SUCCESS;
    if !created {
        blockers.push(format!("hv_vm_create failed: {create_status:#x}"));
        return HvfVmCreateProbe {
            allowed: true,
            attempted: true,
            created,
            destroyed: false,
            host,
            create_status: Some(create_status),
            destroy_status: None,
            blockers,
        };
    }

    let destroy_status = unsafe { hv_vm_destroy() };
    let destroyed = destroy_status == HV_SUCCESS;
    if !destroyed {
        blockers.push(format!("hv_vm_destroy failed: {destroy_status:#x}"));
    }

    HvfVmCreateProbe {
        allowed: true,
        attempted: true,
        created,
        destroyed,
        host,
        create_status: Some(create_status),
        destroy_status: Some(destroy_status),
        blockers,
    }
}

pub fn probe_hvf_vcpu_create(allow_create: bool, host: HvfHostCapabilities) -> HvfVcpuCreateProbe {
    let mut blockers = Vec::new();

    if !allow_create {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_VM_CREATE=1 or pass --allow-create to create and destroy an empty HVF VM and vCPU".to_string(),
        );
        return HvfVcpuCreateProbe {
            allowed: false,
            attempted: false,
            vm_created: false,
            vcpu_created: false,
            vcpu_destroyed: false,
            vm_destroyed: false,
            host,
            vm_create_status: None,
            vcpu_create_status: None,
            vcpu_destroy_status: None,
            vm_destroy_status: None,
            blockers,
        };
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return HvfVcpuCreateProbe {
            allowed: true,
            attempted: false,
            vm_created: false,
            vcpu_created: false,
            vcpu_destroyed: false,
            vm_destroyed: false,
            host,
            vm_create_status: None,
            vcpu_create_status: None,
            vcpu_destroy_status: None,
            vm_destroy_status: None,
            blockers,
        };
    }

    let vm_create_status = unsafe { hv_vm_create(ptr::null_mut()) };
    let vm_created = vm_create_status == HV_SUCCESS;
    if !vm_created {
        blockers.push(format!("hv_vm_create failed: {vm_create_status:#x}"));
        return HvfVcpuCreateProbe {
            allowed: true,
            attempted: true,
            vm_created,
            vcpu_created: false,
            vcpu_destroyed: false,
            vm_destroyed: false,
            host,
            vm_create_status: Some(vm_create_status),
            vcpu_create_status: None,
            vcpu_destroy_status: None,
            vm_destroy_status: None,
            blockers,
        };
    }

    let mut vcpu = 0;
    let mut exit = ptr::null_mut();
    let vcpu_create_status = unsafe { hv_vcpu_create(&mut vcpu, &mut exit, ptr::null_mut()) };
    let vcpu_created = vcpu_create_status == HV_SUCCESS;
    if !vcpu_created {
        blockers.push(format!("hv_vcpu_create failed: {vcpu_create_status:#x}"));
        let vm_destroy_status = unsafe { hv_vm_destroy() };
        let vm_destroyed = vm_destroy_status == HV_SUCCESS;
        if !vm_destroyed {
            blockers.push(format!("hv_vm_destroy failed: {vm_destroy_status:#x}"));
        }
        return HvfVcpuCreateProbe {
            allowed: true,
            attempted: true,
            vm_created,
            vcpu_created,
            vcpu_destroyed: false,
            vm_destroyed,
            host,
            vm_create_status: Some(vm_create_status),
            vcpu_create_status: Some(vcpu_create_status),
            vcpu_destroy_status: None,
            vm_destroy_status: Some(vm_destroy_status),
            blockers,
        };
    }

    let vcpu_destroy_status = unsafe { hv_vcpu_destroy(vcpu) };
    let vcpu_destroyed = vcpu_destroy_status == HV_SUCCESS;
    if !vcpu_destroyed {
        blockers.push(format!("hv_vcpu_destroy failed: {vcpu_destroy_status:#x}"));
    }

    let vm_destroy_status = unsafe { hv_vm_destroy() };
    let vm_destroyed = vm_destroy_status == HV_SUCCESS;
    if !vm_destroyed {
        blockers.push(format!("hv_vm_destroy failed: {vm_destroy_status:#x}"));
    }

    HvfVcpuCreateProbe {
        allowed: true,
        attempted: true,
        vm_created,
        vcpu_created,
        vcpu_destroyed,
        vm_destroyed,
        host,
        vm_create_status: Some(vm_create_status),
        vcpu_create_status: Some(vcpu_create_status),
        vcpu_destroy_status: Some(vcpu_destroy_status),
        vm_destroy_status: Some(vm_destroy_status),
        blockers,
    }
}

pub fn probe_hvf_vcpu_run(allow_run: bool, host: HvfHostCapabilities) -> HvfVcpuRunProbe {
    let mut blockers = Vec::new();

    if !allow_run {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_VCPU_RUN=1 or pass --allow-run to pre-cancel and observe one hv_vcpu_run boundary".to_string(),
        );
        return HvfVcpuRunProbe {
            allowed: false,
            attempted: false,
            vm_created: false,
            vcpu_created: false,
            cancel_requested: false,
            run_attempted: false,
            run_boundary_observed: false,
            vcpu_destroyed: false,
            vm_destroyed: false,
            host,
            vm_create_status: None,
            vcpu_create_status: None,
            cancel_status: None,
            run_status: None,
            exit_reason: None,
            vcpu_destroy_status: None,
            vm_destroy_status: None,
            blockers,
        };
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return HvfVcpuRunProbe {
            allowed: true,
            attempted: false,
            vm_created: false,
            vcpu_created: false,
            cancel_requested: false,
            run_attempted: false,
            run_boundary_observed: false,
            vcpu_destroyed: false,
            vm_destroyed: false,
            host,
            vm_create_status: None,
            vcpu_create_status: None,
            cancel_status: None,
            run_status: None,
            exit_reason: None,
            vcpu_destroy_status: None,
            vm_destroy_status: None,
            blockers,
        };
    }

    let vm_create_status = unsafe { hv_vm_create(ptr::null_mut()) };
    let vm_created = vm_create_status == HV_SUCCESS;
    if !vm_created {
        blockers.push(format!("hv_vm_create failed: {vm_create_status:#x}"));
        return HvfVcpuRunProbe {
            allowed: true,
            attempted: true,
            vm_created,
            vcpu_created: false,
            cancel_requested: false,
            run_attempted: false,
            run_boundary_observed: false,
            vcpu_destroyed: false,
            vm_destroyed: false,
            host,
            vm_create_status: Some(vm_create_status),
            vcpu_create_status: None,
            cancel_status: None,
            run_status: None,
            exit_reason: None,
            vcpu_destroy_status: None,
            vm_destroy_status: None,
            blockers,
        };
    }

    let mut vcpu = 0;
    let mut exit = ptr::null_mut();
    let vcpu_create_status = unsafe { hv_vcpu_create(&mut vcpu, &mut exit, ptr::null_mut()) };
    let vcpu_created = vcpu_create_status == HV_SUCCESS;
    if !vcpu_created {
        blockers.push(format!("hv_vcpu_create failed: {vcpu_create_status:#x}"));
        let vm_destroy_status = unsafe { hv_vm_destroy() };
        let vm_destroyed = vm_destroy_status == HV_SUCCESS;
        if !vm_destroyed {
            blockers.push(format!("hv_vm_destroy failed: {vm_destroy_status:#x}"));
        }
        return HvfVcpuRunProbe {
            allowed: true,
            attempted: true,
            vm_created,
            vcpu_created,
            cancel_requested: false,
            run_attempted: false,
            run_boundary_observed: false,
            vcpu_destroyed: false,
            vm_destroyed,
            host,
            vm_create_status: Some(vm_create_status),
            vcpu_create_status: Some(vcpu_create_status),
            cancel_status: None,
            run_status: None,
            exit_reason: None,
            vcpu_destroy_status: None,
            vm_destroy_status: Some(vm_destroy_status),
            blockers,
        };
    }

    let cancel_status = unsafe { hv_vcpus_exit(&mut vcpu, 1) };
    let cancel_requested = cancel_status == HV_SUCCESS;
    if !cancel_requested {
        blockers.push(format!("hv_vcpus_exit failed: {cancel_status:#x}"));
    }

    let mut run_attempted = false;
    let mut run_status = None;
    let mut exit_reason = None;
    if cancel_requested {
        run_attempted = true;
        let status = unsafe { hv_vcpu_run(vcpu) };
        run_status = Some(status);
        if status == HV_SUCCESS {
            if exit.is_null() {
                blockers
                    .push("hv_vcpu_run returned success without an exit info pointer".to_string());
            } else {
                exit_reason = Some(unsafe { (*exit).reason });
                if exit_reason != Some(HV_EXIT_REASON_CANCELED) {
                    blockers.push(format!(
                        "hv_vcpu_run returned unexpected exit reason: {}",
                        exit_reason.unwrap_or_default()
                    ));
                }
            }
        } else {
            blockers.push(format!("hv_vcpu_run failed: {status:#x}"));
        }
    }

    let run_boundary_observed =
        run_status == Some(HV_SUCCESS) && exit_reason == Some(HV_EXIT_REASON_CANCELED);

    let vcpu_destroy_status = unsafe { hv_vcpu_destroy(vcpu) };
    let vcpu_destroyed = vcpu_destroy_status == HV_SUCCESS;
    if !vcpu_destroyed {
        blockers.push(format!("hv_vcpu_destroy failed: {vcpu_destroy_status:#x}"));
    }

    let vm_destroy_status = unsafe { hv_vm_destroy() };
    let vm_destroyed = vm_destroy_status == HV_SUCCESS;
    if !vm_destroyed {
        blockers.push(format!("hv_vm_destroy failed: {vm_destroy_status:#x}"));
    }

    HvfVcpuRunProbe {
        allowed: true,
        attempted: true,
        vm_created,
        vcpu_created,
        cancel_requested,
        run_attempted,
        run_boundary_observed,
        vcpu_destroyed,
        vm_destroyed,
        host,
        vm_create_status: Some(vm_create_status),
        vcpu_create_status: Some(vcpu_create_status),
        cancel_status: Some(cancel_status),
        run_status,
        exit_reason,
        vcpu_destroy_status: Some(vcpu_destroy_status),
        vm_destroy_status: Some(vm_destroy_status),
        blockers,
    }
}

pub fn probe_hvf_interrupt_timer(
    allow_probe: bool,
    host: HvfHostCapabilities,
) -> HvfInterruptTimerProbe {
    let mut blockers = Vec::new();
    let vtimer_offset_value = 0x1000;

    if !allow_probe {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_INTERRUPT_TIMER=1 or pass --allow-interrupt-timer to create an empty HVF VM/vCPU and verify pending IRQ plus virtual timer controls".to_string(),
        );
        return HvfInterruptTimerProbe {
            allowed: false,
            attempted: false,
            vm_created: false,
            vcpu_created: false,
            pending_irq_set: false,
            pending_irq_cleared: false,
            vtimer_masked: false,
            vtimer_unmasked: false,
            vtimer_offset_set: false,
            boundary_observed: false,
            vcpu_destroyed: false,
            vm_destroyed: false,
            host,
            vtimer_offset_value,
            vm_create_status: None,
            vcpu_create_status: None,
            irq_set_status: None,
            irq_get_after_set_status: None,
            irq_pending_after_set: None,
            irq_clear_status: None,
            irq_get_after_clear_status: None,
            irq_pending_after_clear: None,
            vtimer_mask_set_status: None,
            vtimer_mask_get_status: None,
            vtimer_mask_after_set: None,
            vtimer_unmask_status: None,
            vtimer_unmask_get_status: None,
            vtimer_mask_after_clear: None,
            vtimer_offset_set_status: None,
            vtimer_offset_get_status: None,
            vtimer_offset_after_set: None,
            vcpu_destroy_status: None,
            vm_destroy_status: None,
            blockers,
        };
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return HvfInterruptTimerProbe {
            allowed: true,
            attempted: false,
            vm_created: false,
            vcpu_created: false,
            pending_irq_set: false,
            pending_irq_cleared: false,
            vtimer_masked: false,
            vtimer_unmasked: false,
            vtimer_offset_set: false,
            boundary_observed: false,
            vcpu_destroyed: false,
            vm_destroyed: false,
            host,
            vtimer_offset_value,
            vm_create_status: None,
            vcpu_create_status: None,
            irq_set_status: None,
            irq_get_after_set_status: None,
            irq_pending_after_set: None,
            irq_clear_status: None,
            irq_get_after_clear_status: None,
            irq_pending_after_clear: None,
            vtimer_mask_set_status: None,
            vtimer_mask_get_status: None,
            vtimer_mask_after_set: None,
            vtimer_unmask_status: None,
            vtimer_unmask_get_status: None,
            vtimer_mask_after_clear: None,
            vtimer_offset_set_status: None,
            vtimer_offset_get_status: None,
            vtimer_offset_after_set: None,
            vcpu_destroy_status: None,
            vm_destroy_status: None,
            blockers,
        };
    }

    let vm_create_status = unsafe { hv_vm_create(ptr::null_mut()) };
    let vm_created = vm_create_status == HV_SUCCESS;
    if !vm_created {
        blockers.push(format!("hv_vm_create failed: {vm_create_status:#x}"));
        return HvfInterruptTimerProbe {
            allowed: true,
            attempted: true,
            vm_created,
            vcpu_created: false,
            pending_irq_set: false,
            pending_irq_cleared: false,
            vtimer_masked: false,
            vtimer_unmasked: false,
            vtimer_offset_set: false,
            boundary_observed: false,
            vcpu_destroyed: false,
            vm_destroyed: false,
            host,
            vtimer_offset_value,
            vm_create_status: Some(vm_create_status),
            vcpu_create_status: None,
            irq_set_status: None,
            irq_get_after_set_status: None,
            irq_pending_after_set: None,
            irq_clear_status: None,
            irq_get_after_clear_status: None,
            irq_pending_after_clear: None,
            vtimer_mask_set_status: None,
            vtimer_mask_get_status: None,
            vtimer_mask_after_set: None,
            vtimer_unmask_status: None,
            vtimer_unmask_get_status: None,
            vtimer_mask_after_clear: None,
            vtimer_offset_set_status: None,
            vtimer_offset_get_status: None,
            vtimer_offset_after_set: None,
            vcpu_destroy_status: None,
            vm_destroy_status: None,
            blockers,
        };
    }

    let mut vcpu = 0;
    let mut exit = ptr::null_mut();
    let vcpu_create_status = unsafe { hv_vcpu_create(&mut vcpu, &mut exit, ptr::null_mut()) };
    let vcpu_created = vcpu_create_status == HV_SUCCESS;
    if !vcpu_created {
        blockers.push(format!("hv_vcpu_create failed: {vcpu_create_status:#x}"));
        let vm_destroy_status = unsafe { hv_vm_destroy() };
        let vm_destroyed = vm_destroy_status == HV_SUCCESS;
        if !vm_destroyed {
            blockers.push(format!("hv_vm_destroy failed: {vm_destroy_status:#x}"));
        }
        return HvfInterruptTimerProbe {
            allowed: true,
            attempted: true,
            vm_created,
            vcpu_created,
            pending_irq_set: false,
            pending_irq_cleared: false,
            vtimer_masked: false,
            vtimer_unmasked: false,
            vtimer_offset_set: false,
            boundary_observed: false,
            vcpu_destroyed: false,
            vm_destroyed,
            host,
            vtimer_offset_value,
            vm_create_status: Some(vm_create_status),
            vcpu_create_status: Some(vcpu_create_status),
            irq_set_status: None,
            irq_get_after_set_status: None,
            irq_pending_after_set: None,
            irq_clear_status: None,
            irq_get_after_clear_status: None,
            irq_pending_after_clear: None,
            vtimer_mask_set_status: None,
            vtimer_mask_get_status: None,
            vtimer_mask_after_set: None,
            vtimer_unmask_status: None,
            vtimer_unmask_get_status: None,
            vtimer_mask_after_clear: None,
            vtimer_offset_set_status: None,
            vtimer_offset_get_status: None,
            vtimer_offset_after_set: None,
            vcpu_destroy_status: None,
            vm_destroy_status: Some(vm_destroy_status),
            blockers,
        };
    }

    let irq_set_status =
        unsafe { hv_vcpu_set_pending_interrupt(vcpu, HV_INTERRUPT_TYPE_IRQ, true) };
    let pending_irq_set = irq_set_status == HV_SUCCESS;
    if !pending_irq_set {
        blockers.push(format!(
            "hv_vcpu_set_pending_interrupt IRQ=true failed: {irq_set_status:#x}"
        ));
    }
    let mut irq_pending_after_set_value = false;
    let irq_get_after_set_status = unsafe {
        hv_vcpu_get_pending_interrupt(
            vcpu,
            HV_INTERRUPT_TYPE_IRQ,
            &mut irq_pending_after_set_value,
        )
    };
    let irq_pending_after_set =
        (irq_get_after_set_status == HV_SUCCESS).then_some(irq_pending_after_set_value);
    if irq_get_after_set_status != HV_SUCCESS {
        blockers.push(format!(
            "hv_vcpu_get_pending_interrupt after IRQ set failed: {irq_get_after_set_status:#x}"
        ));
    } else if irq_pending_after_set != Some(true) {
        blockers.push("pending IRQ was not true after set".to_string());
    }

    let irq_clear_status =
        unsafe { hv_vcpu_set_pending_interrupt(vcpu, HV_INTERRUPT_TYPE_IRQ, false) };
    let pending_irq_cleared = irq_clear_status == HV_SUCCESS;
    if !pending_irq_cleared {
        blockers.push(format!(
            "hv_vcpu_set_pending_interrupt IRQ=false failed: {irq_clear_status:#x}"
        ));
    }
    let mut irq_pending_after_clear_value = true;
    let irq_get_after_clear_status = unsafe {
        hv_vcpu_get_pending_interrupt(
            vcpu,
            HV_INTERRUPT_TYPE_IRQ,
            &mut irq_pending_after_clear_value,
        )
    };
    let irq_pending_after_clear =
        (irq_get_after_clear_status == HV_SUCCESS).then_some(irq_pending_after_clear_value);
    if irq_get_after_clear_status != HV_SUCCESS {
        blockers.push(format!(
            "hv_vcpu_get_pending_interrupt after IRQ clear failed: {irq_get_after_clear_status:#x}"
        ));
    } else if irq_pending_after_clear != Some(false) {
        blockers.push("pending IRQ was not false after clear".to_string());
    }

    let vtimer_mask_set_status = unsafe { hv_vcpu_set_vtimer_mask(vcpu, true) };
    let vtimer_masked = vtimer_mask_set_status == HV_SUCCESS;
    if !vtimer_masked {
        blockers.push(format!(
            "hv_vcpu_set_vtimer_mask true failed: {vtimer_mask_set_status:#x}"
        ));
    }
    let mut vtimer_mask_after_set_value = false;
    let vtimer_mask_get_status =
        unsafe { hv_vcpu_get_vtimer_mask(vcpu, &mut vtimer_mask_after_set_value) };
    let vtimer_mask_after_set =
        (vtimer_mask_get_status == HV_SUCCESS).then_some(vtimer_mask_after_set_value);
    if vtimer_mask_get_status != HV_SUCCESS {
        blockers.push(format!(
            "hv_vcpu_get_vtimer_mask after set failed: {vtimer_mask_get_status:#x}"
        ));
    } else if vtimer_mask_after_set != Some(true) {
        blockers.push("VTimer mask was not true after set".to_string());
    }

    let vtimer_unmask_status = unsafe { hv_vcpu_set_vtimer_mask(vcpu, false) };
    let vtimer_unmasked = vtimer_unmask_status == HV_SUCCESS;
    if !vtimer_unmasked {
        blockers.push(format!(
            "hv_vcpu_set_vtimer_mask false failed: {vtimer_unmask_status:#x}"
        ));
    }
    let mut vtimer_mask_after_clear_value = true;
    let vtimer_unmask_get_status =
        unsafe { hv_vcpu_get_vtimer_mask(vcpu, &mut vtimer_mask_after_clear_value) };
    let vtimer_mask_after_clear =
        (vtimer_unmask_get_status == HV_SUCCESS).then_some(vtimer_mask_after_clear_value);
    if vtimer_unmask_get_status != HV_SUCCESS {
        blockers.push(format!(
            "hv_vcpu_get_vtimer_mask after clear failed: {vtimer_unmask_get_status:#x}"
        ));
    } else if vtimer_mask_after_clear != Some(false) {
        blockers.push("VTimer mask was not false after clear".to_string());
    }

    let vtimer_offset_set_status = unsafe { hv_vcpu_set_vtimer_offset(vcpu, vtimer_offset_value) };
    let vtimer_offset_set = vtimer_offset_set_status == HV_SUCCESS;
    if !vtimer_offset_set {
        blockers.push(format!(
            "hv_vcpu_set_vtimer_offset failed: {vtimer_offset_set_status:#x}"
        ));
    }
    let mut vtimer_offset_after_set_value = 0;
    let vtimer_offset_get_status =
        unsafe { hv_vcpu_get_vtimer_offset(vcpu, &mut vtimer_offset_after_set_value) };
    let vtimer_offset_after_set =
        (vtimer_offset_get_status == HV_SUCCESS).then_some(vtimer_offset_after_set_value);
    if vtimer_offset_get_status != HV_SUCCESS {
        blockers.push(format!(
            "hv_vcpu_get_vtimer_offset failed: {vtimer_offset_get_status:#x}"
        ));
    } else if vtimer_offset_after_set != Some(vtimer_offset_value) {
        blockers.push(format!(
            "VTimer offset was not preserved after set: expected {vtimer_offset_value:#x}, got {}",
            vtimer_offset_after_set
                .map_or_else(|| "not observed".to_string(), |value| format!("{value:#x}"))
        ));
    }

    let boundary_observed = pending_irq_set
        && irq_pending_after_set == Some(true)
        && pending_irq_cleared
        && irq_pending_after_clear == Some(false)
        && vtimer_masked
        && vtimer_mask_after_set == Some(true)
        && vtimer_unmasked
        && vtimer_mask_after_clear == Some(false)
        && vtimer_offset_set
        && vtimer_offset_after_set == Some(vtimer_offset_value);

    let vcpu_destroy_status = unsafe { hv_vcpu_destroy(vcpu) };
    let vcpu_destroyed = vcpu_destroy_status == HV_SUCCESS;
    if !vcpu_destroyed {
        blockers.push(format!("hv_vcpu_destroy failed: {vcpu_destroy_status:#x}"));
    }

    let vm_destroy_status = unsafe { hv_vm_destroy() };
    let vm_destroyed = vm_destroy_status == HV_SUCCESS;
    if !vm_destroyed {
        blockers.push(format!("hv_vm_destroy failed: {vm_destroy_status:#x}"));
    }

    HvfInterruptTimerProbe {
        allowed: true,
        attempted: true,
        vm_created,
        vcpu_created,
        pending_irq_set,
        pending_irq_cleared,
        vtimer_masked,
        vtimer_unmasked,
        vtimer_offset_set,
        boundary_observed,
        vcpu_destroyed,
        vm_destroyed,
        host,
        vtimer_offset_value,
        vm_create_status: Some(vm_create_status),
        vcpu_create_status: Some(vcpu_create_status),
        irq_set_status: Some(irq_set_status),
        irq_get_after_set_status: Some(irq_get_after_set_status),
        irq_pending_after_set,
        irq_clear_status: Some(irq_clear_status),
        irq_get_after_clear_status: Some(irq_get_after_clear_status),
        irq_pending_after_clear,
        vtimer_mask_set_status: Some(vtimer_mask_set_status),
        vtimer_mask_get_status: Some(vtimer_mask_get_status),
        vtimer_mask_after_set,
        vtimer_unmask_status: Some(vtimer_unmask_status),
        vtimer_unmask_get_status: Some(vtimer_unmask_get_status),
        vtimer_mask_after_clear,
        vtimer_offset_set_status: Some(vtimer_offset_set_status),
        vtimer_offset_get_status: Some(vtimer_offset_get_status),
        vtimer_offset_after_set,
        vcpu_destroy_status: Some(vcpu_destroy_status),
        vm_destroy_status: Some(vm_destroy_status),
        blockers,
    }
}

pub fn probe_hvf_vtimer_exit(allow_probe: bool, host: HvfHostCapabilities) -> HvfVtimerExitProbe {
    let mut blockers = Vec::new();
    let vtimer_offset_value = 0;
    let cntv_cval_value = 0;
    let cntv_ctl_value = 1;

    if !allow_probe {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_VTIMER_EXIT=1 or pass --allow-vtimer-exit to map a tiny WFI guest, program CNTV_CVAL_EL0/CNTV_CTL_EL0, and observe a real HV_EXIT_REASON_VTIMER_ACTIVATED boundary".to_string(),
        );
        return vtimer_exit_probe_result(false, false, host, blockers);
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return vtimer_exit_probe_result(true, false, host, blockers);
    }

    let mut memory_allocated = false;
    let mut memory_mapped = false;
    let mut vcpu_created = false;
    let mut pc_set = false;
    let mut cpsr_set = false;
    let mut vtimer_offset_set = false;
    let mut cntv_cval_set = false;
    let mut cntv_ctl_set = false;
    let mut vtimer_unmasked = false;
    let mut run_attempted = false;
    let mut vtimer_exit_observed = false;
    let mut pending_irq_injected = false;
    let mut vtimer_mask_observed_after_exit = None;
    let mut vtimer_unmasked_after_exit = false;
    let mut watchdog_cancel_fired = false;
    let mut vcpu_destroyed = false;
    let mut memory_unmapped = false;
    let mut vm_destroyed = false;
    let mut memory_deallocated = false;

    let mut allocate_status = None;
    let mut map_status = None;
    let mut vcpu_create_status = None;
    let mut pc_set_status = None;
    let mut cpsr_set_status = None;
    let mut vtimer_offset_set_status = None;
    let mut cntv_cval_set_status = None;
    let mut cntv_ctl_set_status = None;
    let mut vtimer_unmask_status = None;
    let mut run_status = None;
    let mut exit_reason = None;
    let mut exit_syndrome = None;
    let mut exit_virtual_address = None;
    let mut exit_physical_address = None;
    let mut watchdog_cancel_status = None;
    let mut pending_irq_set_status = None;
    let mut vtimer_mask_get_after_exit_status = None;
    let mut vtimer_unmask_after_exit_status = None;
    let mut vcpu_destroy_status = None;
    let mut unmap_status = None;
    let mut vm_destroy_status = None;
    let mut deallocate_status = None;

    let mut memory = ptr::null_mut();
    let mut vcpu = 0;
    let mut exit = ptr::null_mut();

    let status = unsafe { hv_vm_create(ptr::null_mut()) };
    let vm_create_status = Some(status);
    let vm_created = status == HV_SUCCESS;
    if !vm_created {
        blockers.push(format!("hv_vm_create failed: {status:#x}"));
    }

    if vm_created {
        let status = unsafe { hv_vm_allocate(&mut memory, PROBE_BYTES, HV_ALLOCATE_DEFAULT) };
        allocate_status = Some(status);
        memory_allocated = status == HV_SUCCESS && !memory.is_null();
        if memory_allocated {
            let instructions = [AARCH64_WFI, AARCH64_HVC_0];
            let mut bytes = Vec::with_capacity(instructions.len() * 4);
            for instruction in instructions {
                bytes.extend_from_slice(&instruction.to_le_bytes());
            }
            unsafe {
                ptr::copy_nonoverlapping(bytes.as_ptr(), memory.cast::<u8>(), bytes.len());
            }
        } else {
            blockers.push(format!("hv_vm_allocate failed: {status:#x}"));
        }
    }

    if vm_created && memory_allocated {
        let status = unsafe {
            hv_vm_map(
                memory,
                PROBE_IPA_START,
                PROBE_BYTES,
                HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC,
            )
        };
        map_status = Some(status);
        memory_mapped = status == HV_SUCCESS;
        if !memory_mapped {
            blockers.push(format!("hv_vm_map failed: {status:#x}"));
        }
    }

    if vm_created && memory_mapped {
        let status = unsafe { hv_vcpu_create(&mut vcpu, &mut exit, ptr::null_mut()) };
        vcpu_create_status = Some(status);
        vcpu_created = status == HV_SUCCESS;
        if !vcpu_created {
            blockers.push(format!("hv_vcpu_create failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, PROBE_IPA_START) };
        pc_set_status = Some(status);
        pc_set = status == HV_SUCCESS;
        if !pc_set {
            blockers.push(format!("hv_vcpu_set_reg(PC) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_CPSR, AARCH64_PSTATE_EL1H_DAIF_MASKED) };
        cpsr_set_status = Some(status);
        cpsr_set = status == HV_SUCCESS;
        if !cpsr_set {
            blockers.push(format!("hv_vcpu_set_reg(CPSR) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_vtimer_offset(vcpu, vtimer_offset_value) };
        vtimer_offset_set_status = Some(status);
        vtimer_offset_set = status == HV_SUCCESS;
        if !vtimer_offset_set {
            blockers.push(format!("hv_vcpu_set_vtimer_offset failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status =
            unsafe { hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_CNTV_CVAL_EL0, cntv_cval_value) };
        cntv_cval_set_status = Some(status);
        cntv_cval_set = status == HV_SUCCESS;
        if !cntv_cval_set {
            blockers.push(format!(
                "hv_vcpu_set_sys_reg(CNTV_CVAL_EL0) failed: {status:#x}"
            ));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_CNTV_CTL_EL0, cntv_ctl_value) };
        cntv_ctl_set_status = Some(status);
        cntv_ctl_set = status == HV_SUCCESS;
        if !cntv_ctl_set {
            blockers.push(format!(
                "hv_vcpu_set_sys_reg(CNTV_CTL_EL0) failed: {status:#x}"
            ));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_vtimer_mask(vcpu, false) };
        vtimer_unmask_status = Some(status);
        vtimer_unmasked = status == HV_SUCCESS;
        if !vtimer_unmasked {
            blockers.push(format!(
                "hv_vcpu_set_vtimer_mask(false) failed: {status:#x}"
            ));
        }
    }

    if vcpu_created
        && pc_set
        && cpsr_set
        && vtimer_offset_set
        && cntv_cval_set
        && cntv_ctl_set
        && vtimer_unmasked
    {
        run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        run_status = Some(observation.run_status);
        exit_reason = observation.exit_reason;
        exit_syndrome = observation.exit_syndrome;
        exit_virtual_address = observation.exit_virtual_address;
        exit_physical_address = observation.exit_physical_address;
        watchdog_cancel_status = observation.watchdog_cancel_status;
        watchdog_cancel_fired = watchdog_cancel_status.is_some();

        if observation.run_status == HV_SUCCESS {
            vtimer_exit_observed = observation.exit_reason == Some(HV_EXIT_REASON_VTIMER_ACTIVATED);
            if !vtimer_exit_observed {
                let reason_name = observation
                    .exit_reason
                    .map(hv_exit_reason_name)
                    .unwrap_or("not observed");
                blockers.push(format!(
                    "hv_vcpu_run did not return HV_EXIT_REASON_VTIMER_ACTIVATED; got {reason_name}"
                ));
            }
        } else {
            blockers.push(format!("hv_vcpu_run failed: {:#x}", observation.run_status));
        }

        if vtimer_exit_observed {
            let mut masked = false;
            let status = unsafe { hv_vcpu_get_vtimer_mask(vcpu, &mut masked) };
            vtimer_mask_get_after_exit_status = Some(status);
            if status == HV_SUCCESS {
                vtimer_mask_observed_after_exit = Some(masked);
                if !masked {
                    blockers.push(
                        "VTimer was not automatically masked after HV_EXIT_REASON_VTIMER_ACTIVATED"
                            .to_string(),
                    );
                }
            } else {
                blockers.push(format!(
                    "hv_vcpu_get_vtimer_mask after VTimer exit failed: {status:#x}"
                ));
            }

            let status =
                unsafe { hv_vcpu_set_pending_interrupt(vcpu, HV_INTERRUPT_TYPE_IRQ, true) };
            pending_irq_set_status = Some(status);
            pending_irq_injected = status == HV_SUCCESS;
            if !pending_irq_injected {
                blockers.push(format!(
                    "hv_vcpu_set_pending_interrupt IRQ=true after VTimer exit failed: {status:#x}"
                ));
            }

            let status = unsafe { hv_vcpu_set_vtimer_mask(vcpu, false) };
            vtimer_unmask_after_exit_status = Some(status);
            vtimer_unmasked_after_exit = status == HV_SUCCESS;
            if !vtimer_unmasked_after_exit {
                blockers.push(format!(
                    "hv_vcpu_set_vtimer_mask(false) after VTimer exit failed: {status:#x}"
                ));
            }
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_destroy(vcpu) };
        vcpu_destroy_status = Some(status);
        vcpu_destroyed = status == HV_SUCCESS;
        if !vcpu_destroyed {
            blockers.push(format!("hv_vcpu_destroy failed: {status:#x}"));
        }
    }

    if memory_mapped {
        let status = unsafe { hv_vm_unmap(PROBE_IPA_START, PROBE_BYTES) };
        unmap_status = Some(status);
        memory_unmapped = status == HV_SUCCESS;
        if !memory_unmapped {
            blockers.push(format!("hv_vm_unmap failed: {status:#x}"));
        }
    }

    if vm_created {
        let status = unsafe { hv_vm_destroy() };
        vm_destroy_status = Some(status);
        vm_destroyed = status == HV_SUCCESS;
        if !vm_destroyed {
            blockers.push(format!("hv_vm_destroy failed: {status:#x}"));
        }
    }

    if memory_allocated {
        let status = unsafe { hv_vm_deallocate(memory, PROBE_BYTES) };
        deallocate_status = Some(status);
        memory_deallocated = status == HV_SUCCESS;
        if !memory_deallocated {
            blockers.push(format!("hv_vm_deallocate failed: {status:#x}"));
        }
    }

    HvfVtimerExitProbe {
        allowed: true,
        attempted: true,
        vm_created,
        memory_allocated,
        memory_mapped,
        vcpu_created,
        pc_set,
        cpsr_set,
        vtimer_offset_set,
        cntv_cval_set,
        cntv_ctl_set,
        vtimer_unmasked,
        run_attempted,
        vtimer_exit_observed,
        pending_irq_injected,
        vtimer_mask_observed_after_exit,
        vtimer_unmasked_after_exit,
        watchdog_cancel_fired,
        vcpu_destroyed,
        memory_unmapped,
        vm_destroyed,
        memory_deallocated,
        host,
        ipa_start: PROBE_IPA_START,
        bytes: PROBE_BYTES,
        instructions: "WFI; HVC #0",
        vtimer_offset_value,
        cntv_cval_value,
        cntv_ctl_value,
        vm_create_status,
        allocate_status,
        map_status,
        vcpu_create_status,
        pc_set_status,
        cpsr_set_status,
        vtimer_offset_set_status,
        cntv_cval_set_status,
        cntv_ctl_set_status,
        vtimer_unmask_status,
        run_status,
        exit_reason,
        exit_syndrome,
        exit_virtual_address,
        exit_physical_address,
        watchdog_cancel_status,
        pending_irq_set_status,
        vtimer_mask_get_after_exit_status,
        vtimer_unmask_after_exit_status,
        vcpu_destroy_status,
        unmap_status,
        vm_destroy_status,
        deallocate_status,
        blockers,
    }
}

fn vtimer_exit_probe_result(
    allowed: bool,
    attempted: bool,
    host: HvfHostCapabilities,
    blockers: Vec<String>,
) -> HvfVtimerExitProbe {
    HvfVtimerExitProbe {
        allowed,
        attempted,
        vm_created: false,
        memory_allocated: false,
        memory_mapped: false,
        vcpu_created: false,
        pc_set: false,
        cpsr_set: false,
        vtimer_offset_set: false,
        cntv_cval_set: false,
        cntv_ctl_set: false,
        vtimer_unmasked: false,
        run_attempted: false,
        vtimer_exit_observed: false,
        pending_irq_injected: false,
        vtimer_mask_observed_after_exit: None,
        vtimer_unmasked_after_exit: false,
        watchdog_cancel_fired: false,
        vcpu_destroyed: false,
        memory_unmapped: false,
        vm_destroyed: false,
        memory_deallocated: false,
        host,
        ipa_start: PROBE_IPA_START,
        bytes: PROBE_BYTES,
        instructions: "WFI; HVC #0",
        vtimer_offset_value: 0,
        cntv_cval_value: 0,
        cntv_ctl_value: 1,
        vm_create_status: None,
        allocate_status: None,
        map_status: None,
        vcpu_create_status: None,
        pc_set_status: None,
        cpsr_set_status: None,
        vtimer_offset_set_status: None,
        cntv_cval_set_status: None,
        cntv_ctl_set_status: None,
        vtimer_unmask_status: None,
        run_status: None,
        exit_reason: None,
        exit_syndrome: None,
        exit_virtual_address: None,
        exit_physical_address: None,
        watchdog_cancel_status: None,
        pending_irq_set_status: None,
        vtimer_mask_get_after_exit_status: None,
        vtimer_unmask_after_exit_status: None,
        vcpu_destroy_status: None,
        unmap_status: None,
        vm_destroy_status: None,
        deallocate_status: None,
        blockers,
    }
}

pub fn probe_hvf_memory_map(allow_map: bool, host: HvfHostCapabilities) -> HvfMemoryMapProbe {
    let mut blockers = Vec::new();

    if !allow_map {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_MEMORY_MAP=1 or pass --allow-map to create an empty HVF VM and map/unmap one guest RAM page".to_string(),
        );
        return HvfMemoryMapProbe {
            allowed: false,
            attempted: false,
            vm_created: false,
            memory_allocated: false,
            memory_mapped: false,
            memory_unmapped: false,
            memory_deallocated: false,
            vm_destroyed: false,
            host,
            ipa_start: PROBE_IPA_START,
            bytes: PROBE_BYTES,
            vm_create_status: None,
            allocate_status: None,
            map_status: None,
            unmap_status: None,
            deallocate_status: None,
            vm_destroy_status: None,
            blockers,
        };
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return HvfMemoryMapProbe {
            allowed: true,
            attempted: false,
            vm_created: false,
            memory_allocated: false,
            memory_mapped: false,
            memory_unmapped: false,
            memory_deallocated: false,
            vm_destroyed: false,
            host,
            ipa_start: PROBE_IPA_START,
            bytes: PROBE_BYTES,
            vm_create_status: None,
            allocate_status: None,
            map_status: None,
            unmap_status: None,
            deallocate_status: None,
            vm_destroy_status: None,
            blockers,
        };
    }

    let vm_create_status = unsafe { hv_vm_create(ptr::null_mut()) };
    let vm_created = vm_create_status == HV_SUCCESS;
    if !vm_created {
        blockers.push(format!("hv_vm_create failed: {vm_create_status:#x}"));
        return HvfMemoryMapProbe {
            allowed: true,
            attempted: true,
            vm_created,
            memory_allocated: false,
            memory_mapped: false,
            memory_unmapped: false,
            memory_deallocated: false,
            vm_destroyed: false,
            host,
            ipa_start: PROBE_IPA_START,
            bytes: PROBE_BYTES,
            vm_create_status: Some(vm_create_status),
            allocate_status: None,
            map_status: None,
            unmap_status: None,
            deallocate_status: None,
            vm_destroy_status: None,
            blockers,
        };
    }

    let mut memory = ptr::null_mut();
    let allocate_status = unsafe { hv_vm_allocate(&mut memory, PROBE_BYTES, HV_ALLOCATE_DEFAULT) };
    let memory_allocated = allocate_status == HV_SUCCESS && !memory.is_null();
    if !memory_allocated {
        blockers.push(format!("hv_vm_allocate failed: {allocate_status:#x}"));
        let vm_destroy_status = unsafe { hv_vm_destroy() };
        let vm_destroyed = vm_destroy_status == HV_SUCCESS;
        if !vm_destroyed {
            blockers.push(format!("hv_vm_destroy failed: {vm_destroy_status:#x}"));
        }
        return HvfMemoryMapProbe {
            allowed: true,
            attempted: true,
            vm_created,
            memory_allocated,
            memory_mapped: false,
            memory_unmapped: false,
            memory_deallocated: false,
            vm_destroyed,
            host,
            ipa_start: PROBE_IPA_START,
            bytes: PROBE_BYTES,
            vm_create_status: Some(vm_create_status),
            allocate_status: Some(allocate_status),
            map_status: None,
            unmap_status: None,
            deallocate_status: None,
            vm_destroy_status: Some(vm_destroy_status),
            blockers,
        };
    }

    let map_status = unsafe {
        hv_vm_map(
            memory,
            PROBE_IPA_START,
            PROBE_BYTES,
            HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC,
        )
    };
    let memory_mapped = map_status == HV_SUCCESS;
    if !memory_mapped {
        blockers.push(format!("hv_vm_map failed: {map_status:#x}"));
    }

    let mut unmap_status = None;
    let mut memory_unmapped = false;
    if memory_mapped {
        let status = unsafe { hv_vm_unmap(PROBE_IPA_START, PROBE_BYTES) };
        memory_unmapped = status == HV_SUCCESS;
        unmap_status = Some(status);
        if !memory_unmapped {
            blockers.push(format!("hv_vm_unmap failed: {status:#x}"));
        }
    }

    let vm_destroy_status = unsafe { hv_vm_destroy() };
    let vm_destroyed = vm_destroy_status == HV_SUCCESS;
    if !vm_destroyed {
        blockers.push(format!("hv_vm_destroy failed: {vm_destroy_status:#x}"));
    }

    let deallocate_status = unsafe { hv_vm_deallocate(memory, PROBE_BYTES) };
    let memory_deallocated = deallocate_status == HV_SUCCESS;
    if !memory_deallocated {
        blockers.push(format!("hv_vm_deallocate failed: {deallocate_status:#x}"));
    }

    HvfMemoryMapProbe {
        allowed: true,
        attempted: true,
        vm_created,
        memory_allocated,
        memory_mapped,
        memory_unmapped,
        memory_deallocated,
        vm_destroyed,
        host,
        ipa_start: PROBE_IPA_START,
        bytes: PROBE_BYTES,
        vm_create_status: Some(vm_create_status),
        allocate_status: Some(allocate_status),
        map_status: Some(map_status),
        unmap_status,
        deallocate_status: Some(deallocate_status),
        vm_destroy_status: Some(vm_destroy_status),
        blockers,
    }
}

pub fn probe_windows_11_arm_uefi_pflash_hvf_map(
    allow_map: bool,
    pflash_map: WindowsArmUefiPflashMapProbe,
    host: HvfHostCapabilities,
) -> WindowsArmUefiPflashHvfMapProbe {
    let mut blockers = pflash_map.blockers.clone();
    let firmware_source_bytes = pflash_map
        .firmware_slot
        .as_ref()
        .map(|slot| slot.source_bytes);
    let vars_source_bytes = pflash_map.vars_slot.as_ref().map(|slot| slot.source_bytes);

    if !allow_map {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_UEFI_PFLASH_MAP=1 or pass --allow-map to create an empty HVF VM and map/unmap Windows UEFI code/vars pflash slots".to_string(),
        );
        return pflash_hvf_map_result(
            false,
            false,
            host,
            pflash_map.pflash_map_verified,
            firmware_source_bytes,
            vars_source_bytes,
            PflashHvfMapOutcome {
                blockers,
                ..PflashHvfMapOutcome::default()
            },
        );
    }

    if !pflash_map.pflash_map_verified {
        blockers.push(
            "pflash memory-image mapper did not verify code/vars slots; refusing live HVF map"
                .to_string(),
        );
        return pflash_hvf_map_result(
            true,
            false,
            host,
            false,
            firmware_source_bytes,
            vars_source_bytes,
            PflashHvfMapOutcome {
                blockers,
                ..PflashHvfMapOutcome::default()
            },
        );
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return pflash_hvf_map_result(
            true,
            false,
            host,
            true,
            firmware_source_bytes,
            vars_source_bytes,
            PflashHvfMapOutcome {
                blockers,
                ..PflashHvfMapOutcome::default()
            },
        );
    }

    let slot_bytes_usize: usize = WINDOWS_ARM_UEFI_SLOT_BYTES
        .try_into()
        .expect("Windows UEFI pflash slot fits in usize");
    let mut firmware_memory = ptr::null_mut();
    let mut vars_memory = ptr::null_mut();
    let mut firmware_memory_populated = false;
    let mut vars_memory_populated = false;
    let mut firmware_memory_mapped = false;
    let mut vars_memory_mapped = false;
    let mut firmware_memory_unmapped = false;
    let mut vars_memory_unmapped = false;
    let mut firmware_memory_deallocated = false;
    let mut vars_memory_deallocated = false;
    let mut firmware_map_status = None;
    let mut vars_map_status = None;
    let mut firmware_unmap_status = None;
    let mut vars_unmap_status = None;
    let mut firmware_deallocate_status = None;
    let mut vars_deallocate_status = None;

    let vm_create_status = unsafe { hv_vm_create(ptr::null_mut()) };
    let vm_created = vm_create_status == HV_SUCCESS;
    if !vm_created {
        blockers.push(format!("hv_vm_create failed: {vm_create_status:#x}"));
        return pflash_hvf_map_result(
            true,
            true,
            host,
            true,
            firmware_source_bytes,
            vars_source_bytes,
            PflashHvfMapOutcome {
                vm_create_status: Some(vm_create_status),
                blockers,
                ..PflashHvfMapOutcome::default()
            },
        );
    }

    let firmware_status =
        unsafe { hv_vm_allocate(&mut firmware_memory, slot_bytes_usize, HV_ALLOCATE_DEFAULT) };
    let firmware_allocate_status = Some(firmware_status);
    let firmware_memory_allocated = firmware_status == HV_SUCCESS && !firmware_memory.is_null();
    if !firmware_memory_allocated {
        blockers.push(format!(
            "hv_vm_allocate firmware pflash failed: {firmware_status:#x}"
        ));
    }

    let vars_status =
        unsafe { hv_vm_allocate(&mut vars_memory, slot_bytes_usize, HV_ALLOCATE_DEFAULT) };
    let vars_allocate_status = Some(vars_status);
    let vars_memory_allocated = vars_status == HV_SUCCESS && !vars_memory.is_null();
    if !vars_memory_allocated {
        blockers.push(format!(
            "hv_vm_allocate vars pflash failed: {vars_status:#x}"
        ));
    }

    if firmware_memory_allocated {
        firmware_memory_populated = populate_pflash_hvf_memory(
            firmware_memory,
            pflash_map.firmware_slot.as_ref(),
            "firmware",
            &mut blockers,
        );
    }
    if vars_memory_allocated {
        vars_memory_populated = populate_pflash_hvf_memory(
            vars_memory,
            pflash_map.vars_slot.as_ref(),
            "vars",
            &mut blockers,
        );
    }

    if firmware_memory_populated {
        let status = unsafe {
            hv_vm_map(
                firmware_memory,
                WINDOWS_ARM_UEFI_CODE_IPA,
                slot_bytes_usize,
                HV_MEMORY_READ | HV_MEMORY_EXEC,
            )
        };
        firmware_map_status = Some(status);
        firmware_memory_mapped = status == HV_SUCCESS;
        if !firmware_memory_mapped {
            blockers.push(format!("hv_vm_map firmware pflash failed: {status:#x}"));
        }
    }

    if vars_memory_populated {
        let status = unsafe {
            hv_vm_map(
                vars_memory,
                WINDOWS_ARM_UEFI_VARS_IPA,
                slot_bytes_usize,
                HV_MEMORY_READ | HV_MEMORY_WRITE,
            )
        };
        vars_map_status = Some(status);
        vars_memory_mapped = status == HV_SUCCESS;
        if !vars_memory_mapped {
            blockers.push(format!("hv_vm_map vars pflash failed: {status:#x}"));
        }
    }

    if vars_memory_mapped {
        let status = unsafe { hv_vm_unmap(WINDOWS_ARM_UEFI_VARS_IPA, slot_bytes_usize) };
        vars_unmap_status = Some(status);
        vars_memory_unmapped = status == HV_SUCCESS;
        if !vars_memory_unmapped {
            blockers.push(format!("hv_vm_unmap vars pflash failed: {status:#x}"));
        }
    }

    if firmware_memory_mapped {
        let status = unsafe { hv_vm_unmap(WINDOWS_ARM_UEFI_CODE_IPA, slot_bytes_usize) };
        firmware_unmap_status = Some(status);
        firmware_memory_unmapped = status == HV_SUCCESS;
        if !firmware_memory_unmapped {
            blockers.push(format!("hv_vm_unmap firmware pflash failed: {status:#x}"));
        }
    }

    let vm_destroy_status = unsafe { hv_vm_destroy() };
    let vm_destroyed = vm_destroy_status == HV_SUCCESS;
    if !vm_destroyed {
        blockers.push(format!("hv_vm_destroy failed: {vm_destroy_status:#x}"));
    }

    if firmware_memory_allocated {
        let status = unsafe { hv_vm_deallocate(firmware_memory, slot_bytes_usize) };
        firmware_deallocate_status = Some(status);
        firmware_memory_deallocated = status == HV_SUCCESS;
        if !firmware_memory_deallocated {
            blockers.push(format!(
                "hv_vm_deallocate firmware pflash failed: {status:#x}"
            ));
        }
    }
    if vars_memory_allocated {
        let status = unsafe { hv_vm_deallocate(vars_memory, slot_bytes_usize) };
        vars_deallocate_status = Some(status);
        vars_memory_deallocated = status == HV_SUCCESS;
        if !vars_memory_deallocated {
            blockers.push(format!("hv_vm_deallocate vars pflash failed: {status:#x}"));
        }
    }

    WindowsArmUefiPflashHvfMapProbe {
        allowed: true,
        attempted: true,
        vm_created,
        firmware_memory_allocated,
        vars_memory_allocated,
        firmware_memory_populated,
        vars_memory_populated,
        firmware_memory_mapped,
        vars_memory_mapped,
        firmware_memory_unmapped,
        vars_memory_unmapped,
        firmware_memory_deallocated,
        vars_memory_deallocated,
        vm_destroyed,
        host,
        pflash_map_verified: pflash_map.pflash_map_verified,
        firmware_slot_ipa: WINDOWS_ARM_UEFI_CODE_IPA,
        vars_slot_ipa: WINDOWS_ARM_UEFI_VARS_IPA,
        slot_bytes: WINDOWS_ARM_UEFI_SLOT_BYTES,
        firmware_source_bytes,
        vars_source_bytes,
        firmware_map_flags: "read|exec",
        vars_map_flags: "read|write",
        vm_create_status: Some(vm_create_status),
        firmware_allocate_status,
        vars_allocate_status,
        firmware_map_status,
        vars_map_status,
        firmware_unmap_status,
        vars_unmap_status,
        firmware_deallocate_status,
        vars_deallocate_status,
        vm_destroy_status: Some(vm_destroy_status),
        blockers,
    }
}

fn populate_pflash_hvf_memory(
    memory: *mut c_void,
    slot: Option<&WindowsArmUefiPflashSlotMap>,
    label: &str,
    blockers: &mut Vec<String>,
) -> bool {
    let Some(slot) = slot else {
        blockers.push(format!("{label} pflash slot was not prepared"));
        return false;
    };
    let source_limit = match usize::try_from(slot.source_bytes) {
        Ok(source_limit) => source_limit,
        Err(_) => {
            blockers.push(format!(
                "{label} pflash source length exceeds host address space"
            ));
            return false;
        }
    };
    let source = match crate::media::read_bounded_file(&slot.path, source_limit) {
        Ok(source) => source,
        Err(error) => {
            blockers.push(format!("{label} pflash source read failed: {error}"));
            return false;
        }
    };
    if source.len() as u64 != slot.source_bytes {
        blockers.push(format!(
            "{label} pflash source length changed during HVF map probe"
        ));
        return false;
    }
    let slot_len: usize = slot
        .slot_bytes
        .try_into()
        .expect("Windows UEFI pflash slot fits in usize");
    unsafe {
        ptr::write_bytes(memory.cast::<u8>(), 0, slot_len);
        ptr::copy_nonoverlapping(source.as_ptr(), memory.cast::<u8>(), source.len());
        let mapped = std::slice::from_raw_parts(memory.cast::<u8>(), slot_len);
        mapped[..source.len()] == source[..] && mapped[source.len()..].iter().all(|byte| *byte == 0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DiagnosticExceptionVectorSlotSnapshot {
    start: usize,
    original: [u8; DIAGNOSTIC_EXCEPTION_VECTOR_SLOT_BYTES],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DiagnosticExceptionVectorSlotRange {
    vector_start: usize,
    eret_start: usize,
    landing_start: usize,
    vector_end: usize,
}

fn diagnostic_exception_vector_slot_range(
    bytes: usize,
    base_offset: usize,
    location: &str,
    blockers: &mut Vec<String>,
) -> Option<DiagnosticExceptionVectorSlotRange> {
    let vector_offset = WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET;
    let Some(vector_start) = base_offset.checked_add(vector_offset) else {
        blockers.push(format!(
            "diagnostic exception vector {location} offset overflowed"
        ));
        return None;
    };
    let Some(eret_start) = vector_start.checked_add(4) else {
        blockers.push(format!(
            "diagnostic exception vector {location} ERET offset overflowed"
        ));
        return None;
    };
    let Some(landing_start) = eret_start.checked_add(4) else {
        blockers.push(format!(
            "diagnostic exception vector {location} landing offset overflowed"
        ));
        return None;
    };
    let Some(vector_end) = landing_start.checked_add(4) else {
        blockers.push(format!(
            "diagnostic exception vector {location} end offset overflowed"
        ));
        return None;
    };
    if bytes < vector_end {
        blockers.push(format!(
            "{location} is smaller than diagnostic exception vector slot ({bytes:#x} < {vector_end:#x})"
        ));
        return None;
    }

    Some(DiagnosticExceptionVectorSlotRange {
        vector_start,
        eret_start,
        landing_start,
        vector_end,
    })
}

fn write_diagnostic_exception_vector_slot(
    slot: &mut [u8],
    range: DiagnosticExceptionVectorSlotRange,
) -> bool {
    slot[range.vector_start..range.eret_start].copy_from_slice(&AARCH64_HVC_1.to_le_bytes());
    slot[range.eret_start..range.landing_start].copy_from_slice(&AARCH64_ERET.to_le_bytes());
    slot[range.landing_start..range.vector_end].copy_from_slice(&AARCH64_HVC_0.to_le_bytes());
    let first = u32::from_le_bytes(
        slot[range.vector_start..range.eret_start]
            .try_into()
            .expect("four-byte vector slot"),
    );
    let second = u32::from_le_bytes(
        slot[range.eret_start..range.landing_start]
            .try_into()
            .expect("four-byte ERET vector slot"),
    );
    let third = u32::from_le_bytes(
        slot[range.landing_start..range.vector_end]
            .try_into()
            .expect("four-byte landing vector slot"),
    );
    first == AARCH64_HVC_1 && second == AARCH64_ERET && third == AARCH64_HVC_0
}

fn install_diagnostic_exception_vector_slot_preserving(
    memory: *mut c_void,
    bytes: usize,
    base_offset: usize,
    location: &str,
    blockers: &mut Vec<String>,
) -> Option<DiagnosticExceptionVectorSlotSnapshot> {
    if memory.is_null() {
        blockers.push(format!(
            "diagnostic exception vector {location} pointer was null"
        ));
        return None;
    }
    let range = diagnostic_exception_vector_slot_range(bytes, base_offset, location, blockers)?;
    unsafe {
        let slot = std::slice::from_raw_parts_mut(memory.cast::<u8>(), bytes);
        let original = slot[range.vector_start..range.vector_end]
            .try_into()
            .expect("diagnostic exception vector snapshot is 12 bytes");
        write_diagnostic_exception_vector_slot(slot, range).then_some(
            DiagnosticExceptionVectorSlotSnapshot {
                start: range.vector_start,
                original,
            },
        )
    }
}

fn restore_diagnostic_exception_vector_slot(
    memory: *mut c_void,
    bytes: usize,
    snapshot: DiagnosticExceptionVectorSlotSnapshot,
    location: &str,
    blockers: &mut Vec<String>,
) -> bool {
    if memory.is_null() {
        blockers.push(format!(
            "diagnostic exception vector {location} restore pointer was null"
        ));
        return false;
    }
    let Some(end) = snapshot
        .start
        .checked_add(DIAGNOSTIC_EXCEPTION_VECTOR_SLOT_BYTES)
    else {
        blockers.push(format!(
            "diagnostic exception vector {location} restore end overflowed"
        ));
        return false;
    };
    if bytes < end {
        blockers.push(format!(
            "{location} is smaller than diagnostic exception vector restore slot ({bytes:#x} < {end:#x})"
        ));
        return false;
    }
    unsafe {
        let slot = std::slice::from_raw_parts_mut(memory.cast::<u8>(), bytes);
        slot[snapshot.start..end].copy_from_slice(&snapshot.original);
        slot[snapshot.start..end] == snapshot.original
    }
}

fn populate_diagnostic_exception_vector_slot(
    memory: *mut c_void,
    bytes: usize,
    base_offset: usize,
    location: &str,
    blockers: &mut Vec<String>,
) -> bool {
    if memory.is_null() {
        blockers.push(format!(
            "diagnostic exception vector {location} pointer was null"
        ));
        return false;
    }
    let Some(range) =
        diagnostic_exception_vector_slot_range(bytes, base_offset, location, blockers)
    else {
        return false;
    };
    unsafe {
        let slot = std::slice::from_raw_parts_mut(memory.cast::<u8>(), bytes);
        write_diagnostic_exception_vector_slot(slot, range)
    }
}

fn guest_backing_offset(base: u64, region_start: u64, region_bytes: usize) -> Option<usize> {
    let offset = base.checked_sub(region_start)?;
    (offset < region_bytes as u64)
        .then_some(offset)
        .and_then(|offset| offset.try_into().ok())
}

fn populate_recommended_vector_base_diagnostic_vector_slot(
    recommendation: &WindowsArmUefiVectorBaseRecommendation,
    firmware_memory: *mut c_void,
    vars_memory: *mut c_void,
    guest_ram_memory: *mut c_void,
    slot_bytes: usize,
    guest_ram_bytes: usize,
    blockers: &mut Vec<String>,
) -> bool {
    let base = recommendation.base_virtual_address;
    if let Some(offset) =
        guest_backing_offset(base, WINDOWS_ARM_UEFI_LOW_CODE_ALIAS_IPA, slot_bytes)
            .or_else(|| guest_backing_offset(base, WINDOWS_ARM_UEFI_CODE_IPA, slot_bytes))
    {
        return populate_diagnostic_exception_vector_slot(
            firmware_memory,
            slot_bytes,
            offset,
            "recommended vector-base firmware pflash",
            blockers,
        );
    }
    if let Some(offset) =
        guest_backing_offset(base, WINDOWS_ARM_UEFI_LOW_VARS_ALIAS_IPA, slot_bytes)
            .or_else(|| guest_backing_offset(base, WINDOWS_ARM_UEFI_VARS_IPA, slot_bytes))
    {
        return populate_diagnostic_exception_vector_slot(
            vars_memory,
            slot_bytes,
            offset,
            "recommended vector-base vars pflash",
            blockers,
        );
    }
    if let Some(offset) = guest_backing_offset(base, WINDOWS_ARM_GUEST_RAM_IPA, guest_ram_bytes) {
        return populate_diagnostic_exception_vector_slot(
            guest_ram_memory,
            guest_ram_bytes,
            offset,
            "recommended vector-base guest RAM",
            blockers,
        );
    }

    blockers.push(format!(
        "recommended vector-base {base:#x} does not map to a mutable BridgeVM diagnostic backing"
    ));
    false
}

#[cfg(test)]
mod diagnostic_exception_vector_slot_tests {
    use super::*;

    #[test]
    fn preserving_install_restores_original_low_vector_bytes() {
        let base_offset = 0x40usize;
        let slot_start = base_offset + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET;
        let slot_end = slot_start + DIAGNOSTIC_EXCEPTION_VECTOR_SLOT_BYTES;
        let mut memory = (0..(slot_end + 0x20))
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        let original = memory.clone();
        let mut blockers = Vec::new();

        let snapshot = install_diagnostic_exception_vector_slot_preserving(
            memory.as_mut_ptr().cast(),
            memory.len(),
            base_offset,
            "unit-test",
            &mut blockers,
        )
        .expect("diagnostic vector slot installs");

        assert!(blockers.is_empty());
        assert_eq!(snapshot.start, slot_start);
        assert_eq!(&snapshot.original, &original[slot_start..slot_end]);
        assert_eq!(
            u32::from_le_bytes(memory[slot_start..slot_start + 4].try_into().unwrap()),
            AARCH64_HVC_1
        );
        assert_eq!(
            u32::from_le_bytes(memory[slot_start + 4..slot_start + 8].try_into().unwrap()),
            AARCH64_ERET
        );
        assert_eq!(
            u32::from_le_bytes(memory[slot_start + 8..slot_end].try_into().unwrap()),
            AARCH64_HVC_0
        );
        assert_eq!(&memory[..slot_start], &original[..slot_start]);
        assert_eq!(&memory[slot_end..], &original[slot_end..]);

        assert!(restore_diagnostic_exception_vector_slot(
            memory.as_mut_ptr().cast(),
            memory.len(),
            snapshot,
            "unit-test",
            &mut blockers,
        ));
        assert!(blockers.is_empty());
        assert_eq!(memory, original);
    }

    #[test]
    fn preserving_install_rejects_invalid_slots_without_mutating_memory() {
        let mut blockers = Vec::new();
        let mut memory = vec![0xa5; WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET];
        let original = memory.clone();

        assert!(install_diagnostic_exception_vector_slot_preserving(
            memory.as_mut_ptr().cast(),
            memory.len(),
            0,
            "too-small",
            &mut blockers,
        )
        .is_none());
        assert_eq!(memory, original);
        assert!(blockers
            .iter()
            .any(|blocker| blocker.contains("too-small is smaller")));

        blockers.clear();
        assert!(install_diagnostic_exception_vector_slot_preserving(
            ptr::null_mut(),
            0x1000,
            0,
            "null",
            &mut blockers,
        )
        .is_none());
        assert!(blockers
            .iter()
            .any(|blocker| blocker.contains("null pointer was null")));
    }

    #[test]
    fn non_preserving_populate_writes_only_diagnostic_vector_slot() {
        let base_offset = 0x80usize;
        let slot_start = base_offset + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET;
        let slot_end = slot_start + DIAGNOSTIC_EXCEPTION_VECTOR_SLOT_BYTES;
        let mut memory = (0..(slot_end + 0x20))
            .map(|index| 0xffu8.wrapping_sub((index % 251) as u8))
            .collect::<Vec<_>>();
        let original = memory.clone();
        let mut blockers = Vec::new();

        assert!(populate_diagnostic_exception_vector_slot(
            memory.as_mut_ptr().cast(),
            memory.len(),
            base_offset,
            "unit-test",
            &mut blockers,
        ));

        assert!(blockers.is_empty());
        assert_eq!(&memory[..slot_start], &original[..slot_start]);
        assert_eq!(&memory[slot_end..], &original[slot_end..]);
        assert_eq!(
            u32::from_le_bytes(memory[slot_start..slot_start + 4].try_into().unwrap()),
            AARCH64_HVC_1
        );
        assert_eq!(
            u32::from_le_bytes(memory[slot_start + 4..slot_start + 8].try_into().unwrap()),
            AARCH64_ERET
        );
        assert_eq!(
            u32::from_le_bytes(memory[slot_start + 8..slot_end].try_into().unwrap()),
            AARCH64_HVC_0
        );
    }

    #[test]
    fn recommended_vector_base_slot_install_resolves_low_pflash_alias() {
        let base_offset = WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA as usize;
        let slot_start = base_offset + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET;
        let slot_end = slot_start + DIAGNOSTIC_EXCEPTION_VECTOR_SLOT_BYTES;
        let mut firmware = vec![0_u8; slot_end + 0x20];
        let mut vars = vec![0_u8; 0x1000];
        let mut guest_ram = vec![0_u8; 0x1000];
        let recommendation = WindowsArmUefiVectorBaseRecommendation {
            base_virtual_address: WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA,
            base_physical_address: Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA),
            current_el_spx_sync_instruction_word: Some(0),
            current_el_spx_sync_instruction_hint: "zero",
            reason: "unit-test",
        };
        let mut blockers = Vec::new();

        assert!(populate_recommended_vector_base_diagnostic_vector_slot(
            &recommendation,
            firmware.as_mut_ptr().cast(),
            vars.as_mut_ptr().cast(),
            guest_ram.as_mut_ptr().cast(),
            firmware.len(),
            guest_ram.len(),
            &mut blockers,
        ));

        assert!(blockers.is_empty());
        assert_eq!(
            u32::from_le_bytes(firmware[slot_start..slot_start + 4].try_into().unwrap()),
            AARCH64_HVC_1
        );
        assert_eq!(
            u32::from_le_bytes(firmware[slot_start + 4..slot_start + 8].try_into().unwrap()),
            AARCH64_ERET
        );
        assert_eq!(
            u32::from_le_bytes(firmware[slot_start + 8..slot_end].try_into().unwrap()),
            AARCH64_HVC_0
        );
        assert!(vars.iter().all(|byte| *byte == 0));
        assert!(guest_ram.iter().all(|byte| *byte == 0));
    }

    #[test]
    fn low_vector_recommended_vector_descriptor_remaps_to_real_vector_page() {
        let mut guest_ram = vec![0_u8; 0x4000];
        let tcr_el1 = Some(43);
        let ttbr0_el1 = Some(WINDOWS_ARM_GUEST_RAM_IPA);
        let recommendation = WindowsArmUefiVectorBaseRecommendation {
            base_virtual_address: WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA,
            base_physical_address: Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA),
            current_el_spx_sync_instruction_word: Some(AARCH64_WFI),
            current_el_spx_sync_instruction_hint: "wfi",
            reason: "unit-test",
        };

        let patched = patch_low_vector_recommended_vector_descriptor(
            &recommendation,
            tcr_el1,
            ttbr0_el1,
            ptr::null_mut(),
            ptr::null_mut(),
            guest_ram.as_mut_ptr().cast(),
            guest_ram.len(),
        )
        .expect("low-vector L3 descriptor patches to recommended vector page");

        assert_eq!(patched.0, WINDOWS_ARM_GUEST_RAM_IPA);
        assert_eq!(patched.1, 0);
        assert_eq!(patched.2, 0x200f8f);

        let leaf = read_stage1_leaf_descriptor(
            Some(WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64),
            tcr_el1,
            ttbr0_el1,
            ptr::null(),
            ptr::null(),
            guest_ram.as_ptr().cast(),
            guest_ram.len(),
        )
        .expect("patched low-vector descriptor is readable");

        assert_eq!(leaf.level, 3);
        assert_eq!(leaf.kind, "page");
        assert_eq!(leaf.descriptor, 0x200f8f);
        assert_eq!(
            leaf.output_address,
            Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA)
        );
        assert!(stage1_leaf_is_el1_executable(leaf));
    }
}

fn populate_platform_dtb_guest_ram(
    memory: *mut c_void,
    guest_ram_bytes: usize,
    dtb_blob: &[u8],
    blockers: &mut Vec<String>,
) -> bool {
    if memory.is_null() {
        blockers.push("platform DTB guest RAM pointer was null".to_string());
        return false;
    }
    if dtb_blob.is_empty() {
        blockers.push("platform DTB blob was empty".to_string());
        return false;
    }
    let dtb_offset: usize = match WINDOWS_ARM_PLATFORM_DTB_GUEST_RAM_OFFSET.try_into() {
        Ok(offset) => offset,
        Err(_) => {
            blockers.push("platform DTB guest RAM offset does not fit in usize".to_string());
            return false;
        }
    };
    let Some(dtb_end) = dtb_offset.checked_add(dtb_blob.len()) else {
        blockers.push("platform DTB guest RAM range overflowed".to_string());
        return false;
    };
    if dtb_end > guest_ram_bytes {
        blockers.push(format!(
            "guest RAM is smaller than the platform DTB handoff range ({guest_ram_bytes:#x} < {dtb_end:#x})"
        ));
        return false;
    }

    unsafe {
        ptr::copy_nonoverlapping(
            dtb_blob.as_ptr(),
            memory.cast::<u8>().add(dtb_offset),
            dtb_blob.len(),
        );
        let mapped = std::slice::from_raw_parts(memory.cast::<u8>(), guest_ram_bytes);
        mapped[dtb_offset..dtb_end] == dtb_blob[..]
            && read_be_u32(&mapped[dtb_offset..dtb_end], 0) == Some(FDT_MAGIC)
    }
}

#[derive(Debug, Default)]
struct PflashHvfMapOutcome {
    vm_create_status: Option<i32>,
    firmware_allocate_status: Option<i32>,
    vars_allocate_status: Option<i32>,
    firmware_map_status: Option<i32>,
    vars_map_status: Option<i32>,
    firmware_unmap_status: Option<i32>,
    vars_unmap_status: Option<i32>,
    firmware_deallocate_status: Option<i32>,
    vars_deallocate_status: Option<i32>,
    vm_destroy_status: Option<i32>,
    blockers: Vec<String>,
}

fn pflash_hvf_map_result(
    allowed: bool,
    attempted: bool,
    host: HvfHostCapabilities,
    pflash_map_verified: bool,
    firmware_source_bytes: Option<u64>,
    vars_source_bytes: Option<u64>,
    outcome: PflashHvfMapOutcome,
) -> WindowsArmUefiPflashHvfMapProbe {
    WindowsArmUefiPflashHvfMapProbe {
        allowed,
        attempted,
        vm_created: false,
        firmware_memory_allocated: false,
        vars_memory_allocated: false,
        firmware_memory_populated: false,
        vars_memory_populated: false,
        firmware_memory_mapped: false,
        vars_memory_mapped: false,
        firmware_memory_unmapped: false,
        vars_memory_unmapped: false,
        firmware_memory_deallocated: false,
        vars_memory_deallocated: false,
        vm_destroyed: false,
        host,
        pflash_map_verified,
        firmware_slot_ipa: WINDOWS_ARM_UEFI_CODE_IPA,
        vars_slot_ipa: WINDOWS_ARM_UEFI_VARS_IPA,
        slot_bytes: WINDOWS_ARM_UEFI_SLOT_BYTES,
        firmware_source_bytes,
        vars_source_bytes,
        firmware_map_flags: "read|exec",
        vars_map_flags: "read|write",
        vm_create_status: outcome.vm_create_status,
        firmware_allocate_status: outcome.firmware_allocate_status,
        vars_allocate_status: outcome.vars_allocate_status,
        firmware_map_status: outcome.firmware_map_status,
        vars_map_status: outcome.vars_map_status,
        firmware_unmap_status: outcome.firmware_unmap_status,
        vars_unmap_status: outcome.vars_unmap_status,
        firmware_deallocate_status: outcome.firmware_deallocate_status,
        vars_deallocate_status: outcome.vars_deallocate_status,
        vm_destroy_status: outcome.vm_destroy_status,
        blockers: outcome.blockers,
    }
}

pub fn probe_windows_11_arm_uefi_reset_vector_entry(
    allow_entry: bool,
    pflash_map: WindowsArmUefiPflashMapProbe,
    host: HvfHostCapabilities,
) -> WindowsArmUefiResetVectorEntryProbe {
    let mut blockers = pflash_map.blockers.clone();
    let firmware_source_bytes = pflash_map
        .firmware_slot
        .as_ref()
        .map(|slot| slot.source_bytes);
    let vars_source_bytes = pflash_map.vars_slot.as_ref().map(|slot| slot.source_bytes);

    if !allow_entry {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_UEFI_RESET_VECTOR_ENTRY=1 or pass --allow-entry to map Windows UEFI pflash slots, create one vCPU, set PC to the reset vector, and run once under a watchdog".to_string(),
        );
        return reset_vector_entry_probe_result(
            false,
            false,
            host,
            pflash_map.pflash_map_verified,
            firmware_source_bytes,
            vars_source_bytes,
            blockers,
        );
    }

    if !pflash_map.pflash_map_verified {
        blockers.push(
            "pflash memory-image mapper did not verify code/vars slots; refusing reset-vector entry"
                .to_string(),
        );
        return reset_vector_entry_probe_result(
            true,
            false,
            host,
            false,
            firmware_source_bytes,
            vars_source_bytes,
            blockers,
        );
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return reset_vector_entry_probe_result(
            true,
            false,
            host,
            true,
            firmware_source_bytes,
            vars_source_bytes,
            blockers,
        );
    }

    let slot_bytes_usize: usize = WINDOWS_ARM_UEFI_SLOT_BYTES
        .try_into()
        .expect("Windows UEFI pflash slot fits in usize");
    let mut firmware_memory = ptr::null_mut();
    let mut vars_memory = ptr::null_mut();
    let mut firmware_memory_populated = false;
    let mut vars_memory_populated = false;
    let mut firmware_memory_mapped = false;
    let mut vars_memory_mapped = false;
    let mut vcpu_created = false;
    let mut pc_set = false;
    let mut cpsr_set = false;
    let mut run_attempted = false;
    let mut reset_vector_entry_observed = false;
    let mut firmware_progress_observed = false;
    let mut watchdog_cancel_fired = false;
    let mut vcpu_destroyed = false;
    let mut firmware_memory_unmapped = false;
    let mut vars_memory_unmapped = false;
    let mut firmware_memory_deallocated = false;
    let mut vars_memory_deallocated = false;

    let mut firmware_map_status = None;
    let mut vars_map_status = None;
    let mut vcpu_create_status = None;
    let mut pc_set_status = None;
    let mut cpsr_set_status = None;
    let mut run_status = None;
    let mut exit_reason = None;
    let mut exit_syndrome = None;
    let mut exit_exception_class = None;
    let mut exit_virtual_address = None;
    let mut exit_physical_address = None;
    let mut pc_after_run_status = None;
    let mut pc_after_run = None;
    let mut watchdog_cancel_status = None;
    let mut vcpu_destroy_status = None;
    let mut firmware_unmap_status = None;
    let mut vars_unmap_status = None;
    let mut firmware_deallocate_status = None;
    let mut vars_deallocate_status = None;

    let mut vcpu = 0;
    let mut exit = ptr::null_mut();

    let vm_create_status = unsafe { hv_vm_create(ptr::null_mut()) };
    let vm_created = vm_create_status == HV_SUCCESS;
    if !vm_created {
        blockers.push(format!("hv_vm_create failed: {vm_create_status:#x}"));
        return WindowsArmUefiResetVectorEntryProbe {
            allowed: true,
            attempted: true,
            vm_created,
            firmware_memory_allocated: false,
            vars_memory_allocated: false,
            firmware_memory_populated: false,
            vars_memory_populated: false,
            firmware_memory_mapped: false,
            vars_memory_mapped: false,
            vcpu_created: false,
            pc_set: false,
            cpsr_set: false,
            run_attempted: false,
            reset_vector_entry_observed: false,
            firmware_progress_observed: false,
            watchdog_cancel_fired: false,
            vcpu_destroyed: false,
            firmware_memory_unmapped: false,
            vars_memory_unmapped: false,
            firmware_memory_deallocated: false,
            vars_memory_deallocated: false,
            vm_destroyed: false,
            host,
            pflash_map_verified: true,
            reset_vector_ipa: WINDOWS_ARM_UEFI_CODE_IPA,
            firmware_slot_ipa: WINDOWS_ARM_UEFI_CODE_IPA,
            vars_slot_ipa: WINDOWS_ARM_UEFI_VARS_IPA,
            slot_bytes: WINDOWS_ARM_UEFI_SLOT_BYTES,
            firmware_source_bytes,
            vars_source_bytes,
            firmware_map_flags: "read|exec",
            vars_map_flags: "read|write",
            vm_create_status: Some(vm_create_status),
            firmware_allocate_status: None,
            vars_allocate_status: None,
            firmware_map_status: None,
            vars_map_status: None,
            vcpu_create_status: None,
            pc_set_status: None,
            cpsr_set_status: None,
            run_status: None,
            exit_reason: None,
            exit_syndrome: None,
            exit_exception_class: None,
            exit_virtual_address: None,
            exit_physical_address: None,
            pc_after_run_status: None,
            pc_after_run: None,
            watchdog_cancel_status: None,
            vcpu_destroy_status: None,
            firmware_unmap_status: None,
            vars_unmap_status: None,
            firmware_deallocate_status: None,
            vars_deallocate_status: None,
            vm_destroy_status: None,
            blockers,
        };
    }

    let firmware_status =
        unsafe { hv_vm_allocate(&mut firmware_memory, slot_bytes_usize, HV_ALLOCATE_DEFAULT) };
    let firmware_allocate_status = Some(firmware_status);
    let firmware_memory_allocated = firmware_status == HV_SUCCESS && !firmware_memory.is_null();
    if !firmware_memory_allocated {
        blockers.push(format!(
            "hv_vm_allocate firmware pflash failed: {firmware_status:#x}"
        ));
    }

    let vars_status =
        unsafe { hv_vm_allocate(&mut vars_memory, slot_bytes_usize, HV_ALLOCATE_DEFAULT) };
    let vars_allocate_status = Some(vars_status);
    let vars_memory_allocated = vars_status == HV_SUCCESS && !vars_memory.is_null();
    if !vars_memory_allocated {
        blockers.push(format!(
            "hv_vm_allocate vars pflash failed: {vars_status:#x}"
        ));
    }

    if firmware_memory_allocated {
        firmware_memory_populated = populate_pflash_hvf_memory(
            firmware_memory,
            pflash_map.firmware_slot.as_ref(),
            "firmware",
            &mut blockers,
        );
    }
    if vars_memory_allocated {
        vars_memory_populated = populate_pflash_hvf_memory(
            vars_memory,
            pflash_map.vars_slot.as_ref(),
            "vars",
            &mut blockers,
        );
    }

    if firmware_memory_populated {
        let status = unsafe {
            hv_vm_map(
                firmware_memory,
                WINDOWS_ARM_UEFI_CODE_IPA,
                slot_bytes_usize,
                HV_MEMORY_READ | HV_MEMORY_EXEC,
            )
        };
        firmware_map_status = Some(status);
        firmware_memory_mapped = status == HV_SUCCESS;
        if !firmware_memory_mapped {
            blockers.push(format!("hv_vm_map firmware pflash failed: {status:#x}"));
        }
    }

    if vars_memory_populated {
        let status = unsafe {
            hv_vm_map(
                vars_memory,
                WINDOWS_ARM_UEFI_VARS_IPA,
                slot_bytes_usize,
                HV_MEMORY_READ | HV_MEMORY_WRITE,
            )
        };
        vars_map_status = Some(status);
        vars_memory_mapped = status == HV_SUCCESS;
        if !vars_memory_mapped {
            blockers.push(format!("hv_vm_map vars pflash failed: {status:#x}"));
        }
    }

    if firmware_memory_mapped && vars_memory_mapped {
        let status = unsafe { hv_vcpu_create(&mut vcpu, &mut exit, ptr::null_mut()) };
        vcpu_create_status = Some(status);
        vcpu_created = status == HV_SUCCESS;
        if !vcpu_created {
            blockers.push(format!("hv_vcpu_create failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, WINDOWS_ARM_UEFI_CODE_IPA) };
        pc_set_status = Some(status);
        pc_set = status == HV_SUCCESS;
        if !pc_set {
            blockers.push(format!("hv_vcpu_set_reg(PC) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_CPSR, AARCH64_PSTATE_EL1H_DAIF_MASKED) };
        cpsr_set_status = Some(status);
        cpsr_set = status == HV_SUCCESS;
        if !cpsr_set {
            blockers.push(format!("hv_vcpu_set_reg(CPSR) failed: {status:#x}"));
        }
    }

    if vcpu_created && pc_set && cpsr_set {
        run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        run_status = Some(observation.run_status);
        exit_reason = observation.exit_reason;
        exit_syndrome = observation.exit_syndrome;
        exit_exception_class = exit_syndrome.map(arm_exception_class);
        exit_virtual_address = observation.exit_virtual_address;
        exit_physical_address = observation.exit_physical_address;
        watchdog_cancel_status = observation.watchdog_cancel_status;
        watchdog_cancel_fired = watchdog_cancel_status.is_some();

        if observation.run_status == HV_SUCCESS {
            reset_vector_entry_observed = exit_reason.is_some();
            if !reset_vector_entry_observed {
                blockers
                    .push("hv_vcpu_run returned success without an exit info pointer".to_string());
            }
        } else {
            blockers.push(format!(
                "reset-vector hv_vcpu_run failed: {:#x}",
                observation.run_status
            ));
        }

        let mut pc = 0;
        let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc) };
        pc_after_run_status = Some(status);
        if status == HV_SUCCESS {
            pc_after_run = Some(pc);
            firmware_progress_observed = pc != WINDOWS_ARM_UEFI_CODE_IPA;
        } else {
            blockers.push(format!("hv_vcpu_get_reg(PC) after run failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_destroy(vcpu) };
        vcpu_destroy_status = Some(status);
        vcpu_destroyed = status == HV_SUCCESS;
        if !vcpu_destroyed {
            blockers.push(format!("hv_vcpu_destroy failed: {status:#x}"));
        }
    }

    if vars_memory_mapped {
        let status = unsafe { hv_vm_unmap(WINDOWS_ARM_UEFI_VARS_IPA, slot_bytes_usize) };
        vars_unmap_status = Some(status);
        vars_memory_unmapped = status == HV_SUCCESS;
        if !vars_memory_unmapped {
            blockers.push(format!("hv_vm_unmap vars pflash failed: {status:#x}"));
        }
    }

    if firmware_memory_mapped {
        let status = unsafe { hv_vm_unmap(WINDOWS_ARM_UEFI_CODE_IPA, slot_bytes_usize) };
        firmware_unmap_status = Some(status);
        firmware_memory_unmapped = status == HV_SUCCESS;
        if !firmware_memory_unmapped {
            blockers.push(format!("hv_vm_unmap firmware pflash failed: {status:#x}"));
        }
    }

    let vm_destroy_status = unsafe { hv_vm_destroy() };
    let vm_destroyed = vm_destroy_status == HV_SUCCESS;
    if !vm_destroyed {
        blockers.push(format!("hv_vm_destroy failed: {vm_destroy_status:#x}"));
    }

    if firmware_memory_allocated {
        let status = unsafe { hv_vm_deallocate(firmware_memory, slot_bytes_usize) };
        firmware_deallocate_status = Some(status);
        firmware_memory_deallocated = status == HV_SUCCESS;
        if !firmware_memory_deallocated {
            blockers.push(format!(
                "hv_vm_deallocate firmware pflash failed: {status:#x}"
            ));
        }
    }
    if vars_memory_allocated {
        let status = unsafe { hv_vm_deallocate(vars_memory, slot_bytes_usize) };
        vars_deallocate_status = Some(status);
        vars_memory_deallocated = status == HV_SUCCESS;
        if !vars_memory_deallocated {
            blockers.push(format!("hv_vm_deallocate vars pflash failed: {status:#x}"));
        }
    }

    WindowsArmUefiResetVectorEntryProbe {
        allowed: true,
        attempted: true,
        vm_created,
        firmware_memory_allocated,
        vars_memory_allocated,
        firmware_memory_populated,
        vars_memory_populated,
        firmware_memory_mapped,
        vars_memory_mapped,
        vcpu_created,
        pc_set,
        cpsr_set,
        run_attempted,
        reset_vector_entry_observed,
        firmware_progress_observed,
        watchdog_cancel_fired,
        vcpu_destroyed,
        firmware_memory_unmapped,
        vars_memory_unmapped,
        firmware_memory_deallocated,
        vars_memory_deallocated,
        vm_destroyed,
        host,
        pflash_map_verified: pflash_map.pflash_map_verified,
        reset_vector_ipa: WINDOWS_ARM_UEFI_CODE_IPA,
        firmware_slot_ipa: WINDOWS_ARM_UEFI_CODE_IPA,
        vars_slot_ipa: WINDOWS_ARM_UEFI_VARS_IPA,
        slot_bytes: WINDOWS_ARM_UEFI_SLOT_BYTES,
        firmware_source_bytes,
        vars_source_bytes,
        firmware_map_flags: "read|exec",
        vars_map_flags: "read|write",
        vm_create_status: Some(vm_create_status),
        firmware_allocate_status,
        vars_allocate_status,
        firmware_map_status,
        vars_map_status,
        vcpu_create_status,
        pc_set_status,
        cpsr_set_status,
        run_status,
        exit_reason,
        exit_syndrome,
        exit_exception_class,
        exit_virtual_address,
        exit_physical_address,
        pc_after_run_status,
        pc_after_run,
        watchdog_cancel_status,
        vcpu_destroy_status,
        firmware_unmap_status,
        vars_unmap_status,
        firmware_deallocate_status,
        vars_deallocate_status,
        vm_destroy_status: Some(vm_destroy_status),
        blockers,
    }
}

fn reset_vector_entry_probe_result(
    allowed: bool,
    attempted: bool,
    host: HvfHostCapabilities,
    pflash_map_verified: bool,
    firmware_source_bytes: Option<u64>,
    vars_source_bytes: Option<u64>,
    blockers: Vec<String>,
) -> WindowsArmUefiResetVectorEntryProbe {
    WindowsArmUefiResetVectorEntryProbe {
        allowed,
        attempted,
        vm_created: false,
        firmware_memory_allocated: false,
        vars_memory_allocated: false,
        firmware_memory_populated: false,
        vars_memory_populated: false,
        firmware_memory_mapped: false,
        vars_memory_mapped: false,
        vcpu_created: false,
        pc_set: false,
        cpsr_set: false,
        run_attempted: false,
        reset_vector_entry_observed: false,
        firmware_progress_observed: false,
        watchdog_cancel_fired: false,
        vcpu_destroyed: false,
        firmware_memory_unmapped: false,
        vars_memory_unmapped: false,
        firmware_memory_deallocated: false,
        vars_memory_deallocated: false,
        vm_destroyed: false,
        host,
        pflash_map_verified,
        reset_vector_ipa: WINDOWS_ARM_UEFI_CODE_IPA,
        firmware_slot_ipa: WINDOWS_ARM_UEFI_CODE_IPA,
        vars_slot_ipa: WINDOWS_ARM_UEFI_VARS_IPA,
        slot_bytes: WINDOWS_ARM_UEFI_SLOT_BYTES,
        firmware_source_bytes,
        vars_source_bytes,
        firmware_map_flags: "read|exec",
        vars_map_flags: "read|write",
        vm_create_status: None,
        firmware_allocate_status: None,
        vars_allocate_status: None,
        firmware_map_status: None,
        vars_map_status: None,
        vcpu_create_status: None,
        pc_set_status: None,
        cpsr_set_status: None,
        run_status: None,
        exit_reason: None,
        exit_syndrome: None,
        exit_exception_class: None,
        exit_virtual_address: None,
        exit_physical_address: None,
        pc_after_run_status: None,
        pc_after_run: None,
        watchdog_cancel_status: None,
        vcpu_destroy_status: None,
        firmware_unmap_status: None,
        vars_unmap_status: None,
        firmware_deallocate_status: None,
        vars_deallocate_status: None,
        vm_destroy_status: None,
        blockers,
    }
}

pub fn probe_windows_11_arm_uefi_firmware_run_loop(
    options: WindowsArmUefiFirmwareRunLoopExecutionOptions,
    pflash_map: WindowsArmUefiPflashMapProbe,
    host: HvfHostCapabilities,
) -> WindowsArmUefiFirmwareRunLoopProbe {
    let WindowsArmUefiFirmwareRunLoopExecutionOptions {
        allow_loop,
        requested_exits,
        guest_ram_mib,
        watchdog_timeout_ms,
        map_low_pflash_alias,
        seed_diagnostic_vector,
        seed_guest_ram_diagnostic_vector,
        seed_executable_diagnostic_vector,
        try_recommended_vector_base_vbar,
        continue_after_recommended_vector_base_vbar,
        repair_low_vector_diagnostic_page,
        remap_low_vector_to_recommended_vector,
        continue_after_low_vector_repair,
        restore_low_vector_slot_before_eret,
        wire_interrupt_timer,
        stop_at_first_post_repair_device_boundary,
        installer_iso_path,
        writable_target_disk_path,
    } = options.clone();
    let mut blockers = pflash_map.blockers.clone();
    let diagnostic_vector = windows_arm_diagnostic_vector_selection(
        seed_diagnostic_vector,
        seed_guest_ram_diagnostic_vector,
        seed_executable_diagnostic_vector,
    );
    let diagnostic_vector_seed_requested = diagnostic_vector.requested;
    let diagnostic_vector_location = diagnostic_vector.location;
    let diagnostic_vector_ipa = diagnostic_vector.ipa;
    let diagnostic_vector_request_count = usize::from(seed_diagnostic_vector)
        + usize::from(seed_guest_ram_diagnostic_vector)
        + usize::from(seed_executable_diagnostic_vector);
    if diagnostic_vector_request_count > 1 {
        blockers.push(
            "multiple diagnostic vectors were requested; using the executable candidate when present, otherwise guest RAM".to_string(),
        );
    }
    if seed_executable_diagnostic_vector && !map_low_pflash_alias {
        blockers.push(
            "executable diagnostic vector requires --map-low-pflash-alias so VBAR_EL1 can target the low pflash alias".to_string(),
        );
    }
    if try_recommended_vector_base_vbar && diagnostic_vector_seed_requested {
        blockers.push(
            "recommended vector-base VBAR redirect is ignored while a diagnostic vector seed is requested".to_string(),
        );
    }
    if try_recommended_vector_base_vbar
        && repair_low_vector_diagnostic_page
        && !continue_after_recommended_vector_base_vbar
    {
        blockers.push(
            "recommended vector-base VBAR redirect is ignored while low-vector diagnostic page repair is requested without --continue-after-recommended-vector-base-vbar".to_string(),
        );
    }
    if continue_after_recommended_vector_base_vbar && !try_recommended_vector_base_vbar {
        blockers.push(
            "continue-after-recommended-vector-base-vbar requires --try-recommended-vector-base-vbar; recording the request as a no-op".to_string(),
        );
    }
    if repair_low_vector_diagnostic_page && !map_low_pflash_alias {
        blockers.push(
            "low-vector diagnostic page repair requires --map-low-pflash-alias so the patched low-vector page has a stage-2 pflash backing".to_string(),
        );
    }
    if continue_after_low_vector_repair && !repair_low_vector_diagnostic_page {
        blockers.push(
            "continue-after-low-vector-repair requires --repair-low-vector-diagnostic-page; recording the request as a no-op".to_string(),
        );
    }
    if restore_low_vector_slot_before_eret
        && (!repair_low_vector_diagnostic_page || !continue_after_low_vector_repair)
    {
        blockers.push(
            "restore-low-vector-slot-before-eret requires --repair-low-vector-diagnostic-page and --continue-after-low-vector-repair; recording the request as a no-op".to_string(),
        );
    }
    if remap_low_vector_to_recommended_vector
        && (!repair_low_vector_diagnostic_page || !continue_after_low_vector_repair)
    {
        blockers.push(
            "remap-low-vector-to-recommended-vector requires --repair-low-vector-diagnostic-page and --continue-after-low-vector-repair; recording the request as a no-op".to_string(),
        );
    }
    let firmware_source_bytes = pflash_map
        .firmware_slot
        .as_ref()
        .map(|slot| slot.source_bytes);
    let vars_source_bytes = pflash_map.vars_slot.as_ref().map(|slot| slot.source_bytes);
    let bounded_requested_exits = requested_exits.clamp(1, 64);
    if requested_exits == 0 || requested_exits > 64 {
        blockers.push(
            "--max-exits must be between 1 and 64; using the bounded firmware loop range"
                .to_string(),
        );
    }
    let bounded_watchdog_timeout_ms = watchdog_timeout_ms.clamp(1, 60_000);
    if watchdog_timeout_ms == 0 || watchdog_timeout_ms > 60_000 {
        blockers.push(
            "--watchdog-ms must be between 1 and 60000; using the bounded firmware watchdog range"
                .to_string(),
        );
    }
    let bounded_guest_ram_mib = guest_ram_mib.clamp(1, 4096);
    if guest_ram_mib == 0 || guest_ram_mib > 4096 {
        blockers.push(
            "--guest-ram-mib must be between 1 and 4096; using the bounded guest RAM range"
                .to_string(),
        );
    }
    let guest_ram_bytes = u64::from(bounded_guest_ram_mib) * 1024 * 1024;
    let platform_dtb_blob = build_windows_arm_firmware_run_loop_fdt_blob(guest_ram_bytes);
    let platform_dtb_bytes = platform_dtb_blob.len();
    let platform_dtb_magic = read_be_u32(&platform_dtb_blob, 0).unwrap_or(0);
    let platform_dtb_magic_verified = platform_dtb_magic == FDT_MAGIC;
    if !platform_dtb_magic_verified {
        blockers.push("platform DTB magic did not verify before firmware handoff".to_string());
    }
    let cntv_cval_value = firmware_vtimer_deadline(WINDOWS_ARM_VTIMER_OFFSET_VALUE);
    let cntv_ctl_value = 1;
    let guest_ram_bytes_usize: usize = match guest_ram_bytes.try_into() {
        Ok(bytes) => bytes,
        Err(_) => {
            blockers.push("guest RAM size does not fit in host usize".to_string());
            return firmware_run_loop_probe_result(FirmwareRunLoopProbeResultInput {
                allowed: allow_loop,
                attempted: false,
                host,
                pflash_map_verified: pflash_map.pflash_map_verified,
                guest_ram_bytes,
                requested_exits: bounded_requested_exits,
                watchdog_timeout_ms: bounded_watchdog_timeout_ms,
                options: &options,
                firmware_source_bytes,
                vars_source_bytes,
                blockers,
            });
        }
    };
    let block_devices = windows_arm_firmware_block_devices(
        installer_iso_path.clone(),
        writable_target_disk_path.clone(),
    );

    if !allow_loop {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_UEFI_FIRMWARE_RUN_LOOP=1 or pass --allow-loop to map Windows UEFI pflash plus guest RAM, create one vCPU, and classify bounded firmware exits under a watchdog".to_string(),
        );
        return firmware_run_loop_probe_result(FirmwareRunLoopProbeResultInput {
            allowed: false,
            attempted: false,
            host,
            pflash_map_verified: pflash_map.pflash_map_verified,
            guest_ram_bytes,
            requested_exits: bounded_requested_exits,
            watchdog_timeout_ms: bounded_watchdog_timeout_ms,
            options: &options,
            firmware_source_bytes,
            vars_source_bytes,
            blockers,
        });
    }

    if !pflash_map.pflash_map_verified {
        blockers.push(
            "pflash memory-image mapper did not verify code/vars slots; refusing firmware run-loop entry"
                .to_string(),
        );
        return firmware_run_loop_probe_result(FirmwareRunLoopProbeResultInput {
            allowed: true,
            attempted: false,
            host,
            pflash_map_verified: false,
            guest_ram_bytes,
            requested_exits: bounded_requested_exits,
            watchdog_timeout_ms: bounded_watchdog_timeout_ms,
            options: &options,
            firmware_source_bytes,
            vars_source_bytes,
            blockers,
        });
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return firmware_run_loop_probe_result(FirmwareRunLoopProbeResultInput {
            allowed: true,
            attempted: false,
            host,
            pflash_map_verified: true,
            guest_ram_bytes,
            requested_exits: bounded_requested_exits,
            watchdog_timeout_ms: bounded_watchdog_timeout_ms,
            options: &options,
            firmware_source_bytes,
            vars_source_bytes,
            blockers,
        });
    }

    let slot_bytes_usize: usize = WINDOWS_ARM_UEFI_SLOT_BYTES
        .try_into()
        .expect("Windows UEFI pflash slot fits in usize");
    let mut firmware_memory = ptr::null_mut();
    let mut vars_memory = ptr::null_mut();
    let sp_el1_seed_ipa = windows_arm_initial_sp_el1_ipa(guest_ram_bytes);
    let mut guest_ram_memory = ptr::null_mut();
    let mut firmware_memory_populated = false;
    let mut vars_memory_populated = false;
    let mut firmware_memory_mapped = false;
    let mut vars_memory_mapped = false;
    let mut low_firmware_alias_mapped = false;
    let mut low_vars_alias_mapped = false;
    let mut guest_ram_memory_mapped = false;
    let mut platform_dtb_populated = false;
    let mut diagnostic_vector_populated = false;
    let mut low_vector_diagnostic_page_repaired = false;
    let mut low_vector_diagnostic_page_slot_restored = false;
    let mut low_vector_diagnostic_page_restore_before_eret_attempted = false;
    let mut low_vector_diagnostic_page_slot_snapshot = None;
    let mut low_vector_diagnostic_page_entry_ipa = None;
    let mut low_vector_diagnostic_page_previous_descriptor = None;
    let mut low_vector_diagnostic_page_descriptor = None;
    let mut low_vector_diagnostic_page_repeated_fault_observed = false;
    let mut low_vector_recommended_vector_remap_attempted = false;
    let mut low_vector_recommended_vector_remap_succeeded = false;
    let mut low_vector_recommended_vector_remap_target_physical_address = None;
    let mut low_vector_recommended_vector_remap_descriptor = None;
    let mut low_vector_post_repair = LowVectorPostRepairTelemetry::default();
    let mut low_vector_resume = LowVectorDiagnosticPageResumeTelemetry::new();
    let mut vcpu_created = false;
    let mut pc_set = false;
    let mut x0_dtb_ipa_set = false;
    let mut cpsr_set = false;
    let mut sp_el1_set = false;
    let mut diagnostic_vector_vbar_el1_set = false;
    let mut recommended_vector_base_vbar_attempted = false;
    let mut recommended_vector_base_vbar_set = false;
    let mut recommended_vector_base_vbar_diagnostic_vector_populated = false;
    let mut recommended_vector_base_vbar_source_exit_index = None;
    let mut recommended_vector_base_vbar_target = None;
    let mut recommended_vector_base_vbar_target_physical_address = None;
    let mut recommended_vector_base_vbar_reason = recommended_vector_base_vbar_initial_reason(
        try_recommended_vector_base_vbar,
        diagnostic_vector_seed_requested,
        repair_low_vector_diagnostic_page,
    );
    let mut recommended_vector_base_vbar_current_el_spx_sync_instruction_word = None;
    let mut recommended_vector_base_vbar_current_el_spx_sync_instruction_hint = "not observed";
    let mut recommended_vector_base_vbar_followup_exit_observed = false;
    let mut recommended_vector_base_vbar_followup_exit_index = None;
    let mut recommended_vector_base_vbar_followup_exit_reason = None;
    let mut recommended_vector_base_vbar_followup_exit_diagnosis = "not observed";
    let mut recommended_vector_base_vbar_followup_pc = None;
    let mut recommended_vector_base_vbar_followup_vbar_el1 = None;
    let mut recommended_vector_base_vbar_followup_target_still_set = false;
    let mut recommended_vector_base_vbar_resume_attempted = false;
    let mut recommended_vector_base_vbar_resume_armed = false;
    let mut recommended_vector_base_vbar_resume_original_pc = None;
    let mut recommended_vector_base_vbar_resume_original_elr_el1 = None;
    let mut recommended_vector_base_vbar_resume_original_esr_el1 = None;
    let mut recommended_vector_base_vbar_resume_original_far_el1 = None;
    let mut recommended_vector_base_vbar_resume_original_spsr_el1 = None;
    let mut interrupt_timer_initialized = false;
    let mut run_loop_attempted = false;
    let mut firmware_progress_observed = false;
    let mut unsupported_exit_observed = false;
    let mut watchdog_cancel_fired = false;
    let mut vcpu_destroyed = false;
    let mut firmware_memory_unmapped = false;
    let mut vars_memory_unmapped = false;
    let mut guest_ram_memory_unmapped = false;
    let mut firmware_memory_deallocated = false;
    let mut vars_memory_deallocated = false;
    let mut guest_ram_memory_deallocated = false;

    let mut firmware_allocate_status = None;
    let mut vars_allocate_status = None;
    let mut guest_ram_allocate_status = None;
    let mut firmware_map_status = None;
    let mut vars_map_status = None;
    let mut low_firmware_alias_map_status = None;
    let mut low_vars_alias_map_status = None;
    let mut guest_ram_map_status = None;
    let mut vcpu_create_status = None;
    let mut pc_set_status = None;
    let mut x0_dtb_ipa_set_status = None;
    let mut cpsr_set_status = None;
    let mut sp_el1_set_status = None;
    let mut diagnostic_vector_vbar_el1_set_status = None;
    let mut recommended_vector_base_vbar_set_status = None;
    let mut recommended_vector_base_vbar_resume_vbar_el1_set_status = None;
    let mut recommended_vector_base_vbar_resume_elr_el1_set_status = None;
    let mut recommended_vector_base_vbar_resume_spsr_el1_set_status = None;
    let mut recommended_vector_base_vbar_resume_pc_set_status = None;
    let mut vtimer_offset_set_status = None;
    let mut cntv_cval_set_status = None;
    let mut cntv_ctl_set_status = None;
    let mut vtimer_initial_unmask_status = None;
    let mut last_pending_irq_set_status = None;
    let mut last_device_irq_set_status = None;
    let mut last_device_irq_clear_status = None;
    let mut last_vtimer_unmask_status = None;
    let mut final_pc_status = None;
    let mut final_pc = None;
    let mut vcpu_destroy_status = None;
    let mut firmware_unmap_status = None;
    let mut vars_unmap_status = None;
    let mut low_firmware_alias_unmap_status = None;
    let mut low_vars_alias_unmap_status = None;
    let mut guest_ram_unmap_status = None;
    let mut firmware_deallocate_status = None;
    let mut vars_deallocate_status = None;
    let mut guest_ram_deallocate_status = None;
    let mut exits = Vec::new();
    let mut vtimer_exit_count = 0;
    let mut pending_irq_injected_count = 0;
    let mut device_irq_injected_count = 0;
    let mut device_irq_cleared_count = 0;
    let mut handled_mmio_read_count = 0;
    let mut handled_mmio_write_count = 0;
    let mut handled_pl011_mmio_count = 0;
    let mut handled_pl031_mmio_count = 0;
    let mut handled_gicd_mmio_count = 0;
    let mut handled_gicr_mmio_count = 0;
    let mut handled_virtio_installer_iso_mmio_count = 0;
    let mut handled_virtio_target_disk_mmio_count = 0;
    let mut virtio_queue_notify_count = 0;
    let mut virtio_request_completion_count = 0;
    let mut handled_icc_read_count = 0;
    let mut handled_icc_write_count = 0;
    let mut handled_icc_iar1_read_count = 0;
    let mut handled_icc_eoir1_write_count = 0;
    let mut handled_icc_dir_write_count = 0;
    let mut last_icc_iar1_intid = None;
    let mut last_icc_eoir1_intid = None;
    let mut last_icc_dir_intid = None;
    let mut device_irq_line_asserted = false;
    let mut firmware_mmio_bus = windows_arm_firmware_mmio_bus_with_block_devices(&block_devices);
    let mut gic_cpu_interface = GicV3CpuInterfaceState::new();

    let mut vcpu = 0;
    let mut exit = ptr::null_mut();

    let vm_create_status = unsafe { hv_vm_create(ptr::null_mut()) };
    let vm_created = vm_create_status == HV_SUCCESS;
    if !vm_created {
        blockers.push(format!("hv_vm_create failed: {vm_create_status:#x}"));
    }

    let mut firmware_memory_allocated = false;
    let mut vars_memory_allocated = false;
    let mut guest_ram_memory_allocated = false;

    if vm_created {
        let status =
            unsafe { hv_vm_allocate(&mut firmware_memory, slot_bytes_usize, HV_ALLOCATE_DEFAULT) };
        firmware_allocate_status = Some(status);
        firmware_memory_allocated = status == HV_SUCCESS && !firmware_memory.is_null();
        if !firmware_memory_allocated {
            blockers.push(format!(
                "hv_vm_allocate firmware pflash failed: {status:#x}"
            ));
        }

        let status =
            unsafe { hv_vm_allocate(&mut vars_memory, slot_bytes_usize, HV_ALLOCATE_DEFAULT) };
        vars_allocate_status = Some(status);
        vars_memory_allocated = status == HV_SUCCESS && !vars_memory.is_null();
        if !vars_memory_allocated {
            blockers.push(format!("hv_vm_allocate vars pflash failed: {status:#x}"));
        }

        let status = unsafe {
            hv_vm_allocate(
                &mut guest_ram_memory,
                guest_ram_bytes_usize,
                HV_ALLOCATE_DEFAULT,
            )
        };
        guest_ram_allocate_status = Some(status);
        guest_ram_memory_allocated = status == HV_SUCCESS && !guest_ram_memory.is_null();
        if !guest_ram_memory_allocated {
            blockers.push(format!("hv_vm_allocate guest RAM failed: {status:#x}"));
        }
    }

    if firmware_memory_allocated {
        firmware_memory_populated = populate_pflash_hvf_memory(
            firmware_memory,
            pflash_map.firmware_slot.as_ref(),
            "firmware",
            &mut blockers,
        );
    }
    if vars_memory_allocated {
        vars_memory_populated = populate_pflash_hvf_memory(
            vars_memory,
            pflash_map.vars_slot.as_ref(),
            "vars",
            &mut blockers,
        );
    }

    if firmware_memory_populated {
        let status = unsafe {
            hv_vm_map(
                firmware_memory,
                WINDOWS_ARM_UEFI_CODE_IPA,
                slot_bytes_usize,
                HV_MEMORY_READ | HV_MEMORY_EXEC,
            )
        };
        firmware_map_status = Some(status);
        firmware_memory_mapped = status == HV_SUCCESS;
        if !firmware_memory_mapped {
            blockers.push(format!("hv_vm_map firmware pflash failed: {status:#x}"));
        }
    }

    if vars_memory_populated {
        let status = unsafe {
            hv_vm_map(
                vars_memory,
                WINDOWS_ARM_UEFI_VARS_IPA,
                slot_bytes_usize,
                HV_MEMORY_READ | HV_MEMORY_WRITE,
            )
        };
        vars_map_status = Some(status);
        vars_memory_mapped = status == HV_SUCCESS;
        if !vars_memory_mapped {
            blockers.push(format!("hv_vm_map vars pflash failed: {status:#x}"));
        }
    }

    if diagnostic_vector_seed_requested {
        if seed_executable_diagnostic_vector {
            if firmware_memory_populated {
                diagnostic_vector_populated = populate_diagnostic_exception_vector_slot(
                    firmware_memory,
                    slot_bytes_usize,
                    WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA as usize,
                    "low pflash executable diagnostic candidate",
                    &mut blockers,
                );
            } else {
                blockers.push(
                    "executable diagnostic exception vector requested before firmware pflash population succeeded".to_string(),
                );
            }
        } else if seed_guest_ram_diagnostic_vector {
            if guest_ram_memory_allocated {
                diagnostic_vector_populated = populate_diagnostic_exception_vector_slot(
                    guest_ram_memory,
                    guest_ram_bytes_usize,
                    0,
                    "guest RAM",
                    &mut blockers,
                );
            } else {
                blockers.push(
                    "guest RAM diagnostic exception vector requested before guest RAM allocation succeeded".to_string(),
                );
            }
        } else if firmware_memory_populated {
            diagnostic_vector_populated = populate_diagnostic_exception_vector_slot(
                firmware_memory,
                slot_bytes_usize,
                0,
                "firmware pflash",
                &mut blockers,
            );
        } else {
            blockers.push(
                "pflash diagnostic exception vector requested before firmware pflash population succeeded".to_string(),
            );
        }
    }

    if guest_ram_memory_allocated && platform_dtb_magic_verified {
        platform_dtb_populated = populate_platform_dtb_guest_ram(
            guest_ram_memory,
            guest_ram_bytes_usize,
            &platform_dtb_blob,
            &mut blockers,
        );
    }

    if map_low_pflash_alias && firmware_memory_populated {
        let status = unsafe {
            hv_vm_map(
                firmware_memory,
                WINDOWS_ARM_UEFI_LOW_CODE_ALIAS_IPA,
                slot_bytes_usize,
                HV_MEMORY_READ | HV_MEMORY_EXEC,
            )
        };
        low_firmware_alias_map_status = Some(status);
        low_firmware_alias_mapped = status == HV_SUCCESS;
        if !low_firmware_alias_mapped {
            blockers.push(format!(
                "hv_vm_map low firmware pflash alias failed: {status:#x}"
            ));
        }
    }

    if map_low_pflash_alias && vars_memory_populated {
        let status = unsafe {
            hv_vm_map(
                vars_memory,
                WINDOWS_ARM_UEFI_LOW_VARS_ALIAS_IPA,
                slot_bytes_usize,
                HV_MEMORY_READ | HV_MEMORY_WRITE,
            )
        };
        low_vars_alias_map_status = Some(status);
        low_vars_alias_mapped = status == HV_SUCCESS;
        if !low_vars_alias_mapped {
            blockers.push(format!(
                "hv_vm_map low vars pflash alias failed: {status:#x}"
            ));
        }
    }

    if guest_ram_memory_allocated {
        let status = unsafe {
            hv_vm_map(
                guest_ram_memory,
                WINDOWS_ARM_GUEST_RAM_IPA,
                guest_ram_bytes_usize,
                HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC,
            )
        };
        guest_ram_map_status = Some(status);
        guest_ram_memory_mapped = status == HV_SUCCESS;
        if !guest_ram_memory_mapped {
            blockers.push(format!("hv_vm_map guest RAM failed: {status:#x}"));
        }
    }

    let requested_aliases_ready =
        !map_low_pflash_alias || (low_firmware_alias_mapped && low_vars_alias_mapped);
    if firmware_memory_mapped
        && vars_memory_mapped
        && guest_ram_memory_mapped
        && requested_aliases_ready
    {
        let status = unsafe { hv_vcpu_create(&mut vcpu, &mut exit, ptr::null_mut()) };
        vcpu_create_status = Some(status);
        vcpu_created = status == HV_SUCCESS;
        if !vcpu_created {
            blockers.push(format!("hv_vcpu_create failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, WINDOWS_ARM_UEFI_CODE_IPA) };
        pc_set_status = Some(status);
        pc_set = status == HV_SUCCESS;
        if !pc_set {
            blockers.push(format!("hv_vcpu_set_reg(PC) failed: {status:#x}"));
        }
    }

    if vcpu_created && platform_dtb_populated {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X0, WINDOWS_ARM_PLATFORM_DTB_IPA) };
        x0_dtb_ipa_set_status = Some(status);
        x0_dtb_ipa_set = status == HV_SUCCESS;
        if !x0_dtb_ipa_set {
            blockers.push(format!(
                "hv_vcpu_set_reg(X0=platform DTB IPA) failed: {status:#x}"
            ));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_CPSR, AARCH64_PSTATE_EL1H_DAIF_MASKED) };
        cpsr_set_status = Some(status);
        cpsr_set = status == HV_SUCCESS;
        if !cpsr_set {
            blockers.push(format!("hv_vcpu_set_reg(CPSR) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_SP_EL1, sp_el1_seed_ipa) };
        sp_el1_set_status = Some(status);
        sp_el1_set = status == HV_SUCCESS;
        if !sp_el1_set {
            blockers.push(format!("hv_vcpu_set_sys_reg(SP_EL1) failed: {status:#x}"));
        }
    }

    if vcpu_created && diagnostic_vector_seed_requested && diagnostic_vector_populated {
        let status =
            unsafe { hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_VBAR_EL1, diagnostic_vector_ipa) };
        diagnostic_vector_vbar_el1_set_status = Some(status);
        diagnostic_vector_vbar_el1_set = status == HV_SUCCESS;
        if !diagnostic_vector_vbar_el1_set {
            blockers.push(format!(
                "hv_vcpu_set_sys_reg(VBAR_EL1 diagnostic vector) failed: {status:#x}"
            ));
        }
    }

    if vcpu_created && wire_interrupt_timer {
        let offset_status =
            unsafe { hv_vcpu_set_vtimer_offset(vcpu, WINDOWS_ARM_VTIMER_OFFSET_VALUE) };
        vtimer_offset_set_status = Some(offset_status);
        if offset_status != HV_SUCCESS {
            blockers.push(format!(
                "hv_vcpu_set_vtimer_offset for firmware run-loop failed: {offset_status:#x}"
            ));
        }

        let cval_status =
            unsafe { hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_CNTV_CVAL_EL0, cntv_cval_value) };
        cntv_cval_set_status = Some(cval_status);
        if cval_status != HV_SUCCESS {
            blockers.push(format!(
                "hv_vcpu_set_sys_reg(CNTV_CVAL_EL0) for firmware run-loop failed: {cval_status:#x}"
            ));
        }

        let ctl_status =
            unsafe { hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_CNTV_CTL_EL0, cntv_ctl_value) };
        cntv_ctl_set_status = Some(ctl_status);
        if ctl_status != HV_SUCCESS {
            blockers.push(format!(
                "hv_vcpu_set_sys_reg(CNTV_CTL_EL0) for firmware run-loop failed: {ctl_status:#x}"
            ));
        }

        let unmask_status = unsafe { hv_vcpu_set_vtimer_mask(vcpu, false) };
        vtimer_initial_unmask_status = Some(unmask_status);
        if unmask_status != HV_SUCCESS {
            blockers.push(format!(
                "hv_vcpu_set_vtimer_mask(false) for firmware run-loop failed: {unmask_status:#x}"
            ));
        }

        interrupt_timer_initialized = offset_status == HV_SUCCESS
            && cval_status == HV_SUCCESS
            && ctl_status == HV_SUCCESS
            && unmask_status == HV_SUCCESS;
    }

    let diagnostic_vector_ready =
        !diagnostic_vector_seed_requested || diagnostic_vector_vbar_el1_set;
    let interrupt_timer_ready = !wire_interrupt_timer || interrupt_timer_initialized;
    if vcpu_created
        && pc_set
        && x0_dtb_ipa_set
        && cpsr_set
        && sp_el1_set
        && diagnostic_vector_ready
        && interrupt_timer_ready
    {
        run_loop_attempted = true;
        for index in 1..=bounded_requested_exits {
            let observation =
                run_vcpu_once_with_watchdog_timeout(vcpu, exit, bounded_watchdog_timeout_ms);
            let exit_exception_class = observation.exit_syndrome.map(arm_exception_class);
            let mut pc_after_exit = None;
            let mut pc = 0;
            let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc) };
            let pc_after_exit_status = Some(status);
            final_pc_status = Some(status);
            if status == HV_SUCCESS {
                pc_after_exit = Some(pc);
                final_pc = Some(pc);
                firmware_progress_observed = pc != WINDOWS_ARM_UEFI_CODE_IPA;
            } else {
                blockers.push(format!(
                    "hv_vcpu_get_reg(PC) after firmware exit {index} failed: {status:#x}"
                ));
            }

            let watchdog_blocker = observation
                .watchdog_cancel_status
                .is_some()
                .then(|| format!("firmware run-loop watchdog fired before exit {index} completed"));
            if let Some(blocker) = &watchdog_blocker {
                watchdog_cancel_fired = true;
                blockers.push(blocker.clone());
            }

            let instruction_word_after_exit = read_guest_instruction_word(
                pc_after_exit,
                firmware_memory.cast_const(),
                vars_memory.cast_const(),
                guest_ram_memory.cast_const(),
                guest_ram_bytes_usize,
            );
            let instruction_hint_after_exit = instruction_word_after_exit
                .map(aarch64_instruction_hint)
                .unwrap_or("not observed");
            let x0_after_exit = read_vcpu_reg(vcpu, HV_REG_X0);
            let x1_after_exit = read_vcpu_reg(vcpu, HV_REG_X1);
            let x2_after_exit = read_vcpu_reg(vcpu, HV_REG_X2);
            let x3_after_exit = read_vcpu_reg(vcpu, HV_REG_X3);
            let x4_after_exit = read_vcpu_reg(vcpu, HV_REG_X4);
            let cpsr_after_exit = read_vcpu_reg(vcpu, HV_REG_CPSR);
            let vbar_el1_after_exit = read_vcpu_sys_reg(vcpu, HV_SYS_REG_VBAR_EL1);
            let elr_el1_after_exit = read_vcpu_sys_reg(vcpu, HV_SYS_REG_ELR_EL1);
            let esr_el1_after_exit = read_vcpu_sys_reg(vcpu, HV_SYS_REG_ESR_EL1);
            let far_el1_after_exit = read_vcpu_sys_reg(vcpu, HV_SYS_REG_FAR_EL1);
            let spsr_el1_after_exit = read_vcpu_sys_reg(vcpu, HV_SYS_REG_SPSR_EL1);
            let sctlr_el1_after_exit = read_vcpu_sys_reg(vcpu, HV_SYS_REG_SCTLR_EL1);
            let tcr_el1_after_exit = read_vcpu_sys_reg(vcpu, HV_SYS_REG_TCR_EL1);
            let ttbr0_el1_after_exit = read_vcpu_sys_reg(vcpu, HV_SYS_REG_TTBR0_EL1);
            let ttbr1_el1_after_exit = read_vcpu_sys_reg(vcpu, HV_SYS_REG_TTBR1_EL1);
            let mair_el1_after_exit = read_vcpu_sys_reg(vcpu, HV_SYS_REG_MAIR_EL1);
            let sp_el1_after_exit = read_vcpu_sys_reg(vcpu, HV_SYS_REG_SP_EL1);
            let stage1_memory = WindowsArmKnownGuestMemory {
                firmware_memory: firmware_memory.cast_const(),
                vars_memory: vars_memory.cast_const(),
                guest_ram_memory: guest_ram_memory.cast_const(),
                guest_ram_bytes: guest_ram_bytes_usize,
            };
            let stage1_translation = Stage1TranslationContext {
                tcr_el1: tcr_el1_after_exit,
                ttbr0_el1: ttbr0_el1_after_exit,
                memory: stage1_memory,
            };
            let stage1_addresses = Stage1ExitAddresses {
                pc: pc_after_exit,
                vbar_el1: vbar_el1_after_exit,
                elr_el1: elr_el1_after_exit,
                far_el1: far_el1_after_exit,
                sp_el1: sp_el1_after_exit,
            };
            let pc_stage1_leaf_after_exit = read_stage1_leaf_descriptor(
                pc_after_exit,
                tcr_el1_after_exit,
                ttbr0_el1_after_exit,
                firmware_memory.cast_const(),
                vars_memory.cast_const(),
                guest_ram_memory.cast_const(),
                guest_ram_bytes_usize,
            );
            let stage1_descriptor_samples_after_exit =
                collect_stage1_descriptor_samples(stage1_addresses, stage1_translation);
            let stage1_walk_entries_after_exit =
                collect_stage1_walk_entries(stage1_addresses, stage1_translation);
            let stage1_executable_candidates_after_exit = collect_stage1_executable_candidates(
                tcr_el1_after_exit,
                ttbr0_el1_after_exit,
                firmware_memory.cast_const(),
                vars_memory.cast_const(),
                guest_ram_memory.cast_const(),
                guest_ram_bytes_usize,
            );

            let mut run_loop_exit = WindowsArmUefiFirmwareRunLoopExit {
                index,
                run_status: Some(observation.run_status),
                exit_reason: observation.exit_reason,
                exit_syndrome: observation.exit_syndrome,
                exit_exception_class,
                exit_virtual_address: observation.exit_virtual_address,
                exit_physical_address: observation.exit_physical_address,
                pc_after_exit_status,
                pc_after_exit,
                instruction_word_after_exit,
                instruction_hint_after_exit,
                pc_stage1_leaf_level_after_exit: pc_stage1_leaf_after_exit.map(|leaf| leaf.level),
                pc_stage1_leaf_descriptor_after_exit: pc_stage1_leaf_after_exit
                    .map(|leaf| leaf.descriptor),
                pc_stage1_leaf_descriptor_kind_after_exit: pc_stage1_leaf_after_exit
                    .map(|leaf| leaf.kind)
                    .unwrap_or("not observed"),
                pc_stage1_leaf_pxn_after_exit: pc_stage1_leaf_after_exit.map(|leaf| leaf.pxn),
                pc_stage1_leaf_uxn_after_exit: pc_stage1_leaf_after_exit.map(|leaf| leaf.uxn),
                stage1_descriptor_samples_after_exit,
                stage1_walk_entries_after_exit,
                stage1_executable_candidates_after_exit,
                x0_after_exit,
                x1_after_exit,
                x2_after_exit,
                x3_after_exit,
                x4_after_exit,
                cpsr_after_exit,
                vbar_el1_after_exit,
                elr_el1_after_exit,
                esr_el1_after_exit,
                far_el1_after_exit,
                spsr_el1_after_exit,
                sctlr_el1_after_exit,
                tcr_el1_after_exit,
                ttbr0_el1_after_exit,
                ttbr1_el1_after_exit,
                mair_el1_after_exit,
                sp_el1_after_exit,
                watchdog_cancel_status: observation.watchdog_cancel_status,
                vtimer_auto_mask_get_status: None,
                vtimer_auto_mask_after_exit: None,
                vtimer_rearm_cval_value: None,
                vtimer_rearm_cval_set_status: None,
                vtimer_ppi_pending_recorded: None,
                vtimer_irq_line_assertable: None,
                vtimer_gic_group1_enabled: None,
                vtimer_gic_priority_mask: None,
                vtimer_gic_running_priority: None,
                vtimer_gic_priority_threshold: None,
                vtimer_gic_pending_intid: None,
                vtimer_pending_irq_set_status: None,
                vtimer_unmask_status: None,
                handled: false,
            };

            if low_vector_post_repair.continue_attempted
                && low_vector_resume.armed
                && !low_vector_post_repair.first_exit.observed
            {
                low_vector_post_repair.observe_first_exit(&block_devices, &run_loop_exit);
            }
            if low_vector_post_repair.continue_attempted && low_vector_resume.armed {
                low_vector_post_repair.observe_device_interaction(&block_devices, &run_loop_exit);
            }

            if observation.run_status != HV_SUCCESS {
                unsupported_exit_observed = true;
                blockers.push(format!(
                    "firmware run-loop stopped at hv_vcpu_run failure on exit {index}: {:#x}",
                    observation.run_status
                ));
                exits.push(run_loop_exit);
                break;
            }

            if observation.exit_reason.is_none() {
                unsupported_exit_observed = true;
                blockers.push(format!(
                    "firmware run-loop stopped because exit {index} returned no exit info"
                ));
                exits.push(run_loop_exit);
                break;
            }

            if recommended_vector_base_vbar_set
                && !recommended_vector_base_vbar_followup_exit_observed
                && recommended_vector_base_vbar_source_exit_index
                    .is_some_and(|source_index| index > source_index)
            {
                recommended_vector_base_vbar_followup_exit_observed = true;
                recommended_vector_base_vbar_followup_exit_index = Some(index);
                recommended_vector_base_vbar_followup_exit_reason = run_loop_exit.exit_reason;
                recommended_vector_base_vbar_followup_exit_diagnosis =
                    windows_arm_firmware_run_loop_exit_diagnosis(&run_loop_exit);
                recommended_vector_base_vbar_followup_pc = run_loop_exit.pc_after_exit;
                recommended_vector_base_vbar_followup_vbar_el1 = run_loop_exit.vbar_el1_after_exit;
                recommended_vector_base_vbar_followup_target_still_set =
                    recommended_vector_base_vbar_target
                        .zip(run_loop_exit.vbar_el1_after_exit)
                        .is_some_and(|(target, observed)| target == observed);
            }

            if observation.exit_reason == Some(HV_EXIT_REASON_EXCEPTION) {
                let mmio_ipa = observation
                    .exit_physical_address
                    .or(observation.exit_virtual_address);
                if let (Some(syndrome), Some(mmio_ipa), Some(pc)) =
                    (observation.exit_syndrome, mmio_ipa, pc_after_exit)
                {
                    if windows_arm_device_mmio_contains(mmio_ipa) {
                        let Some(mmio_access) = decode_mmio_data_abort(syndrome) else {
                            unsupported_exit_observed = true;
                            blockers.push(format!(
                                "firmware run-loop stopped at undecodable data-abort MMIO exit {index}: syndrome {syndrome:#x}, ipa {mmio_ipa:#x}"
                            ));
                            exits.push(run_loop_exit);
                            break;
                        };

                        let pc_next = pc.saturating_add(4);
                        let pc_status = if mmio_access.is_write {
                            match read_vcpu_reg(vcpu, u32::from(mmio_access.register)) {
                                Some(value) => {
                                    let value = mask_mmio_value(value, mmio_access.width);
                                    let block_queue_notify =
                                        windows_arm_firmware_block_queue_notify_ipa(
                                            &block_devices,
                                            mmio_ipa,
                                        );
                                    let block_irq_source_may_change =
                                        windows_arm_firmware_block_irq_source_may_change(
                                            &block_devices,
                                            mmio_ipa,
                                            value,
                                        );
                                    let gicd_pending_clear_may_need_source_refresh =
                                        windows_arm_firmware_gicd_pending_clear_may_need_source_refresh(
                                            mmio_ipa,
                                            value,
                                            mmio_access.width,
                                        );
                                    match firmware_mmio_bus.dispatch(MmioAccess::write(
                                        mmio_ipa,
                                        value,
                                        mmio_access.width,
                                    )) {
                                        MmioAction::WriteAccepted { .. } => {
                                            if block_queue_notify {
                                                virtio_queue_notify_count += 1;
                                                let completion_result = unsafe {
                                                    let bytes = std::slice::from_raw_parts_mut(
                                                        guest_ram_memory.cast::<u8>(),
                                                        guest_ram_bytes_usize,
                                                    );
                                                    let mut guest_memory = VirtioGuestMemory::new(
                                                        WINDOWS_ARM_GUEST_RAM_IPA,
                                                        bytes,
                                                    );
                                                    complete_windows_arm_firmware_block_queue_notify(
                                                        &mut firmware_mmio_bus,
                                                        &mut guest_memory,
                                                        &block_devices,
                                                        mmio_ipa,
                                                        value,
                                                    )
                                                };
                                                if let Err(error) = completion_result {
                                                    unsupported_exit_observed = true;
                                                    blockers.push(format!(
                                                        "firmware run-loop VirtIO block queue_notify completion failed on exit {index}: {}",
                                                        error.render_blocker()
                                                    ));
                                                    exits.push(run_loop_exit);
                                                    break;
                                                }
                                                virtio_request_completion_count += 1;
                                            }
                                            let irq_delivery =
                                                service_windows_arm_firmware_gic_irq_line_delivery(
                                                    vcpu,
                                                    &mut firmware_mmio_bus,
                                                    &block_devices,
                                                    &gic_cpu_interface,
                                                    device_irq_line_asserted,
                                                    block_irq_source_may_change
                                                        || gicd_pending_clear_may_need_source_refresh,
                                                );
                                            record_windows_arm_firmware_irq_line_delivery(
                                                irq_delivery,
                                                &mut device_irq_line_asserted,
                                                &mut last_device_irq_set_status,
                                                &mut last_device_irq_clear_status,
                                                &mut device_irq_injected_count,
                                                &mut device_irq_cleared_count,
                                            );
                                            if !irq_delivery.succeeded() {
                                                unsupported_exit_observed = true;
                                                blockers.push(irq_delivery.failure_blocker(index));
                                                exits.push(run_loop_exit);
                                                break;
                                            }
                                            unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, pc_next) }
                                        }
                                        unexpected_action @ (MmioAction::ReadValue(_)
                                        | MmioAction::Unhandled) => {
                                            let handler_result = match unexpected_action {
                                                MmioAction::ReadValue(_) => {
                                                    "device-bus-returned-read-for-write"
                                                }
                                                MmioAction::Unhandled => {
                                                    "device-bus-unhandled-write"
                                                }
                                                MmioAction::WriteAccepted { .. } => {
                                                    "device-bus-write-accepted"
                                                }
                                            };
                                            if low_vector_post_repair.continue_attempted
                                                && low_vector_resume.armed
                                            {
                                                low_vector_post_repair
                                                    .observe_unhandled_mmio_access(
                                                        &block_devices,
                                                        &run_loop_exit,
                                                        mmio_access,
                                                        mmio_ipa,
                                                        Some(value),
                                                        handler_result,
                                                    );
                                            }
                                            unsupported_exit_observed = true;
                                            blockers.push(format!(
                                                "firmware run-loop MMIO write exit {index} was not handled by the device bus: register X{}, width {}, ipa {mmio_ipa:#x}, value {value:#x}",
                                                mmio_access.register, mmio_access.width
                                            ));
                                            exits.push(run_loop_exit);
                                            break;
                                        }
                                    }
                                }
                                None => {
                                    if low_vector_post_repair.continue_attempted
                                        && low_vector_resume.armed
                                    {
                                        low_vector_post_repair.observe_unhandled_mmio_access(
                                            &block_devices,
                                            &run_loop_exit,
                                            mmio_access,
                                            mmio_ipa,
                                            None,
                                            "write-register-read-failed",
                                        );
                                    }
                                    unsupported_exit_observed = true;
                                    blockers.push(format!(
                                        "firmware run-loop could not read X{} for MMIO write exit {index} at ipa {mmio_ipa:#x}",
                                        mmio_access.register
                                    ));
                                    exits.push(run_loop_exit);
                                    break;
                                }
                            }
                        } else {
                            match firmware_mmio_bus
                                .dispatch(MmioAccess::read(mmio_ipa, mmio_access.width))
                            {
                                MmioAction::ReadValue(value) => {
                                    let value = mask_mmio_value(value, mmio_access.width);
                                    let value_status = unsafe {
                                        hv_vcpu_set_reg(
                                            vcpu,
                                            u32::from(mmio_access.register),
                                            value,
                                        )
                                    };
                                    if value_status == HV_SUCCESS {
                                        unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, pc_next) }
                                    } else {
                                        value_status
                                    }
                                }
                                unexpected_action @ (MmioAction::WriteAccepted { .. }
                                | MmioAction::Unhandled) => {
                                    let handler_result = match unexpected_action {
                                        MmioAction::WriteAccepted { .. } => {
                                            "device-bus-returned-write-for-read"
                                        }
                                        MmioAction::Unhandled => "device-bus-unhandled-read",
                                        MmioAction::ReadValue(_) => "device-bus-read-value",
                                    };
                                    if low_vector_post_repair.continue_attempted
                                        && low_vector_resume.armed
                                    {
                                        low_vector_post_repair.observe_unhandled_mmio_access(
                                            &block_devices,
                                            &run_loop_exit,
                                            mmio_access,
                                            mmio_ipa,
                                            None,
                                            handler_result,
                                        );
                                    }
                                    unsupported_exit_observed = true;
                                    blockers.push(format!(
                                        "firmware run-loop MMIO read exit {index} was not handled by the device bus: register X{}, width {}, ipa {mmio_ipa:#x}",
                                        mmio_access.register, mmio_access.width
                                    ));
                                    exits.push(run_loop_exit);
                                    break;
                                }
                            }
                        };

                        if pc_status == HV_SUCCESS {
                            if mmio_access.is_write {
                                handled_mmio_write_count += 1;
                            } else {
                                handled_mmio_read_count += 1;
                            }
                            match windows_arm_firmware_mmio_device_kind(&block_devices, mmio_ipa) {
                                Some(WindowsArmFirmwareMmioDeviceKind::Pl011) => {
                                    handled_pl011_mmio_count += 1;
                                }
                                Some(WindowsArmFirmwareMmioDeviceKind::Pl031) => {
                                    handled_pl031_mmio_count += 1;
                                }
                                Some(WindowsArmFirmwareMmioDeviceKind::GicDistributor) => {
                                    handled_gicd_mmio_count += 1;
                                }
                                Some(WindowsArmFirmwareMmioDeviceKind::GicRedistributor) => {
                                    handled_gicr_mmio_count += 1;
                                }
                                Some(WindowsArmFirmwareMmioDeviceKind::VirtioInstallerIso) => {
                                    handled_virtio_installer_iso_mmio_count += 1;
                                }
                                Some(WindowsArmFirmwareMmioDeviceKind::VirtioTargetDisk) => {
                                    handled_virtio_target_disk_mmio_count += 1;
                                }
                                None => {}
                            }
                            run_loop_exit.handled = true;
                            exits.push(run_loop_exit);
                            if stop_at_first_post_repair_device_boundary
                                && low_vector_post_repair.first_device_interaction_is(index)
                            {
                                break;
                            }
                            continue;
                        }

                        unsupported_exit_observed = true;
                        blockers.push(format!(
                            "firmware run-loop failed to resume after MMIO {} exit {index}: register X{}, width {}, ipa {mmio_ipa:#x}, hv_vcpu_set_reg(PC={pc_next:#x})={pc_status:#x}",
                            mmio_access.access_name(),
                            mmio_access.register,
                            mmio_access.width
                        ));
                        exits.push(run_loop_exit);
                        break;
                    }
                }

                if let (Some(syndrome), Some(pc)) = (observation.exit_syndrome, pc_after_exit) {
                    if let Some(sysreg_access) = decode_system_register_trap(syndrome) {
                        let write_value = if sysreg_access.is_read {
                            None
                        } else if sysreg_access.register == 31 {
                            Some(0)
                        } else {
                            match read_vcpu_reg(vcpu, u32::from(sysreg_access.register)) {
                                Some(value) => Some(value),
                                None => {
                                    if low_vector_post_repair.continue_attempted
                                        && low_vector_resume.armed
                                    {
                                        low_vector_post_repair.observe_unhandled_sysreg_access(
                                            &run_loop_exit,
                                            sysreg_access,
                                            None,
                                            "sysreg-write-register-read-failed",
                                        );
                                    }
                                    unsupported_exit_observed = true;
                                    blockers.push(format!(
                                        "firmware run-loop could not read X{} for GIC CPU-interface sysreg write exit {index}: sys_reg={:#x}",
                                        sysreg_access.register, sysreg_access.sys_reg
                                    ));
                                    exits.push(run_loop_exit);
                                    break;
                                }
                            }
                        };

                        let Some(gic_action) = gic_cpu_interface.handle_system_register_access(
                            &mut firmware_mmio_bus,
                            sysreg_access,
                            write_value,
                        ) else {
                            if low_vector_post_repair.continue_attempted && low_vector_resume.armed
                            {
                                low_vector_post_repair.observe_unhandled_sysreg_access(
                                    &run_loop_exit,
                                    sysreg_access,
                                    write_value,
                                    "sysreg-unhandled",
                                );
                            }
                            unsupported_exit_observed = true;
                            blockers.push(format!(
                                "firmware run-loop GIC CPU-interface sysreg {} exit {index} was not handled: sys_reg={:#x}, op0={}, op1={}, crn={}, crm={}, op2={}, rt=X{}",
                                sysreg_access.access_name(),
                                sysreg_access.sys_reg,
                                sysreg_access.op0,
                                sysreg_access.op1,
                                sysreg_access.crn,
                                sysreg_access.crm,
                                sysreg_access.op2,
                                sysreg_access.register,
                            ));
                            exits.push(run_loop_exit);
                            break;
                        };

                        let pc_next = pc.saturating_add(4);
                        let value_status = match gic_action {
                            GicV3CpuInterfaceAction::Read(value) => {
                                if sysreg_access.register == 31 {
                                    HV_SUCCESS
                                } else {
                                    unsafe {
                                        hv_vcpu_set_reg(
                                            vcpu,
                                            u32::from(sysreg_access.register),
                                            value,
                                        )
                                    }
                                }
                            }
                            GicV3CpuInterfaceAction::Write { .. } => HV_SUCCESS,
                        };
                        if value_status != HV_SUCCESS {
                            unsupported_exit_observed = true;
                            blockers.push(format!(
                                "firmware run-loop failed to inject GIC CPU-interface sysreg read value on exit {index}: sys_reg={:#x}, rt=X{}, hv_vcpu_set_reg={value_status:#x}",
                                sysreg_access.sys_reg, sysreg_access.register
                            ));
                            exits.push(run_loop_exit);
                            break;
                        }

                        if sysreg_access.is_read {
                            handled_icc_read_count += 1;
                            if sysreg_access.sys_reg == ICC_IAR1_EL1_SYSREG {
                                handled_icc_iar1_read_count += 1;
                                if let GicV3CpuInterfaceAction::Read(value) = gic_action {
                                    last_icc_iar1_intid = Some((value & 0x00ff_ffff) as u32);
                                }
                            }
                        } else {
                            handled_icc_write_count += 1;
                            match sysreg_access.sys_reg {
                                ICC_EOIR1_EL1_SYSREG => {
                                    handled_icc_eoir1_write_count += 1;
                                    last_icc_eoir1_intid =
                                        write_value.map(|value| (value & 0x00ff_ffff) as u32);
                                }
                                ICC_DIR_EL1_SYSREG => {
                                    handled_icc_dir_write_count += 1;
                                    last_icc_dir_intid =
                                        write_value.map(|value| (value & 0x00ff_ffff) as u32);
                                }
                                _ => {}
                            }
                        }

                        let GicV3CpuInterfaceAction::Write {
                            refresh_level_sources,
                        } = gic_action
                        else {
                            let irq_delivery = service_windows_arm_firmware_gic_irq_line_delivery(
                                vcpu,
                                &mut firmware_mmio_bus,
                                &block_devices,
                                &gic_cpu_interface,
                                device_irq_line_asserted,
                                false,
                            );
                            record_windows_arm_firmware_irq_line_delivery(
                                irq_delivery,
                                &mut device_irq_line_asserted,
                                &mut last_device_irq_set_status,
                                &mut last_device_irq_clear_status,
                                &mut device_irq_injected_count,
                                &mut device_irq_cleared_count,
                            );
                            if !irq_delivery.succeeded() {
                                unsupported_exit_observed = true;
                                blockers.push(irq_delivery.failure_blocker(index));
                                exits.push(run_loop_exit);
                                break;
                            }
                            let pc_status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, pc_next) };
                            if pc_status == HV_SUCCESS {
                                run_loop_exit.handled = true;
                                exits.push(run_loop_exit);
                                if stop_at_first_post_repair_device_boundary
                                    && low_vector_post_repair.first_device_interaction_is(index)
                                {
                                    break;
                                }
                                continue;
                            }
                            unsupported_exit_observed = true;
                            blockers.push(format!(
                                "firmware run-loop failed to advance PC after GIC CPU-interface sysreg read exit {index}: sys_reg={:#x}, hv_vcpu_set_reg(PC={pc_next:#x})={pc_status:#x}",
                                sysreg_access.sys_reg
                            ));
                            exits.push(run_loop_exit);
                            break;
                        };

                        let irq_delivery = service_windows_arm_firmware_gic_irq_line_delivery(
                            vcpu,
                            &mut firmware_mmio_bus,
                            &block_devices,
                            &gic_cpu_interface,
                            device_irq_line_asserted,
                            refresh_level_sources,
                        );
                        record_windows_arm_firmware_irq_line_delivery(
                            irq_delivery,
                            &mut device_irq_line_asserted,
                            &mut last_device_irq_set_status,
                            &mut last_device_irq_clear_status,
                            &mut device_irq_injected_count,
                            &mut device_irq_cleared_count,
                        );
                        if !irq_delivery.succeeded() {
                            unsupported_exit_observed = true;
                            blockers.push(irq_delivery.failure_blocker(index));
                            exits.push(run_loop_exit);
                            break;
                        }
                        let pc_status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, pc_next) };
                        if pc_status == HV_SUCCESS {
                            run_loop_exit.handled = true;
                            exits.push(run_loop_exit);
                            if stop_at_first_post_repair_device_boundary
                                && low_vector_post_repair.first_device_interaction_is(index)
                            {
                                break;
                            }
                            continue;
                        }

                        unsupported_exit_observed = true;
                        blockers.push(format!(
                            "firmware run-loop failed to advance PC after GIC CPU-interface sysreg write exit {index}: sys_reg={:#x}, hv_vcpu_set_reg(PC={pc_next:#x})={pc_status:#x}",
                            sysreg_access.sys_reg
                        ));
                        exits.push(run_loop_exit);
                        break;
                    }
                }
            }

            if wire_interrupt_timer
                && observation.exit_reason == Some(HV_EXIT_REASON_VTIMER_ACTIVATED)
            {
                vtimer_exit_count += 1;
                let mut auto_masked = false;
                let mask_status = unsafe { hv_vcpu_get_vtimer_mask(vcpu, &mut auto_masked) };
                run_loop_exit.vtimer_auto_mask_get_status = Some(mask_status);
                if mask_status == HV_SUCCESS {
                    run_loop_exit.vtimer_auto_mask_after_exit = Some(auto_masked);
                } else {
                    unsupported_exit_observed = true;
                    blockers.push(format!(
                        "firmware run-loop failed to inspect VTimer auto-mask after exit {index}: hv_vcpu_get_vtimer_mask={mask_status:#x}"
                    ));
                    exits.push(run_loop_exit);
                    break;
                }

                if !auto_masked {
                    unsupported_exit_observed = true;
                    blockers.push(format!(
                        "firmware run-loop VTimer exit {index} was not automatically masked before IRQ injection"
                    ));
                    exits.push(run_loop_exit);
                    break;
                }

                let low_vector_fault =
                    windows_arm_firmware_run_loop_exit_diagnosis_kind(&run_loop_exit)
                        == WindowsArmFirmwareRunLoopDiagnosis::El1LowVectorMmuTranslationFault;
                let defer_to_low_vector_repair = repair_low_vector_diagnostic_page
                    && low_vector_fault
                    && !low_vector_diagnostic_page_repaired;
                let defer_to_low_vector_repeat_guard = repair_low_vector_diagnostic_page
                    && low_vector_fault
                    && low_vector_diagnostic_page_repaired;
                let delivery = service_windows_arm_firmware_vtimer_delivery(
                    vcpu,
                    &mut firmware_mmio_bus,
                    &gic_cpu_interface,
                    device_irq_line_asserted,
                    defer_to_low_vector_repair,
                );
                run_loop_exit.vtimer_rearm_cval_value = Some(delivery.rearm_cval_value);
                run_loop_exit.vtimer_rearm_cval_set_status = Some(delivery.rearm_cval_status);
                run_loop_exit.vtimer_ppi_pending_recorded = Some(delivery.ppi_pending_recorded);
                run_loop_exit.vtimer_irq_line_assertable = Some(delivery.irq_line_should_assert);
                run_loop_exit.vtimer_gic_group1_enabled =
                    Some(delivery.irq_line_snapshot.group1_enabled);
                run_loop_exit.vtimer_gic_priority_mask =
                    Some(delivery.irq_line_snapshot.priority_mask);
                run_loop_exit.vtimer_gic_running_priority =
                    Some(delivery.irq_line_snapshot.running_priority);
                run_loop_exit.vtimer_gic_priority_threshold =
                    Some(delivery.irq_line_snapshot.priority_threshold);
                run_loop_exit.vtimer_gic_pending_intid =
                    Some(delivery.irq_line_snapshot.pending_intid);
                run_loop_exit.vtimer_pending_irq_set_status = delivery.pending_irq_status;
                run_loop_exit.vtimer_unmask_status = delivery.unmask_status;

                if let Some(irq_status) = delivery.pending_irq_status {
                    last_pending_irq_set_status = Some(irq_status);
                    if delivery.irq_line_should_assert {
                        last_device_irq_set_status = Some(irq_status);
                    } else {
                        last_device_irq_clear_status = Some(irq_status);
                    }
                }
                if delivery.device_irq_injected {
                    device_irq_injected_count += 1;
                }
                if delivery.device_irq_cleared {
                    device_irq_cleared_count += 1;
                }
                device_irq_line_asserted = delivery.next_device_irq_line_asserted;
                if let Some(unmask_status) = delivery.unmask_status {
                    last_vtimer_unmask_status = Some(unmask_status);
                }

                if delivery.succeeded() {
                    if delivery.pending_irq_injected() {
                        pending_irq_injected_count += 1;
                    }
                    if defer_to_low_vector_repair || defer_to_low_vector_repeat_guard {
                        // The timer boundary is serviced, but the same snapshot also
                        // exposes the low-vector fault. Let the repair/repeat-fault
                        // handlers below decide whether to patch or stop with telemetry.
                    } else {
                        run_loop_exit.handled = true;
                        exits.push(run_loop_exit);
                        if delivery.unmask_status.is_some() {
                            continue;
                        }
                        break;
                    }
                } else {
                    unsupported_exit_observed = true;
                    blockers.push(delivery.failure_blocker(index));
                    exits.push(run_loop_exit);
                    break;
                }
            }

            if try_recommended_vector_base_vbar
                && !remap_low_vector_to_recommended_vector
                && !diagnostic_vector_seed_requested
                && (!repair_low_vector_diagnostic_page
                    || continue_after_recommended_vector_base_vbar)
                && !recommended_vector_base_vbar_attempted
                && windows_arm_firmware_run_loop_exit_diagnosis_kind(&run_loop_exit)
                    == WindowsArmFirmwareRunLoopDiagnosis::El1LowVectorMmuTranslationFault
            {
                if let Some(recommendation) =
                    recommended_vector_base_vbar_redirect_target(&run_loop_exit)
                {
                    recommended_vector_base_vbar_attempted = true;
                    recommended_vector_base_vbar_source_exit_index = Some(index);
                    recommended_vector_base_vbar_target = Some(recommendation.base_virtual_address);
                    recommended_vector_base_vbar_target_physical_address =
                        recommendation.base_physical_address;
                    recommended_vector_base_vbar_reason = recommendation.reason;
                    recommended_vector_base_vbar_current_el_spx_sync_instruction_word =
                        recommendation.current_el_spx_sync_instruction_word;
                    recommended_vector_base_vbar_current_el_spx_sync_instruction_hint =
                        recommendation.current_el_spx_sync_instruction_hint;
                    if continue_after_recommended_vector_base_vbar {
                        recommended_vector_base_vbar_resume_original_pc = pc_after_exit;
                        recommended_vector_base_vbar_resume_original_elr_el1 = elr_el1_after_exit;
                        recommended_vector_base_vbar_resume_original_esr_el1 = esr_el1_after_exit;
                        recommended_vector_base_vbar_resume_original_far_el1 = far_el1_after_exit;
                        recommended_vector_base_vbar_resume_original_spsr_el1 = spsr_el1_after_exit;
                    }
                    recommended_vector_base_vbar_diagnostic_vector_populated =
                        populate_recommended_vector_base_diagnostic_vector_slot(
                            recommendation,
                            firmware_memory,
                            vars_memory,
                            guest_ram_memory,
                            slot_bytes_usize,
                            guest_ram_bytes_usize,
                            &mut blockers,
                        );
                    if !recommended_vector_base_vbar_diagnostic_vector_populated {
                        unsupported_exit_observed = true;
                        blockers.push(format!(
                            "firmware run-loop could not seed diagnostic vector at recommended vector base on exit {index}: target={:#x}, target_pa={}",
                            recommendation.base_virtual_address,
                            crate::render_optional_u64(recommendation.base_physical_address)
                        ));
                        exits.push(run_loop_exit);
                        break;
                    }
                    diagnostic_vector_populated = true;
                    if repair_low_vector_diagnostic_page {
                        low_vector_resume.capture_original_context(&run_loop_exit);
                        let low_vector_repair = prepare_low_vector_diagnostic_page_repair(
                            LowVectorDiagnosticPageRepairRequest {
                                firmware_memory,
                                vars_memory,
                                guest_ram_memory,
                                slot_bytes: slot_bytes_usize,
                                guest_ram_bytes: guest_ram_bytes_usize,
                                tcr_el1: tcr_el1_after_exit,
                                ttbr0_el1: ttbr0_el1_after_exit,
                                location:
                                    "recommended-vector VBAR low-vector diagnostic page repair",
                                blockers: &mut blockers,
                            },
                        );
                        low_vector_diagnostic_page_slot_snapshot =
                            low_vector_repair.diagnostic_slot_snapshot;
                        low_vector_resume.capture_diagnostic_slot_bytes(
                            low_vector_diagnostic_page_slot_snapshot
                                .map(|snapshot| snapshot.original),
                        );
                        let low_vector_populated = low_vector_repair.vector_populated();
                        if let Some((entry_ipa, previous_descriptor)) =
                            low_vector_repair.patched_descriptor
                        {
                            low_vector_diagnostic_page_entry_ipa = Some(entry_ipa);
                            low_vector_diagnostic_page_previous_descriptor =
                                Some(previous_descriptor);
                            low_vector_diagnostic_page_descriptor =
                                Some(WINDOWS_ARM_LOW_VECTOR_DIAGNOSTIC_PAGE_DESCRIPTOR);
                            low_vector_diagnostic_page_repaired = low_vector_populated;
                        }
                        if !low_vector_diagnostic_page_repaired {
                            unsupported_exit_observed = true;
                            blockers.push(
                                "firmware run-loop could not prepare low-vector diagnostic page repair before recommended-vector original-context resume"
                                    .to_string(),
                            );
                            exits.push(run_loop_exit);
                            break;
                        }
                    }
                    let vbar_status = unsafe {
                        hv_vcpu_set_sys_reg(
                            vcpu,
                            HV_SYS_REG_VBAR_EL1,
                            recommendation.base_virtual_address,
                        )
                    };
                    recommended_vector_base_vbar_set_status = Some(vbar_status);
                    recommended_vector_base_vbar_set = vbar_status == HV_SUCCESS;
                    if recommended_vector_base_vbar_set {
                        run_loop_exit.handled = true;
                        if let Some(blocker) = &watchdog_blocker {
                            blockers.retain(|candidate| candidate != blocker);
                        }
                        exits.push(run_loop_exit);
                        continue;
                    }

                    unsupported_exit_observed = true;
                    blockers.push(format!(
                        "firmware run-loop failed to set VBAR_EL1 to recommended vector base on exit {index}: target={:#x}, hv_vcpu_set_sys_reg={vbar_status:#x}",
                        recommendation.base_virtual_address
                    ));
                    exits.push(run_loop_exit);
                    break;
                }

                recommended_vector_base_vbar_reason = "no recommended vector base candidate";
            }

            if let Some(target) = recommended_vector_base_vbar_target {
                let route = recommended_vector_base_diagnostic_route(target);
                if let Some((eret_pc, landing_pc)) =
                    diagnostic_vector_hvc_eret_recovery_target(&run_loop_exit, route)
                {
                    if continue_after_recommended_vector_base_vbar {
                        if recommended_vector_base_vbar_resume_attempted {
                            unsupported_exit_observed = true;
                            blockers.push(format!(
                                "firmware run-loop repeated recommended-vector diagnostic HVC after original-context resume on exit {index}: original ELR_EL1={}, original SPSR_EL1={}",
                                crate::render_optional_u64(
                                    recommended_vector_base_vbar_resume_original_elr_el1,
                                ),
                                crate::render_optional_u64(
                                    recommended_vector_base_vbar_resume_original_spsr_el1,
                                )
                            ));
                            exits.push(run_loop_exit);
                            break;
                        }
                        recommended_vector_base_vbar_resume_attempted = true;
                        let Some(original_elr_el1) =
                            recommended_vector_base_vbar_resume_original_elr_el1
                        else {
                            unsupported_exit_observed = true;
                            blockers.push(
                                "firmware run-loop reached recommended-vector diagnostic HVC, but original ELR_EL1 was not captured before VBAR redirect"
                                    .to_string(),
                            );
                            exits.push(run_loop_exit);
                            break;
                        };
                        let Some(original_spsr_el1) =
                            recommended_vector_base_vbar_resume_original_spsr_el1
                        else {
                            unsupported_exit_observed = true;
                            blockers.push(
                                "firmware run-loop reached recommended-vector diagnostic HVC, but original SPSR_EL1 was not captured before VBAR redirect"
                                    .to_string(),
                            );
                            exits.push(run_loop_exit);
                            break;
                        };

                        let resume_status = resume_diagnostic_eret_to_original_context(
                            vcpu,
                            original_elr_el1,
                            original_spsr_el1,
                            eret_pc,
                            repair_low_vector_diagnostic_page
                                && low_vector_diagnostic_page_repaired,
                        );
                        let elr_status = resume_status.elr_status;
                        let vbar_status = resume_status.vbar_effective_status();
                        let spsr_status = resume_status.spsr_status;
                        let pc_status = resume_status.pc_status;
                        recommended_vector_base_vbar_resume_elr_el1_set_status = Some(elr_status);
                        recommended_vector_base_vbar_resume_vbar_el1_set_status =
                            resume_status.vbar_status;
                        recommended_vector_base_vbar_resume_spsr_el1_set_status = Some(spsr_status);
                        recommended_vector_base_vbar_resume_pc_set_status = Some(pc_status);

                        if resume_status.succeeded() {
                            recommended_vector_base_vbar_resume_armed = true;
                            run_loop_exit.handled = true;
                            if let Some(blocker) = &watchdog_blocker {
                                blockers.retain(|candidate| candidate != blocker);
                            }
                            exits.push(run_loop_exit);
                            continue;
                        }

                        unsupported_exit_observed = true;
                        blockers.push(format!(
                            "firmware run-loop failed to arm recommended-vector diagnostic ERET resume to original context: hv_vcpu_set_sys_reg(ELR_EL1={original_elr_el1:#x})={elr_status:#x}, hv_vcpu_set_sys_reg(VBAR_EL1=0x0)={vbar_status:#x}, hv_vcpu_set_sys_reg(SPSR_EL1={original_spsr_el1:#x})={spsr_status:#x}, hv_vcpu_set_reg(PC={eret_pc:#x})={pc_status:#x}"
                        ));
                        exits.push(run_loop_exit);
                        break;
                    }

                    let route_status =
                        route_diagnostic_hvc_exit_through_eret_landing(vcpu, eret_pc, landing_pc);
                    let elr_status = route_status.elr_status;
                    let pc_status = route_status.pc_status;
                    if route_status.succeeded() {
                        run_loop_exit.handled = true;
                        exits.push(run_loop_exit);
                        continue;
                    }

                    unsupported_exit_observed = true;
                    blockers.push(format!(
                        "firmware run-loop failed to route recommended-vector diagnostic HVC exit {index} through ERET landing: hv_vcpu_set_sys_reg(ELR_EL1={landing_pc:#x})={elr_status:#x}, hv_vcpu_set_reg(PC={eret_pc:#x})={pc_status:#x}"
                    ));
                    exits.push(run_loop_exit);
                    break;
                }

                if diagnostic_vector_eret_landing_stop(&run_loop_exit, route) {
                    exits.push(run_loop_exit);
                    break;
                }
            }

            if let Some((eret_pc, landing_pc)) =
                executable_diagnostic_vector_hvc_eret_recovery_target(&run_loop_exit)
            {
                let route_status =
                    route_diagnostic_hvc_exit_through_eret_landing(vcpu, eret_pc, landing_pc);
                let elr_status = route_status.elr_status;
                let pc_status = route_status.pc_status;
                if route_status.succeeded() {
                    run_loop_exit.handled = true;
                    exits.push(run_loop_exit);
                    continue;
                }

                unsupported_exit_observed = true;
                blockers.push(format!(
                    "firmware run-loop failed to route executable diagnostic HVC exit {index} through ERET landing: hv_vcpu_set_sys_reg(ELR_EL1={landing_pc:#x})={elr_status:#x}, hv_vcpu_set_reg(PC={eret_pc:#x})={pc_status:#x}"
                ));
                exits.push(run_loop_exit);
                break;
            }

            if executable_diagnostic_vector_eret_landing_stop(&run_loop_exit) {
                exits.push(run_loop_exit);
                break;
            }

            if repair_low_vector_diagnostic_page
                && low_vector_diagnostic_page_repaired
                && windows_arm_firmware_run_loop_exit_diagnosis_kind(&run_loop_exit)
                    == WindowsArmFirmwareRunLoopDiagnosis::El1LowVectorMmuTranslationFault
            {
                low_vector_diagnostic_page_repeated_fault_observed = true;
                if continue_after_low_vector_repair {
                    low_vector_post_repair.observe_unsupported_exit(&run_loop_exit);
                }
                unsupported_exit_observed = true;
                blockers.push(format!(
                    "firmware run-loop saw a repeated low-vector stage-1 translation fault after diagnostic page repair: entry_ipa={}, previous_descriptor={}, patched_descriptor={}",
                    crate::render_optional_u64(low_vector_diagnostic_page_entry_ipa),
                    crate::render_optional_u64(low_vector_diagnostic_page_previous_descriptor),
                    crate::render_optional_u64(low_vector_diagnostic_page_descriptor)
                ));
                exits.push(run_loop_exit);
                break;
            }

            if repair_low_vector_diagnostic_page
                && !low_vector_diagnostic_page_repaired
                && windows_arm_firmware_run_loop_exit_diagnosis_kind(&run_loop_exit)
                    == WindowsArmFirmwareRunLoopDiagnosis::El1LowVectorMmuTranslationFault
            {
                low_vector_resume.capture_original_context(&run_loop_exit);
                if remap_low_vector_to_recommended_vector && continue_after_low_vector_repair {
                    low_vector_recommended_vector_remap_attempted = true;
                    if let Some(recommendation) =
                        low_vector_recommended_vector_remap_target(&run_loop_exit)
                    {
                        low_vector_recommended_vector_remap_target_physical_address =
                            recommendation.base_physical_address;
                        if let (Some(original_elr_el1), Some(original_spsr_el1)) = (
                            low_vector_resume.original_elr_el1,
                            low_vector_resume.original_spsr_el1,
                        ) {
                            if let Some((entry_ipa, previous_descriptor, descriptor)) =
                                patch_low_vector_recommended_vector_descriptor(
                                    recommendation,
                                    tcr_el1_after_exit,
                                    ttbr0_el1_after_exit,
                                    firmware_memory,
                                    vars_memory,
                                    guest_ram_memory,
                                    guest_ram_bytes_usize,
                                )
                            {
                                low_vector_diagnostic_page_entry_ipa = Some(entry_ipa);
                                low_vector_diagnostic_page_previous_descriptor =
                                    Some(previous_descriptor);
                                low_vector_diagnostic_page_descriptor = Some(descriptor);
                                low_vector_recommended_vector_remap_descriptor = Some(descriptor);
                                let cpsr_status = unsafe {
                                    hv_vcpu_set_reg(vcpu, HV_REG_CPSR, original_spsr_el1)
                                };
                                let pc_status = if cpsr_status == HV_SUCCESS {
                                    unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, original_elr_el1) }
                                } else {
                                    cpsr_status
                                };
                                low_vector_resume
                                    .record_direct_resume_status(cpsr_status, pc_status);

                                if cpsr_status == HV_SUCCESS && pc_status == HV_SUCCESS {
                                    low_vector_recommended_vector_remap_succeeded = true;
                                    low_vector_diagnostic_page_repaired = true;
                                    low_vector_resume.mark_attempted();
                                    low_vector_resume.mark_armed();
                                    low_vector_post_repair.mark_continue_attempted();
                                    run_loop_exit.handled = true;
                                    if let Some(blocker) = &watchdog_blocker {
                                        blockers.retain(|candidate| candidate != blocker);
                                    }
                                    exits.push(run_loop_exit);
                                    continue;
                                }

                                unsupported_exit_observed = true;
                                blockers.push(format!(
                                    "firmware run-loop patched low-vector descriptor at {entry_ipa:#x} from {previous_descriptor:#x} to recommended-vector descriptor {descriptor:#x}, but failed to resume original context directly: hv_vcpu_set_reg(CPSR={original_spsr_el1:#x})={cpsr_status:#x}, hv_vcpu_set_reg(PC={original_elr_el1:#x})={pc_status:#x}"
                                ));
                                exits.push(run_loop_exit);
                                break;
                            }
                        }
                    }
                }
                let low_vector_repair = prepare_low_vector_diagnostic_page_repair(
                    LowVectorDiagnosticPageRepairRequest {
                        firmware_memory,
                        vars_memory,
                        guest_ram_memory,
                        slot_bytes: slot_bytes_usize,
                        guest_ram_bytes: guest_ram_bytes_usize,
                        tcr_el1: tcr_el1_after_exit,
                        ttbr0_el1: ttbr0_el1_after_exit,
                        location: "low-vector diagnostic page repair",
                        blockers: &mut blockers,
                    },
                );
                low_vector_diagnostic_page_slot_snapshot =
                    low_vector_repair.diagnostic_slot_snapshot;
                low_vector_resume.capture_diagnostic_slot_bytes(
                    low_vector_diagnostic_page_slot_snapshot.map(|snapshot| snapshot.original),
                );
                let vector_populated = low_vector_repair.vector_populated();
                if vector_populated {
                    diagnostic_vector_populated = true;
                }
                if let Some((entry_ipa, previous_descriptor)) = low_vector_repair.patched_descriptor
                {
                    low_vector_diagnostic_page_entry_ipa = Some(entry_ipa);
                    low_vector_diagnostic_page_previous_descriptor = Some(previous_descriptor);
                    low_vector_diagnostic_page_descriptor =
                        Some(WINDOWS_ARM_LOW_VECTOR_DIAGNOSTIC_PAGE_DESCRIPTOR);
                    let pc_status = unsafe {
                        hv_vcpu_set_reg(
                            vcpu,
                            HV_REG_PC,
                            WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64,
                        )
                    };
                    if vector_populated && pc_status == HV_SUCCESS {
                        low_vector_diagnostic_page_repaired = true;
                        run_loop_exit.handled = true;
                        if let Some(blocker) = &watchdog_blocker {
                            blockers.retain(|candidate| candidate != blocker);
                        }
                        exits.push(run_loop_exit);
                        continue;
                    }
                    unsupported_exit_observed = true;
                    blockers.push(format!(
                        "firmware run-loop patched low-vector page descriptor at {entry_ipa:#x} from {previous_descriptor:#x} to {:#x}, but failed to resume at the low vector: vector_populated={vector_populated}, hv_vcpu_set_reg(PC=0x200)={pc_status:#x}",
                        WINDOWS_ARM_LOW_VECTOR_DIAGNOSTIC_PAGE_DESCRIPTOR
                    ));
                    exits.push(run_loop_exit);
                    break;
                }

                unsupported_exit_observed = true;
                blockers.push(
                    "firmware run-loop could not find or patch the low-vector stage-1 L3 descriptor for diagnostic page repair"
                        .to_string(),
                );
                exits.push(run_loop_exit);
                break;
            }

            if let Some((eret_pc, landing_pc)) =
                low_vector_diagnostic_page_hvc_eret_recovery_target(&run_loop_exit)
            {
                let route_status =
                    route_diagnostic_hvc_exit_through_eret_landing(vcpu, eret_pc, landing_pc);
                let elr_status = route_status.elr_status;
                let pc_status = route_status.pc_status;
                if route_status.succeeded() {
                    run_loop_exit.handled = true;
                    exits.push(run_loop_exit);
                    continue;
                }

                unsupported_exit_observed = true;
                blockers.push(format!(
                    "firmware run-loop failed to route low-vector diagnostic HVC exit {index} through ERET landing: hv_vcpu_set_sys_reg(ELR_EL1={landing_pc:#x})={elr_status:#x}, hv_vcpu_set_reg(PC={eret_pc:#x})={pc_status:#x}"
                ));
                exits.push(run_loop_exit);
                break;
            }

            if low_vector_diagnostic_page_eret_landing_stop(&run_loop_exit) {
                if repair_low_vector_diagnostic_page
                    && low_vector_diagnostic_page_repaired
                    && !low_vector_resume.attempted
                {
                    low_vector_resume.mark_attempted();
                    if continue_after_low_vector_repair {
                        low_vector_post_repair.mark_continue_attempted();
                    }
                    let Some(original_elr_el1) = low_vector_resume.original_elr_el1 else {
                        unsupported_exit_observed = true;
                        blockers.push(
                            "firmware run-loop reached low-vector diagnostic ERET landing, but original ELR_EL1 was not captured before repair"
                                .to_string(),
                        );
                        exits.push(run_loop_exit);
                        break;
                    };
                    let Some(original_spsr_el1) = low_vector_resume.original_spsr_el1 else {
                        unsupported_exit_observed = true;
                        blockers.push(
                            "firmware run-loop reached low-vector diagnostic ERET landing, but original SPSR_EL1 was not captured before repair"
                                .to_string(),
                        );
                        exits.push(run_loop_exit);
                        break;
                    };

                    let mut eret_pc = low_vector_diagnostic_page_route().eret_pc();
                    if restore_low_vector_slot_before_eret && continue_after_low_vector_repair {
                        low_vector_diagnostic_page_restore_before_eret_attempted = true;
                        let Some(snapshot) = low_vector_diagnostic_page_slot_snapshot else {
                            unsupported_exit_observed = true;
                            blockers.push(
                                "firmware run-loop requested low-vector slot restore before ERET, but no preserved low-vector diagnostic slot snapshot was captured"
                                    .to_string(),
                            );
                            exits.push(run_loop_exit);
                            break;
                        };
                        let trampoline_snapshot =
                            install_diagnostic_exception_vector_slot_preserving(
                                firmware_memory,
                                slot_bytes_usize,
                                WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA as usize,
                                "executable ERET trampoline for low-vector restore",
                                &mut blockers,
                            );
                        if trampoline_snapshot.is_none() {
                            unsupported_exit_observed = true;
                            blockers.push(
                                "firmware run-loop could not populate executable ERET trampoline before low-vector slot restore"
                                    .to_string(),
                            );
                            exits.push(run_loop_exit);
                            break;
                        }
                        if !restore_diagnostic_exception_vector_slot(
                            firmware_memory,
                            slot_bytes_usize,
                            snapshot,
                            "low-vector diagnostic page restore before ERET",
                            &mut blockers,
                        ) {
                            unsupported_exit_observed = true;
                            blockers.push(
                                "firmware run-loop failed to restore the preserved low-vector slot before ERET"
                                    .to_string(),
                            );
                            exits.push(run_loop_exit);
                            break;
                        }
                        low_vector_diagnostic_page_slot_restored = true;
                        eret_pc = executable_diagnostic_vector_route().eret_pc();
                    }

                    let resume_target_instruction_before_eret = read_guest_instruction_word(
                        Some(original_elr_el1),
                        firmware_memory.cast_const(),
                        vars_memory.cast_const(),
                        guest_ram_memory.cast_const(),
                        guest_ram_bytes_usize,
                    );
                    let resume_target_stage1_leaf_before_eret = read_stage1_leaf_descriptor(
                        Some(original_elr_el1),
                        tcr_el1_after_exit,
                        ttbr0_el1_after_exit,
                        firmware_memory.cast_const(),
                        vars_memory.cast_const(),
                        guest_ram_memory.cast_const(),
                        guest_ram_bytes_usize,
                    );
                    low_vector_resume.record_eret_target_snapshot(
                        resume_target_instruction_before_eret,
                        resume_target_stage1_leaf_before_eret.map(|leaf| leaf.descriptor),
                        resume_target_stage1_leaf_before_eret
                            .map(|leaf| leaf.kind)
                            .unwrap_or("not observed"),
                    );
                    let resume_status = arm_diagnostic_eret_resume(
                        vcpu,
                        &mut low_vector_resume,
                        original_elr_el1,
                        original_spsr_el1,
                        eret_pc,
                    );
                    let elr_status = resume_status.elr_status;
                    let spsr_status = resume_status.spsr_status;
                    let pc_status = resume_status.pc_status;

                    if resume_status.succeeded() {
                        run_loop_exit.handled = true;
                        exits.push(run_loop_exit);
                        continue;
                    }

                    unsupported_exit_observed = true;
                    if continue_after_low_vector_repair {
                        blockers.push(format!(
                            "firmware run-loop failed to keep the repaired low-vector diagnostic page installed and arm ERET resume to original context: hv_vcpu_set_sys_reg(ELR_EL1={original_elr_el1:#x})={elr_status:#x}, hv_vcpu_set_sys_reg(SPSR_EL1={original_spsr_el1:#x})={spsr_status:#x}, hv_vcpu_set_reg(PC={eret_pc:#x})={pc_status:#x}"
                        ));
                    } else {
                        blockers.push(format!(
                            "firmware run-loop failed to arm low-vector diagnostic ERET resume to original context: hv_vcpu_set_sys_reg(ELR_EL1={original_elr_el1:#x})={elr_status:#x}, hv_vcpu_set_sys_reg(SPSR_EL1={original_spsr_el1:#x})={spsr_status:#x}, hv_vcpu_set_reg(PC={eret_pc:#x})={pc_status:#x}"
                        ));
                    }
                    exits.push(run_loop_exit);
                    break;
                }
                exits.push(run_loop_exit);
                break;
            }

            let reason_name = observation
                .exit_reason
                .map(hv_exit_reason_name)
                .unwrap_or("not observed");
            let exception_class_name = exit_exception_class
                .map(arm_exception_class_name)
                .unwrap_or("not observed");
            if low_vector_post_repair.continue_attempted {
                low_vector_post_repair.observe_unsupported_exit(&run_loop_exit);
            }
            unsupported_exit_observed = true;
            blockers.push(format!(
                "firmware run-loop stopped at unsupported exit {index}: reason {reason_name}, exception class {exception_class_name}"
            ));
            exits.push(run_loop_exit);
            break;
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_destroy(vcpu) };
        vcpu_destroy_status = Some(status);
        vcpu_destroyed = status == HV_SUCCESS;
        if !vcpu_destroyed {
            blockers.push(format!("hv_vcpu_destroy failed: {status:#x}"));
        }
    }

    if guest_ram_memory_mapped {
        let status = unsafe { hv_vm_unmap(WINDOWS_ARM_GUEST_RAM_IPA, guest_ram_bytes_usize) };
        guest_ram_unmap_status = Some(status);
        guest_ram_memory_unmapped = status == HV_SUCCESS;
        if !guest_ram_memory_unmapped {
            blockers.push(format!("hv_vm_unmap guest RAM failed: {status:#x}"));
        }
    }

    if vars_memory_mapped {
        let status = unsafe { hv_vm_unmap(WINDOWS_ARM_UEFI_VARS_IPA, slot_bytes_usize) };
        vars_unmap_status = Some(status);
        vars_memory_unmapped = status == HV_SUCCESS;
        if !vars_memory_unmapped {
            blockers.push(format!("hv_vm_unmap vars pflash failed: {status:#x}"));
        }
    }

    if low_vars_alias_mapped {
        let status = unsafe { hv_vm_unmap(WINDOWS_ARM_UEFI_LOW_VARS_ALIAS_IPA, slot_bytes_usize) };
        low_vars_alias_unmap_status = Some(status);
        if status != HV_SUCCESS {
            blockers.push(format!(
                "hv_vm_unmap low vars pflash alias failed: {status:#x}"
            ));
        }
    }

    if low_firmware_alias_mapped {
        let status = unsafe { hv_vm_unmap(WINDOWS_ARM_UEFI_LOW_CODE_ALIAS_IPA, slot_bytes_usize) };
        low_firmware_alias_unmap_status = Some(status);
        if status != HV_SUCCESS {
            blockers.push(format!(
                "hv_vm_unmap low firmware pflash alias failed: {status:#x}"
            ));
        }
    }

    if firmware_memory_mapped {
        let status = unsafe { hv_vm_unmap(WINDOWS_ARM_UEFI_CODE_IPA, slot_bytes_usize) };
        firmware_unmap_status = Some(status);
        firmware_memory_unmapped = status == HV_SUCCESS;
        if !firmware_memory_unmapped {
            blockers.push(format!("hv_vm_unmap firmware pflash failed: {status:#x}"));
        }
    }

    let vm_destroy_status = if vm_created {
        let status = unsafe { hv_vm_destroy() };
        if status != HV_SUCCESS {
            blockers.push(format!("hv_vm_destroy failed: {status:#x}"));
        }
        Some(status)
    } else {
        None
    };
    let vm_destroyed = vm_destroy_status == Some(HV_SUCCESS);

    if firmware_memory_allocated {
        let status = unsafe { hv_vm_deallocate(firmware_memory, slot_bytes_usize) };
        firmware_deallocate_status = Some(status);
        firmware_memory_deallocated = status == HV_SUCCESS;
        if !firmware_memory_deallocated {
            blockers.push(format!(
                "hv_vm_deallocate firmware pflash failed: {status:#x}"
            ));
        }
    }
    if vars_memory_allocated {
        let status = unsafe { hv_vm_deallocate(vars_memory, slot_bytes_usize) };
        vars_deallocate_status = Some(status);
        vars_memory_deallocated = status == HV_SUCCESS;
        if !vars_memory_deallocated {
            blockers.push(format!("hv_vm_deallocate vars pflash failed: {status:#x}"));
        }
    }
    if guest_ram_memory_allocated {
        let status = unsafe { hv_vm_deallocate(guest_ram_memory, guest_ram_bytes_usize) };
        guest_ram_deallocate_status = Some(status);
        guest_ram_memory_deallocated = status == HV_SUCCESS;
        if !guest_ram_memory_deallocated {
            blockers.push(format!("hv_vm_deallocate guest RAM failed: {status:#x}"));
        }
    }

    WindowsArmUefiFirmwareRunLoopProbe {
        allowed: true,
        attempted: true,
        vm_created,
        firmware_memory_allocated,
        vars_memory_allocated,
        guest_ram_memory_allocated,
        firmware_memory_populated,
        vars_memory_populated,
        firmware_memory_mapped,
        vars_memory_mapped,
        low_firmware_alias_mapped,
        low_vars_alias_mapped,
        guest_ram_memory_mapped,
        platform_dtb_populated,
        diagnostic_vector_seed_requested,
        diagnostic_vector_populated,
        low_vector_diagnostic_page_repair_requested: repair_low_vector_diagnostic_page,
        low_vector_diagnostic_page_repaired,
        low_vector_diagnostic_page_slot_restored,
        low_vector_diagnostic_page_restore_before_eret_requested:
            restore_low_vector_slot_before_eret,
        low_vector_diagnostic_page_restore_before_eret_attempted,
        low_vector_diagnostic_page_entry_ipa,
        low_vector_diagnostic_page_previous_descriptor,
        low_vector_diagnostic_page_descriptor,
        low_vector_diagnostic_page_repeated_fault_observed,
        low_vector_recommended_vector_remap_requested: remap_low_vector_to_recommended_vector,
        low_vector_recommended_vector_remap_attempted,
        low_vector_recommended_vector_remap_succeeded,
        low_vector_recommended_vector_remap_target_physical_address,
        low_vector_recommended_vector_remap_descriptor,
        low_vector_post_repair_continue_requested: continue_after_low_vector_repair,
        low_vector_post_repair_continue_attempted: low_vector_post_repair.continue_attempted,
        stop_at_first_post_repair_device_boundary_requested:
            stop_at_first_post_repair_device_boundary,
        low_vector_post_repair_unsupported_exit_observed: low_vector_post_repair
            .unsupported_exit_observed,
        low_vector_post_repair_unsupported_exit_reason: low_vector_post_repair
            .unsupported_exit_reason,
        low_vector_post_repair_unsupported_exit_diagnosis: low_vector_post_repair
            .unsupported_exit_diagnosis,
        low_vector_post_repair_first_exit_observed: low_vector_post_repair.first_exit.observed,
        low_vector_post_repair_first_exit_index: low_vector_post_repair.first_exit.index,
        low_vector_post_repair_first_exit_reason: low_vector_post_repair.first_exit.reason,
        low_vector_post_repair_first_exit_diagnosis: low_vector_post_repair.first_exit.diagnosis,
        low_vector_post_repair_first_exit_pc: low_vector_post_repair.first_exit.pc,
        low_vector_post_repair_first_interaction_kind: low_vector_post_repair
            .first_exit
            .interaction_kind,
        low_vector_post_repair_first_exit_access_kind: low_vector_post_repair
            .first_exit
            .access
            .kind,
        low_vector_post_repair_first_exit_access_direction: low_vector_post_repair
            .first_exit
            .access
            .direction,
        low_vector_post_repair_first_exit_access_address: low_vector_post_repair
            .first_exit
            .access
            .address,
        low_vector_post_repair_first_exit_access_sysreg: low_vector_post_repair
            .first_exit
            .access
            .sysreg,
        low_vector_post_repair_first_exit_access_syndrome: low_vector_post_repair
            .first_exit
            .access
            .syndrome,
        low_vector_post_repair_first_device_interaction_observed: low_vector_post_repair
            .first_device_interaction
            .observed,
        low_vector_post_repair_first_device_interaction_index: low_vector_post_repair
            .first_device_interaction
            .index,
        low_vector_post_repair_first_device_interaction_reason: low_vector_post_repair
            .first_device_interaction
            .reason,
        low_vector_post_repair_first_device_interaction_diagnosis: low_vector_post_repair
            .first_device_interaction
            .diagnosis,
        low_vector_post_repair_first_device_interaction_pc: low_vector_post_repair
            .first_device_interaction
            .pc,
        low_vector_post_repair_first_device_interaction_kind: low_vector_post_repair
            .first_device_interaction
            .interaction_kind,
        low_vector_post_repair_first_device_interaction_access_kind: low_vector_post_repair
            .first_device_interaction
            .access
            .kind,
        low_vector_post_repair_first_device_interaction_access_direction: low_vector_post_repair
            .first_device_interaction
            .access
            .direction,
        low_vector_post_repair_first_device_interaction_access_address: low_vector_post_repair
            .first_device_interaction
            .access
            .address,
        low_vector_post_repair_first_device_interaction_access_sysreg: low_vector_post_repair
            .first_device_interaction
            .access
            .sysreg,
        low_vector_post_repair_first_device_interaction_access_syndrome: low_vector_post_repair
            .first_device_interaction
            .access
            .syndrome,
        low_vector_post_repair_first_unhandled_access_observed: low_vector_post_repair
            .first_unhandled_access
            .observed,
        low_vector_post_repair_first_unhandled_access_index: low_vector_post_repair
            .first_unhandled_access
            .index,
        low_vector_post_repair_first_unhandled_access_reason: low_vector_post_repair
            .first_unhandled_access
            .reason,
        low_vector_post_repair_first_unhandled_access_diagnosis: low_vector_post_repair
            .first_unhandled_access
            .diagnosis,
        low_vector_post_repair_first_unhandled_access_pc: low_vector_post_repair
            .first_unhandled_access
            .pc,
        low_vector_post_repair_first_unhandled_access_syndrome: low_vector_post_repair
            .first_unhandled_access
            .syndrome,
        low_vector_post_repair_first_unhandled_access_kind: low_vector_post_repair
            .first_unhandled_access
            .kind,
        low_vector_post_repair_first_unhandled_access_direction: low_vector_post_repair
            .first_unhandled_access
            .access,
        low_vector_post_repair_first_unhandled_access_register: low_vector_post_repair
            .first_unhandled_access
            .register,
        low_vector_post_repair_first_unhandled_access_value: low_vector_post_repair
            .first_unhandled_access
            .value,
        low_vector_post_repair_first_unhandled_access_handler_result: low_vector_post_repair
            .first_unhandled_access
            .handler_result,
        low_vector_post_repair_first_unhandled_access_mmio_ipa: low_vector_post_repair
            .first_unhandled_access
            .mmio_ipa,
        low_vector_post_repair_first_unhandled_access_mmio_width: low_vector_post_repair
            .first_unhandled_access
            .mmio_width,
        low_vector_post_repair_first_unhandled_access_mmio_device_kind: low_vector_post_repair
            .first_unhandled_access
            .mmio_device_kind,
        low_vector_post_repair_first_unhandled_access_sysreg: low_vector_post_repair
            .first_unhandled_access
            .sysreg,
        low_vector_post_repair_first_unhandled_access_sysreg_name: low_vector_post_repair
            .first_unhandled_access
            .sysreg_name,
        low_vector_post_repair_first_unhandled_access_sysreg_op0: low_vector_post_repair
            .first_unhandled_access
            .sysreg_op0,
        low_vector_post_repair_first_unhandled_access_sysreg_op1: low_vector_post_repair
            .first_unhandled_access
            .sysreg_op1,
        low_vector_post_repair_first_unhandled_access_sysreg_crn: low_vector_post_repair
            .first_unhandled_access
            .sysreg_crn,
        low_vector_post_repair_first_unhandled_access_sysreg_crm: low_vector_post_repair
            .first_unhandled_access
            .sysreg_crm,
        low_vector_post_repair_first_unhandled_access_sysreg_op2: low_vector_post_repair
            .first_unhandled_access
            .sysreg_op2,
        low_vector_diagnostic_page_resume_attempted: low_vector_resume.attempted,
        low_vector_diagnostic_page_resume_armed: low_vector_resume.armed,
        low_vector_diagnostic_page_resume_original_pc: low_vector_resume.original_pc,
        low_vector_diagnostic_page_resume_original_elr_el1: low_vector_resume.original_elr_el1,
        low_vector_diagnostic_page_resume_original_esr_el1: low_vector_resume.original_esr_el1,
        low_vector_diagnostic_page_resume_original_far_el1: low_vector_resume.original_far_el1,
        low_vector_diagnostic_page_resume_original_spsr_el1: low_vector_resume.original_spsr_el1,
        low_vector_diagnostic_page_original_slot_bytes: low_vector_resume.original_slot_bytes,
        low_vector_diagnostic_page_resume_target_instruction_before_eret: low_vector_resume
            .target_instruction_word_before_eret,
        low_vector_diagnostic_page_resume_target_stage1_leaf_descriptor_before_eret:
            low_vector_resume.target_stage1_leaf_descriptor_before_eret,
        low_vector_diagnostic_page_resume_target_stage1_leaf_kind_before_eret: low_vector_resume
            .target_stage1_leaf_kind_before_eret,
        low_vector_diagnostic_page_resume_target_is_installed_diagnostic_hvc_before_eret:
            low_vector_resume.target_is_installed_diagnostic_hvc_before_eret,
        low_vector_diagnostic_page_resume_elr_el1_set_status: low_vector_resume.elr_el1_set_status,
        low_vector_diagnostic_page_resume_spsr_el1_set_status: low_vector_resume
            .spsr_el1_set_status,
        low_vector_diagnostic_page_resume_cpsr_set_status: low_vector_resume.cpsr_set_status,
        low_vector_diagnostic_page_resume_pc_set_status: low_vector_resume.pc_set_status,
        vcpu_created,
        pc_set,
        x0_dtb_ipa_set,
        cpsr_set,
        sp_el1_set,
        diagnostic_vector_vbar_el1_set,
        recommended_vector_base_vbar_requested: try_recommended_vector_base_vbar,
        recommended_vector_base_vbar_attempted,
        recommended_vector_base_vbar_set,
        recommended_vector_base_vbar_diagnostic_vector_populated,
        recommended_vector_base_vbar_resume_requested: continue_after_recommended_vector_base_vbar,
        recommended_vector_base_vbar_resume_attempted,
        recommended_vector_base_vbar_resume_armed,
        interrupt_timer_wiring_requested: wire_interrupt_timer,
        interrupt_timer_initialized,
        run_loop_attempted,
        firmware_progress_observed,
        unsupported_exit_observed,
        watchdog_cancel_fired,
        vcpu_destroyed,
        firmware_memory_unmapped,
        vars_memory_unmapped,
        guest_ram_memory_unmapped,
        firmware_memory_deallocated,
        vars_memory_deallocated,
        guest_ram_memory_deallocated,
        vm_destroyed,
        host,
        pflash_map_verified: pflash_map.pflash_map_verified,
        reset_vector_ipa: WINDOWS_ARM_UEFI_CODE_IPA,
        firmware_slot_ipa: WINDOWS_ARM_UEFI_CODE_IPA,
        vars_slot_ipa: WINDOWS_ARM_UEFI_VARS_IPA,
        low_firmware_alias_ipa: WINDOWS_ARM_UEFI_LOW_CODE_ALIAS_IPA,
        low_vars_alias_ipa: WINDOWS_ARM_UEFI_LOW_VARS_ALIAS_IPA,
        guest_ram_ipa: WINDOWS_ARM_GUEST_RAM_IPA,
        platform_dtb_ipa: WINDOWS_ARM_PLATFORM_DTB_IPA,
        platform_dtb_guest_ram_offset: WINDOWS_ARM_PLATFORM_DTB_GUEST_RAM_OFFSET,
        sp_el1_seed_ipa,
        diagnostic_vector_location,
        diagnostic_vector_ipa,
        diagnostic_vector_bytes: WINDOWS_ARM_DIAGNOSTIC_VECTOR_BYTES,
        recommended_vector_base_vbar_source_exit_index,
        recommended_vector_base_vbar_target,
        recommended_vector_base_vbar_target_physical_address,
        recommended_vector_base_vbar_reason,
        recommended_vector_base_vbar_current_el_spx_sync_instruction_word,
        recommended_vector_base_vbar_current_el_spx_sync_instruction_hint,
        recommended_vector_base_vbar_followup_exit_observed,
        recommended_vector_base_vbar_followup_exit_index,
        recommended_vector_base_vbar_followup_exit_reason,
        recommended_vector_base_vbar_followup_exit_diagnosis,
        recommended_vector_base_vbar_followup_pc,
        recommended_vector_base_vbar_followup_vbar_el1,
        recommended_vector_base_vbar_followup_target_still_set,
        recommended_vector_base_vbar_resume_original_pc,
        recommended_vector_base_vbar_resume_original_elr_el1,
        recommended_vector_base_vbar_resume_original_esr_el1,
        recommended_vector_base_vbar_resume_original_far_el1,
        recommended_vector_base_vbar_resume_original_spsr_el1,
        slot_bytes: WINDOWS_ARM_UEFI_SLOT_BYTES,
        guest_ram_bytes,
        platform_dtb_bytes,
        platform_dtb_magic,
        platform_dtb_magic_verified,
        requested_exits: bounded_requested_exits,
        observed_exits: exits.len() as u32,
        watchdog_timeout_ms: bounded_watchdog_timeout_ms,
        vtimer_offset_value: wire_interrupt_timer.then_some(WINDOWS_ARM_VTIMER_OFFSET_VALUE),
        cntv_cval_value: wire_interrupt_timer.then_some(cntv_cval_value),
        cntv_ctl_value: wire_interrupt_timer.then_some(cntv_ctl_value),
        vtimer_exit_count,
        pending_irq_injected_count,
        device_irq_injected_count,
        device_irq_cleared_count,
        handled_mmio_read_count,
        handled_mmio_write_count,
        handled_pl011_mmio_count,
        handled_pl031_mmio_count,
        handled_gicd_mmio_count,
        handled_gicr_mmio_count,
        handled_virtio_installer_iso_mmio_count,
        handled_virtio_target_disk_mmio_count,
        virtio_queue_notify_count,
        virtio_request_completion_count,
        handled_icc_read_count,
        handled_icc_write_count,
        handled_icc_iar1_read_count,
        handled_icc_eoir1_write_count,
        handled_icc_dir_write_count,
        last_icc_iar1_intid,
        last_icc_eoir1_intid,
        last_icc_dir_intid,
        firmware_source_bytes,
        vars_source_bytes,
        installer_iso_path,
        writable_target_disk_path,
        block_devices,
        firmware_map_flags: "read|exec",
        vars_map_flags: "read|write",
        low_firmware_alias_map_flags: "read|exec",
        low_vars_alias_map_flags: "read|write",
        guest_ram_map_flags: "read|write|exec",
        low_pflash_alias_requested: map_low_pflash_alias,
        vm_create_status: Some(vm_create_status),
        firmware_allocate_status,
        vars_allocate_status,
        guest_ram_allocate_status,
        firmware_map_status,
        vars_map_status,
        low_firmware_alias_map_status,
        low_vars_alias_map_status,
        guest_ram_map_status,
        vcpu_create_status,
        pc_set_status,
        x0_dtb_ipa_set_status,
        cpsr_set_status,
        sp_el1_set_status,
        diagnostic_vector_vbar_el1_set_status,
        recommended_vector_base_vbar_set_status,
        recommended_vector_base_vbar_resume_vbar_el1_set_status,
        recommended_vector_base_vbar_resume_elr_el1_set_status,
        recommended_vector_base_vbar_resume_spsr_el1_set_status,
        recommended_vector_base_vbar_resume_pc_set_status,
        vtimer_offset_set_status,
        cntv_cval_set_status,
        cntv_ctl_set_status,
        vtimer_initial_unmask_status,
        last_pending_irq_set_status,
        last_device_irq_set_status,
        last_device_irq_clear_status,
        last_vtimer_unmask_status,
        final_pc_status,
        final_pc,
        vcpu_destroy_status,
        firmware_unmap_status,
        vars_unmap_status,
        low_firmware_alias_unmap_status,
        low_vars_alias_unmap_status,
        guest_ram_unmap_status,
        firmware_deallocate_status,
        vars_deallocate_status,
        guest_ram_deallocate_status,
        vm_destroy_status,
        exits,
        blockers,
    }
}

struct FirmwareRunLoopProbeResultInput<'a> {
    allowed: bool,
    attempted: bool,
    host: HvfHostCapabilities,
    pflash_map_verified: bool,
    guest_ram_bytes: u64,
    requested_exits: u32,
    watchdog_timeout_ms: u64,
    options: &'a WindowsArmUefiFirmwareRunLoopExecutionOptions,
    firmware_source_bytes: Option<u64>,
    vars_source_bytes: Option<u64>,
    blockers: Vec<String>,
}

fn firmware_run_loop_probe_result(
    input: FirmwareRunLoopProbeResultInput<'_>,
) -> WindowsArmUefiFirmwareRunLoopProbe {
    let FirmwareRunLoopProbeResultInput {
        allowed,
        attempted,
        host,
        pflash_map_verified,
        guest_ram_bytes,
        requested_exits,
        watchdog_timeout_ms,
        options,
        firmware_source_bytes,
        vars_source_bytes,
        blockers,
    } = input;
    let map_low_pflash_alias = options.map_low_pflash_alias;
    let seed_diagnostic_vector = options.seed_diagnostic_vector;
    let seed_guest_ram_diagnostic_vector = options.seed_guest_ram_diagnostic_vector;
    let seed_executable_diagnostic_vector = options.seed_executable_diagnostic_vector;
    let try_recommended_vector_base_vbar = options.try_recommended_vector_base_vbar;
    let continue_after_recommended_vector_base_vbar =
        options.continue_after_recommended_vector_base_vbar;
    let repair_low_vector_diagnostic_page = options.repair_low_vector_diagnostic_page;
    let continue_after_low_vector_repair = options.continue_after_low_vector_repair;
    let wire_interrupt_timer = options.wire_interrupt_timer;
    let installer_iso_path = options.installer_iso_path.clone();
    let writable_target_disk_path = options.writable_target_disk_path.clone();
    let diagnostic_vector = windows_arm_diagnostic_vector_selection(
        seed_diagnostic_vector,
        seed_guest_ram_diagnostic_vector,
        seed_executable_diagnostic_vector,
    );
    let diagnostic_vector_seed_requested = diagnostic_vector.requested;
    let diagnostic_vector_location = diagnostic_vector.location;
    let diagnostic_vector_ipa = diagnostic_vector.ipa;
    let block_devices = windows_arm_firmware_block_devices(
        installer_iso_path.clone(),
        writable_target_disk_path.clone(),
    );
    let (platform_dtb_bytes, platform_dtb_magic, platform_dtb_magic_verified) =
        windows_arm_firmware_run_loop_dtb_metadata(guest_ram_bytes);
    let recommended_vector_base_vbar_reason = recommended_vector_base_vbar_initial_reason(
        try_recommended_vector_base_vbar,
        diagnostic_vector_seed_requested,
        repair_low_vector_diagnostic_page,
    );
    let low_vector_post_repair = LowVectorPostRepairTelemetry::default();
    WindowsArmUefiFirmwareRunLoopProbe {
        allowed,
        attempted,
        vm_created: false,
        firmware_memory_allocated: false,
        vars_memory_allocated: false,
        guest_ram_memory_allocated: false,
        firmware_memory_populated: false,
        vars_memory_populated: false,
        firmware_memory_mapped: false,
        vars_memory_mapped: false,
        low_firmware_alias_mapped: false,
        low_vars_alias_mapped: false,
        guest_ram_memory_mapped: false,
        platform_dtb_populated: false,
        diagnostic_vector_seed_requested,
        diagnostic_vector_populated: false,
        low_vector_diagnostic_page_repair_requested: repair_low_vector_diagnostic_page,
        low_vector_diagnostic_page_repaired: false,
        low_vector_diagnostic_page_slot_restored: false,
        low_vector_diagnostic_page_restore_before_eret_requested: options
            .restore_low_vector_slot_before_eret,
        low_vector_diagnostic_page_restore_before_eret_attempted: false,
        low_vector_diagnostic_page_entry_ipa: None,
        low_vector_diagnostic_page_previous_descriptor: None,
        low_vector_diagnostic_page_descriptor: None,
        low_vector_diagnostic_page_repeated_fault_observed: false,
        low_vector_recommended_vector_remap_requested: options
            .remap_low_vector_to_recommended_vector,
        low_vector_recommended_vector_remap_attempted: false,
        low_vector_recommended_vector_remap_succeeded: false,
        low_vector_recommended_vector_remap_target_physical_address: None,
        low_vector_recommended_vector_remap_descriptor: None,
        low_vector_post_repair_continue_requested: continue_after_low_vector_repair,
        low_vector_post_repair_continue_attempted: low_vector_post_repair.continue_attempted,
        stop_at_first_post_repair_device_boundary_requested: options
            .stop_at_first_post_repair_device_boundary,
        low_vector_post_repair_unsupported_exit_observed: low_vector_post_repair
            .unsupported_exit_observed,
        low_vector_post_repair_unsupported_exit_reason: low_vector_post_repair
            .unsupported_exit_reason,
        low_vector_post_repair_unsupported_exit_diagnosis: low_vector_post_repair
            .unsupported_exit_diagnosis,
        low_vector_post_repair_first_exit_observed: low_vector_post_repair.first_exit.observed,
        low_vector_post_repair_first_exit_index: low_vector_post_repair.first_exit.index,
        low_vector_post_repair_first_exit_reason: low_vector_post_repair.first_exit.reason,
        low_vector_post_repair_first_exit_diagnosis: low_vector_post_repair.first_exit.diagnosis,
        low_vector_post_repair_first_exit_pc: low_vector_post_repair.first_exit.pc,
        low_vector_post_repair_first_interaction_kind: low_vector_post_repair
            .first_exit
            .interaction_kind,
        low_vector_post_repair_first_exit_access_kind: low_vector_post_repair
            .first_exit
            .access
            .kind,
        low_vector_post_repair_first_exit_access_direction: low_vector_post_repair
            .first_exit
            .access
            .direction,
        low_vector_post_repair_first_exit_access_address: low_vector_post_repair
            .first_exit
            .access
            .address,
        low_vector_post_repair_first_exit_access_sysreg: low_vector_post_repair
            .first_exit
            .access
            .sysreg,
        low_vector_post_repair_first_exit_access_syndrome: low_vector_post_repair
            .first_exit
            .access
            .syndrome,
        low_vector_post_repair_first_device_interaction_observed: low_vector_post_repair
            .first_device_interaction
            .observed,
        low_vector_post_repair_first_device_interaction_index: low_vector_post_repair
            .first_device_interaction
            .index,
        low_vector_post_repair_first_device_interaction_reason: low_vector_post_repair
            .first_device_interaction
            .reason,
        low_vector_post_repair_first_device_interaction_diagnosis: low_vector_post_repair
            .first_device_interaction
            .diagnosis,
        low_vector_post_repair_first_device_interaction_pc: low_vector_post_repair
            .first_device_interaction
            .pc,
        low_vector_post_repair_first_device_interaction_kind: low_vector_post_repair
            .first_device_interaction
            .interaction_kind,
        low_vector_post_repair_first_device_interaction_access_kind: low_vector_post_repair
            .first_device_interaction
            .access
            .kind,
        low_vector_post_repair_first_device_interaction_access_direction: low_vector_post_repair
            .first_device_interaction
            .access
            .direction,
        low_vector_post_repair_first_device_interaction_access_address: low_vector_post_repair
            .first_device_interaction
            .access
            .address,
        low_vector_post_repair_first_device_interaction_access_sysreg: low_vector_post_repair
            .first_device_interaction
            .access
            .sysreg,
        low_vector_post_repair_first_device_interaction_access_syndrome: low_vector_post_repair
            .first_device_interaction
            .access
            .syndrome,
        low_vector_post_repair_first_unhandled_access_observed: low_vector_post_repair
            .first_unhandled_access
            .observed,
        low_vector_post_repair_first_unhandled_access_index: low_vector_post_repair
            .first_unhandled_access
            .index,
        low_vector_post_repair_first_unhandled_access_reason: low_vector_post_repair
            .first_unhandled_access
            .reason,
        low_vector_post_repair_first_unhandled_access_diagnosis: low_vector_post_repair
            .first_unhandled_access
            .diagnosis,
        low_vector_post_repair_first_unhandled_access_pc: low_vector_post_repair
            .first_unhandled_access
            .pc,
        low_vector_post_repair_first_unhandled_access_syndrome: low_vector_post_repair
            .first_unhandled_access
            .syndrome,
        low_vector_post_repair_first_unhandled_access_kind: low_vector_post_repair
            .first_unhandled_access
            .kind,
        low_vector_post_repair_first_unhandled_access_direction: low_vector_post_repair
            .first_unhandled_access
            .access,
        low_vector_post_repair_first_unhandled_access_register: low_vector_post_repair
            .first_unhandled_access
            .register,
        low_vector_post_repair_first_unhandled_access_value: low_vector_post_repair
            .first_unhandled_access
            .value,
        low_vector_post_repair_first_unhandled_access_handler_result: low_vector_post_repair
            .first_unhandled_access
            .handler_result,
        low_vector_post_repair_first_unhandled_access_mmio_ipa: low_vector_post_repair
            .first_unhandled_access
            .mmio_ipa,
        low_vector_post_repair_first_unhandled_access_mmio_width: low_vector_post_repair
            .first_unhandled_access
            .mmio_width,
        low_vector_post_repair_first_unhandled_access_mmio_device_kind: low_vector_post_repair
            .first_unhandled_access
            .mmio_device_kind,
        low_vector_post_repair_first_unhandled_access_sysreg: low_vector_post_repair
            .first_unhandled_access
            .sysreg,
        low_vector_post_repair_first_unhandled_access_sysreg_name: low_vector_post_repair
            .first_unhandled_access
            .sysreg_name,
        low_vector_post_repair_first_unhandled_access_sysreg_op0: low_vector_post_repair
            .first_unhandled_access
            .sysreg_op0,
        low_vector_post_repair_first_unhandled_access_sysreg_op1: low_vector_post_repair
            .first_unhandled_access
            .sysreg_op1,
        low_vector_post_repair_first_unhandled_access_sysreg_crn: low_vector_post_repair
            .first_unhandled_access
            .sysreg_crn,
        low_vector_post_repair_first_unhandled_access_sysreg_crm: low_vector_post_repair
            .first_unhandled_access
            .sysreg_crm,
        low_vector_post_repair_first_unhandled_access_sysreg_op2: low_vector_post_repair
            .first_unhandled_access
            .sysreg_op2,
        low_vector_diagnostic_page_resume_attempted: false,
        low_vector_diagnostic_page_resume_armed: false,
        low_vector_diagnostic_page_resume_original_pc: None,
        low_vector_diagnostic_page_resume_original_elr_el1: None,
        low_vector_diagnostic_page_resume_original_esr_el1: None,
        low_vector_diagnostic_page_resume_original_far_el1: None,
        low_vector_diagnostic_page_resume_original_spsr_el1: None,
        low_vector_diagnostic_page_original_slot_bytes: None,
        low_vector_diagnostic_page_resume_target_instruction_before_eret: None,
        low_vector_diagnostic_page_resume_target_stage1_leaf_descriptor_before_eret: None,
        low_vector_diagnostic_page_resume_target_stage1_leaf_kind_before_eret: "not observed",
        low_vector_diagnostic_page_resume_target_is_installed_diagnostic_hvc_before_eret: false,
        low_vector_diagnostic_page_resume_elr_el1_set_status: None,
        low_vector_diagnostic_page_resume_spsr_el1_set_status: None,
        low_vector_diagnostic_page_resume_cpsr_set_status: None,
        low_vector_diagnostic_page_resume_pc_set_status: None,
        vcpu_created: false,
        pc_set: false,
        x0_dtb_ipa_set: false,
        cpsr_set: false,
        sp_el1_set: false,
        diagnostic_vector_vbar_el1_set: false,
        recommended_vector_base_vbar_requested: try_recommended_vector_base_vbar,
        recommended_vector_base_vbar_attempted: false,
        recommended_vector_base_vbar_set: false,
        recommended_vector_base_vbar_diagnostic_vector_populated: false,
        recommended_vector_base_vbar_resume_requested: continue_after_recommended_vector_base_vbar,
        recommended_vector_base_vbar_resume_attempted: false,
        recommended_vector_base_vbar_resume_armed: false,
        interrupt_timer_wiring_requested: wire_interrupt_timer,
        interrupt_timer_initialized: false,
        run_loop_attempted: false,
        firmware_progress_observed: false,
        unsupported_exit_observed: false,
        watchdog_cancel_fired: false,
        vcpu_destroyed: false,
        firmware_memory_unmapped: false,
        vars_memory_unmapped: false,
        guest_ram_memory_unmapped: false,
        firmware_memory_deallocated: false,
        vars_memory_deallocated: false,
        guest_ram_memory_deallocated: false,
        vm_destroyed: false,
        host,
        pflash_map_verified,
        reset_vector_ipa: WINDOWS_ARM_UEFI_CODE_IPA,
        firmware_slot_ipa: WINDOWS_ARM_UEFI_CODE_IPA,
        vars_slot_ipa: WINDOWS_ARM_UEFI_VARS_IPA,
        low_firmware_alias_ipa: WINDOWS_ARM_UEFI_LOW_CODE_ALIAS_IPA,
        low_vars_alias_ipa: WINDOWS_ARM_UEFI_LOW_VARS_ALIAS_IPA,
        guest_ram_ipa: WINDOWS_ARM_GUEST_RAM_IPA,
        platform_dtb_ipa: WINDOWS_ARM_PLATFORM_DTB_IPA,
        platform_dtb_guest_ram_offset: WINDOWS_ARM_PLATFORM_DTB_GUEST_RAM_OFFSET,
        sp_el1_seed_ipa: windows_arm_initial_sp_el1_ipa(guest_ram_bytes),
        diagnostic_vector_location,
        diagnostic_vector_ipa,
        diagnostic_vector_bytes: WINDOWS_ARM_DIAGNOSTIC_VECTOR_BYTES,
        recommended_vector_base_vbar_source_exit_index: None,
        recommended_vector_base_vbar_target: None,
        recommended_vector_base_vbar_target_physical_address: None,
        recommended_vector_base_vbar_reason,
        recommended_vector_base_vbar_current_el_spx_sync_instruction_word: None,
        recommended_vector_base_vbar_current_el_spx_sync_instruction_hint: "not observed",
        recommended_vector_base_vbar_followup_exit_observed: false,
        recommended_vector_base_vbar_followup_exit_index: None,
        recommended_vector_base_vbar_followup_exit_reason: None,
        recommended_vector_base_vbar_followup_exit_diagnosis: "not observed",
        recommended_vector_base_vbar_followup_pc: None,
        recommended_vector_base_vbar_followup_vbar_el1: None,
        recommended_vector_base_vbar_followup_target_still_set: false,
        recommended_vector_base_vbar_resume_original_pc: None,
        recommended_vector_base_vbar_resume_original_elr_el1: None,
        recommended_vector_base_vbar_resume_original_esr_el1: None,
        recommended_vector_base_vbar_resume_original_far_el1: None,
        recommended_vector_base_vbar_resume_original_spsr_el1: None,
        slot_bytes: WINDOWS_ARM_UEFI_SLOT_BYTES,
        guest_ram_bytes,
        platform_dtb_bytes,
        platform_dtb_magic,
        platform_dtb_magic_verified,
        requested_exits,
        observed_exits: 0,
        watchdog_timeout_ms,
        vtimer_offset_value: wire_interrupt_timer.then_some(WINDOWS_ARM_VTIMER_OFFSET_VALUE),
        cntv_cval_value: wire_interrupt_timer.then_some(0),
        cntv_ctl_value: wire_interrupt_timer.then_some(1),
        vtimer_exit_count: 0,
        pending_irq_injected_count: 0,
        device_irq_injected_count: 0,
        device_irq_cleared_count: 0,
        handled_mmio_read_count: 0,
        handled_mmio_write_count: 0,
        handled_pl011_mmio_count: 0,
        handled_pl031_mmio_count: 0,
        handled_gicd_mmio_count: 0,
        handled_gicr_mmio_count: 0,
        handled_virtio_installer_iso_mmio_count: 0,
        handled_virtio_target_disk_mmio_count: 0,
        virtio_queue_notify_count: 0,
        virtio_request_completion_count: 0,
        handled_icc_read_count: 0,
        handled_icc_write_count: 0,
        handled_icc_iar1_read_count: 0,
        handled_icc_eoir1_write_count: 0,
        handled_icc_dir_write_count: 0,
        last_icc_iar1_intid: None,
        last_icc_eoir1_intid: None,
        last_icc_dir_intid: None,
        firmware_source_bytes,
        vars_source_bytes,
        installer_iso_path,
        writable_target_disk_path,
        block_devices,
        firmware_map_flags: "read|exec",
        vars_map_flags: "read|write",
        low_firmware_alias_map_flags: "read|exec",
        low_vars_alias_map_flags: "read|write",
        guest_ram_map_flags: "read|write|exec",
        low_pflash_alias_requested: map_low_pflash_alias,
        vm_create_status: None,
        firmware_allocate_status: None,
        vars_allocate_status: None,
        guest_ram_allocate_status: None,
        firmware_map_status: None,
        vars_map_status: None,
        low_firmware_alias_map_status: None,
        low_vars_alias_map_status: None,
        guest_ram_map_status: None,
        vcpu_create_status: None,
        pc_set_status: None,
        x0_dtb_ipa_set_status: None,
        cpsr_set_status: None,
        sp_el1_set_status: None,
        diagnostic_vector_vbar_el1_set_status: None,
        recommended_vector_base_vbar_set_status: None,
        recommended_vector_base_vbar_resume_vbar_el1_set_status: None,
        recommended_vector_base_vbar_resume_elr_el1_set_status: None,
        recommended_vector_base_vbar_resume_spsr_el1_set_status: None,
        recommended_vector_base_vbar_resume_pc_set_status: None,
        vtimer_offset_set_status: None,
        cntv_cval_set_status: None,
        cntv_ctl_set_status: None,
        vtimer_initial_unmask_status: None,
        last_pending_irq_set_status: None,
        last_device_irq_set_status: None,
        last_device_irq_clear_status: None,
        last_vtimer_unmask_status: None,
        final_pc_status: None,
        final_pc: None,
        vcpu_destroy_status: None,
        firmware_unmap_status: None,
        vars_unmap_status: None,
        low_firmware_alias_unmap_status: None,
        low_vars_alias_unmap_status: None,
        guest_ram_unmap_status: None,
        firmware_deallocate_status: None,
        vars_deallocate_status: None,
        guest_ram_deallocate_status: None,
        vm_destroy_status: None,
        exits: Vec::new(),
        blockers,
    }
}

pub fn probe_hvf_guest_entry(allow_entry: bool, host: HvfHostCapabilities) -> HvfGuestEntryProbe {
    let mut blockers = Vec::new();

    if !allow_entry {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_GUEST_ENTRY=1 or pass --allow-entry to map one HVC instruction, set PC/CPSR, and run with a watchdog".to_string(),
        );
        return guest_entry_probe_result(false, false, host, blockers);
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return guest_entry_probe_result(true, false, host, blockers);
    }

    let mut memory_allocated = false;
    let mut memory_mapped = false;
    let mut vcpu_created = false;
    let mut pc_set = false;
    let mut cpsr_set = false;
    let mut run_attempted = false;
    let mut entry_boundary_observed = false;
    let mut watchdog_cancel_fired = false;
    let mut vcpu_destroyed = false;
    let mut memory_unmapped = false;
    let mut vm_destroyed = false;
    let mut memory_deallocated = false;

    let mut allocate_status = None;
    let mut map_status = None;
    let mut vcpu_create_status = None;
    let mut pc_set_status = None;
    let mut cpsr_set_status = None;
    let mut run_status = None;
    let mut exit_reason = None;
    let mut exit_syndrome = None;
    let mut exit_virtual_address = None;
    let mut exit_physical_address = None;
    let mut watchdog_cancel_status = None;
    let mut vcpu_destroy_status = None;
    let mut unmap_status = None;
    let mut vm_destroy_status = None;
    let mut deallocate_status = None;

    let mut memory = ptr::null_mut();
    let mut vcpu = 0;
    let mut exit = ptr::null_mut();

    let status = unsafe { hv_vm_create(ptr::null_mut()) };
    let vm_create_status = Some(status);
    let vm_created = status == HV_SUCCESS;
    if !vm_created {
        blockers.push(format!("hv_vm_create failed: {status:#x}"));
    }

    if vm_created {
        let status = unsafe { hv_vm_allocate(&mut memory, PROBE_BYTES, HV_ALLOCATE_DEFAULT) };
        allocate_status = Some(status);
        memory_allocated = status == HV_SUCCESS && !memory.is_null();
        if memory_allocated {
            let instruction = AARCH64_HVC_0.to_le_bytes();
            unsafe {
                ptr::copy_nonoverlapping(
                    instruction.as_ptr(),
                    memory.cast::<u8>(),
                    instruction.len(),
                );
            }
        } else {
            blockers.push(format!("hv_vm_allocate failed: {status:#x}"));
        }
    }

    if vm_created && memory_allocated {
        let status = unsafe {
            hv_vm_map(
                memory,
                PROBE_IPA_START,
                PROBE_BYTES,
                HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC,
            )
        };
        map_status = Some(status);
        memory_mapped = status == HV_SUCCESS;
        if !memory_mapped {
            blockers.push(format!("hv_vm_map failed: {status:#x}"));
        }
    }

    if vm_created && memory_mapped {
        let status = unsafe { hv_vcpu_create(&mut vcpu, &mut exit, ptr::null_mut()) };
        vcpu_create_status = Some(status);
        vcpu_created = status == HV_SUCCESS;
        if !vcpu_created {
            blockers.push(format!("hv_vcpu_create failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, PROBE_IPA_START) };
        pc_set_status = Some(status);
        pc_set = status == HV_SUCCESS;
        if !pc_set {
            blockers.push(format!("hv_vcpu_set_reg(PC) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_CPSR, AARCH64_PSTATE_EL1H_DAIF_MASKED) };
        cpsr_set_status = Some(status);
        cpsr_set = status == HV_SUCCESS;
        if !cpsr_set {
            blockers.push(format!("hv_vcpu_set_reg(CPSR) failed: {status:#x}"));
        }
    }

    if vcpu_created && pc_set && cpsr_set {
        run_attempted = true;
        let done = Arc::new(AtomicBool::new(false));
        let watchdog_done = Arc::clone(&done);
        let vcpu_for_watchdog = vcpu;
        let watchdog = thread::spawn(move || {
            for _ in 0..100 {
                if watchdog_done.load(Ordering::SeqCst) {
                    return None;
                }
                thread::sleep(Duration::from_millis(1));
            }
            let mut vcpu = vcpu_for_watchdog;
            Some(unsafe { hv_vcpus_exit(&mut vcpu, 1) })
        });

        let status = unsafe { hv_vcpu_run(vcpu) };
        run_status = Some(status);
        done.store(true, Ordering::SeqCst);
        watchdog_cancel_status = watchdog.join().ok().flatten();
        watchdog_cancel_fired = watchdog_cancel_status.is_some();

        if status == HV_SUCCESS {
            if exit.is_null() {
                blockers
                    .push("hv_vcpu_run returned success without an exit info pointer".to_string());
            } else {
                let exit_info = unsafe { &*exit };
                exit_reason = Some(exit_info.reason);
                exit_syndrome = Some(exit_info.exception.syndrome);
                exit_virtual_address = Some(exit_info.exception.virtual_address);
                exit_physical_address = Some(exit_info.exception.physical_address);
                entry_boundary_observed = exit_reason == Some(1);
                if !entry_boundary_observed {
                    blockers.push(format!(
                        "hv_vcpu_run returned non-exception exit reason: {}",
                        exit_reason.unwrap_or_default()
                    ));
                }
            }
        } else {
            blockers.push(format!("hv_vcpu_run failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_destroy(vcpu) };
        vcpu_destroy_status = Some(status);
        vcpu_destroyed = status == HV_SUCCESS;
        if !vcpu_destroyed {
            blockers.push(format!("hv_vcpu_destroy failed: {status:#x}"));
        }
    }

    if memory_mapped {
        let status = unsafe { hv_vm_unmap(PROBE_IPA_START, PROBE_BYTES) };
        unmap_status = Some(status);
        memory_unmapped = status == HV_SUCCESS;
        if !memory_unmapped {
            blockers.push(format!("hv_vm_unmap failed: {status:#x}"));
        }
    }

    if vm_created {
        let status = unsafe { hv_vm_destroy() };
        vm_destroy_status = Some(status);
        vm_destroyed = status == HV_SUCCESS;
        if !vm_destroyed {
            blockers.push(format!("hv_vm_destroy failed: {status:#x}"));
        }
    }

    if memory_allocated {
        let status = unsafe { hv_vm_deallocate(memory, PROBE_BYTES) };
        deallocate_status = Some(status);
        memory_deallocated = status == HV_SUCCESS;
        if !memory_deallocated {
            blockers.push(format!("hv_vm_deallocate failed: {status:#x}"));
        }
    }

    HvfGuestEntryProbe {
        allowed: true,
        attempted: true,
        vm_created,
        memory_allocated,
        memory_mapped,
        vcpu_created,
        pc_set,
        cpsr_set,
        run_attempted,
        entry_boundary_observed,
        watchdog_cancel_fired,
        vcpu_destroyed,
        memory_unmapped,
        vm_destroyed,
        memory_deallocated,
        host,
        ipa_start: PROBE_IPA_START,
        bytes: PROBE_BYTES,
        instruction: "HVC #0",
        vm_create_status,
        allocate_status,
        map_status,
        vcpu_create_status,
        pc_set_status,
        cpsr_set_status,
        run_status,
        exit_reason,
        exit_syndrome,
        exit_virtual_address,
        exit_physical_address,
        watchdog_cancel_status,
        vcpu_destroy_status,
        unmap_status,
        vm_destroy_status,
        deallocate_status,
        blockers,
    }
}

fn guest_entry_probe_result(
    allowed: bool,
    attempted: bool,
    host: HvfHostCapabilities,
    blockers: Vec<String>,
) -> HvfGuestEntryProbe {
    HvfGuestEntryProbe {
        allowed,
        attempted,
        vm_created: false,
        memory_allocated: false,
        memory_mapped: false,
        vcpu_created: false,
        pc_set: false,
        cpsr_set: false,
        run_attempted: false,
        entry_boundary_observed: false,
        watchdog_cancel_fired: false,
        vcpu_destroyed: false,
        memory_unmapped: false,
        vm_destroyed: false,
        memory_deallocated: false,
        host,
        ipa_start: PROBE_IPA_START,
        bytes: PROBE_BYTES,
        instruction: "HVC #0",
        vm_create_status: None,
        allocate_status: None,
        map_status: None,
        vcpu_create_status: None,
        pc_set_status: None,
        cpsr_set_status: None,
        run_status: None,
        exit_reason: None,
        exit_syndrome: None,
        exit_virtual_address: None,
        exit_physical_address: None,
        watchdog_cancel_status: None,
        vcpu_destroy_status: None,
        unmap_status: None,
        vm_destroy_status: None,
        deallocate_status: None,
        blockers,
    }
}

pub fn probe_hvf_guest_exit_loop(
    allow_loop: bool,
    host: HvfHostCapabilities,
) -> HvfGuestExitLoopProbe {
    let mut blockers = Vec::new();

    if !allow_loop {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_EXIT_LOOP=1 or pass --allow-loop to run two HVC exits with an explicit PC advance".to_string(),
        );
        return guest_exit_loop_probe_result(false, false, host, blockers);
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return guest_exit_loop_probe_result(true, false, host, blockers);
    }

    let mut memory_allocated = false;
    let mut memory_mapped = false;
    let mut vcpu_created = false;
    let mut initial_pc_set = false;
    let mut cpsr_set = false;
    let mut first_run_attempted = false;
    let mut first_exit_observed = false;
    let mut pc_read_after_first_exit = false;
    let mut pc_advanced = false;
    let mut second_run_attempted = false;
    let mut second_exit_observed = false;
    let mut vcpu_destroyed = false;
    let mut memory_unmapped = false;
    let mut vm_destroyed = false;
    let mut memory_deallocated = false;

    let mut allocate_status = None;
    let mut map_status = None;
    let mut vcpu_create_status = None;
    let mut initial_pc_set_status = None;
    let mut cpsr_set_status = None;
    let mut first_run_status = None;
    let mut first_exit_reason = None;
    let mut first_exit_syndrome = None;
    let mut first_exit_virtual_address = None;
    let mut first_exit_physical_address = None;
    let mut first_watchdog_cancel_status = None;
    let mut pc_read_status = None;
    let mut pc_after_first_exit = None;
    let mut pc_advance_status = None;
    let mut second_run_status = None;
    let mut second_exit_reason = None;
    let mut second_exit_syndrome = None;
    let mut second_exit_virtual_address = None;
    let mut second_exit_physical_address = None;
    let mut second_watchdog_cancel_status = None;
    let mut vcpu_destroy_status = None;
    let mut unmap_status = None;
    let mut vm_destroy_status = None;
    let mut deallocate_status = None;

    let mut memory = ptr::null_mut();
    let mut vcpu = 0;
    let mut exit = ptr::null_mut();

    let status = unsafe { hv_vm_create(ptr::null_mut()) };
    let vm_create_status = Some(status);
    let vm_created = status == HV_SUCCESS;
    if !vm_created {
        blockers.push(format!("hv_vm_create failed: {status:#x}"));
    }

    if vm_created {
        let status = unsafe { hv_vm_allocate(&mut memory, PROBE_BYTES, HV_ALLOCATE_DEFAULT) };
        allocate_status = Some(status);
        memory_allocated = status == HV_SUCCESS && !memory.is_null();
        if memory_allocated {
            let first = AARCH64_HVC_0.to_le_bytes();
            let second = AARCH64_HVC_1.to_le_bytes();
            unsafe {
                ptr::copy_nonoverlapping(first.as_ptr(), memory.cast::<u8>(), first.len());
                ptr::copy_nonoverlapping(
                    second.as_ptr(),
                    memory.cast::<u8>().add(first.len()),
                    second.len(),
                );
            }
        } else {
            blockers.push(format!("hv_vm_allocate failed: {status:#x}"));
        }
    }

    if vm_created && memory_allocated {
        let status = unsafe {
            hv_vm_map(
                memory,
                PROBE_IPA_START,
                PROBE_BYTES,
                HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC,
            )
        };
        map_status = Some(status);
        memory_mapped = status == HV_SUCCESS;
        if !memory_mapped {
            blockers.push(format!("hv_vm_map failed: {status:#x}"));
        }
    }

    if vm_created && memory_mapped {
        let status = unsafe { hv_vcpu_create(&mut vcpu, &mut exit, ptr::null_mut()) };
        vcpu_create_status = Some(status);
        vcpu_created = status == HV_SUCCESS;
        if !vcpu_created {
            blockers.push(format!("hv_vcpu_create failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, PROBE_IPA_START) };
        initial_pc_set_status = Some(status);
        initial_pc_set = status == HV_SUCCESS;
        if !initial_pc_set {
            blockers.push(format!("hv_vcpu_set_reg(PC) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_CPSR, AARCH64_PSTATE_EL1H_DAIF_MASKED) };
        cpsr_set_status = Some(status);
        cpsr_set = status == HV_SUCCESS;
        if !cpsr_set {
            blockers.push(format!("hv_vcpu_set_reg(CPSR) failed: {status:#x}"));
        }
    }

    if vcpu_created && initial_pc_set && cpsr_set {
        first_run_attempted = true;
        let first = run_vcpu_once_with_watchdog(vcpu, exit);
        first_run_status = Some(first.run_status);
        first_exit_reason = first.exit_reason;
        first_exit_syndrome = first.exit_syndrome;
        first_exit_virtual_address = first.exit_virtual_address;
        first_exit_physical_address = first.exit_physical_address;
        first_watchdog_cancel_status = first.watchdog_cancel_status;
        if first_watchdog_cancel_status.is_some() {
            blockers.push("first run watchdog fired before guest exception exit".to_string());
        }

        if first.run_status == HV_SUCCESS {
            if first_exit_reason.is_none() {
                blockers.push(
                    "first hv_vcpu_run returned success without an exit info pointer".to_string(),
                );
            } else {
                first_exit_observed = first_exit_reason == Some(HV_EXIT_REASON_EXCEPTION)
                    && first_exit_syndrome == Some(AARCH64_HVC_0_SYNDROME);
                if first_exit_reason != Some(HV_EXIT_REASON_EXCEPTION) {
                    blockers.push(format!(
                        "first hv_vcpu_run returned non-exception exit reason: {}",
                        first_exit_reason.unwrap_or_default()
                    ));
                }
                if first_exit_syndrome != Some(AARCH64_HVC_0_SYNDROME) {
                    blockers.push(format!(
                        "first hv_vcpu_run returned unexpected syndrome: {}",
                        first_exit_syndrome.map_or_else(
                            || "not observed".to_string(),
                            |value| { format!("{value:#x}") }
                        )
                    ));
                }
            }
        } else {
            blockers.push(format!("first hv_vcpu_run failed: {:#x}", first.run_status));
        }
    }

    if first_exit_observed {
        let mut pc = 0;
        let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc) };
        pc_read_status = Some(status);
        pc_read_after_first_exit = status == HV_SUCCESS;
        if pc_read_after_first_exit {
            pc_after_first_exit = Some(pc);
        } else {
            blockers.push(format!("hv_vcpu_get_reg(PC) failed: {status:#x}"));
        }
    }

    if pc_read_after_first_exit {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, PROBE_IPA_START + 4) };
        pc_advance_status = Some(status);
        pc_advanced = status == HV_SUCCESS;
        if !pc_advanced {
            blockers.push(format!("hv_vcpu_set_reg(PC + 4) failed: {status:#x}"));
        }
    }

    if pc_advanced {
        second_run_attempted = true;
        let second = run_vcpu_once_with_watchdog(vcpu, exit);
        second_run_status = Some(second.run_status);
        second_exit_reason = second.exit_reason;
        second_exit_syndrome = second.exit_syndrome;
        second_exit_virtual_address = second.exit_virtual_address;
        second_exit_physical_address = second.exit_physical_address;
        second_watchdog_cancel_status = second.watchdog_cancel_status;
        if second_watchdog_cancel_status.is_some() {
            blockers.push("second run watchdog fired before guest exception exit".to_string());
        }

        if second.run_status == HV_SUCCESS {
            if second_exit_reason.is_none() {
                blockers.push(
                    "second hv_vcpu_run returned success without an exit info pointer".to_string(),
                );
            } else {
                second_exit_observed = second_exit_reason == Some(HV_EXIT_REASON_EXCEPTION)
                    && second_exit_syndrome == Some(AARCH64_HVC_1_SYNDROME);
                if second_exit_reason != Some(HV_EXIT_REASON_EXCEPTION) {
                    blockers.push(format!(
                        "second hv_vcpu_run returned non-exception exit reason: {}",
                        second_exit_reason.unwrap_or_default()
                    ));
                }
                if second_exit_syndrome != Some(AARCH64_HVC_1_SYNDROME) {
                    blockers.push(format!(
                        "second hv_vcpu_run returned unexpected syndrome: {}",
                        second_exit_syndrome.map_or_else(
                            || "not observed".to_string(),
                            |value| { format!("{value:#x}") }
                        )
                    ));
                }
            }
        } else {
            blockers.push(format!(
                "second hv_vcpu_run failed: {:#x}",
                second.run_status
            ));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_destroy(vcpu) };
        vcpu_destroy_status = Some(status);
        vcpu_destroyed = status == HV_SUCCESS;
        if !vcpu_destroyed {
            blockers.push(format!("hv_vcpu_destroy failed: {status:#x}"));
        }
    }

    if memory_mapped {
        let status = unsafe { hv_vm_unmap(PROBE_IPA_START, PROBE_BYTES) };
        unmap_status = Some(status);
        memory_unmapped = status == HV_SUCCESS;
        if !memory_unmapped {
            blockers.push(format!("hv_vm_unmap failed: {status:#x}"));
        }
    }

    if vm_created {
        let status = unsafe { hv_vm_destroy() };
        vm_destroy_status = Some(status);
        vm_destroyed = status == HV_SUCCESS;
        if !vm_destroyed {
            blockers.push(format!("hv_vm_destroy failed: {status:#x}"));
        }
    }

    if memory_allocated {
        let status = unsafe { hv_vm_deallocate(memory, PROBE_BYTES) };
        deallocate_status = Some(status);
        memory_deallocated = status == HV_SUCCESS;
        if !memory_deallocated {
            blockers.push(format!("hv_vm_deallocate failed: {status:#x}"));
        }
    }

    let watchdog_cancel_fired =
        first_watchdog_cancel_status.is_some() || second_watchdog_cancel_status.is_some();
    let exit_loop_observed = first_exit_observed && pc_advanced && second_exit_observed;

    HvfGuestExitLoopProbe {
        allowed: true,
        attempted: true,
        vm_created,
        memory_allocated,
        memory_mapped,
        vcpu_created,
        initial_pc_set,
        cpsr_set,
        first_run_attempted,
        first_exit_observed,
        pc_read_after_first_exit,
        pc_advanced,
        second_run_attempted,
        second_exit_observed,
        exit_loop_observed,
        watchdog_cancel_fired,
        vcpu_destroyed,
        memory_unmapped,
        vm_destroyed,
        memory_deallocated,
        host,
        ipa_start: PROBE_IPA_START,
        bytes: PROBE_BYTES,
        instructions: "HVC #0; HVC #1",
        vm_create_status,
        allocate_status,
        map_status,
        vcpu_create_status,
        initial_pc_set_status,
        cpsr_set_status,
        first_run_status,
        first_exit_reason,
        first_exit_syndrome,
        first_exit_virtual_address,
        first_exit_physical_address,
        first_watchdog_cancel_status,
        pc_read_status,
        pc_after_first_exit,
        pc_advance_status,
        second_run_status,
        second_exit_reason,
        second_exit_syndrome,
        second_exit_virtual_address,
        second_exit_physical_address,
        second_watchdog_cancel_status,
        vcpu_destroy_status,
        unmap_status,
        vm_destroy_status,
        deallocate_status,
        blockers,
    }
}

fn guest_exit_loop_probe_result(
    allowed: bool,
    attempted: bool,
    host: HvfHostCapabilities,
    blockers: Vec<String>,
) -> HvfGuestExitLoopProbe {
    HvfGuestExitLoopProbe {
        allowed,
        attempted,
        vm_created: false,
        memory_allocated: false,
        memory_mapped: false,
        vcpu_created: false,
        initial_pc_set: false,
        cpsr_set: false,
        first_run_attempted: false,
        first_exit_observed: false,
        pc_read_after_first_exit: false,
        pc_advanced: false,
        second_run_attempted: false,
        second_exit_observed: false,
        exit_loop_observed: false,
        watchdog_cancel_fired: false,
        vcpu_destroyed: false,
        memory_unmapped: false,
        vm_destroyed: false,
        memory_deallocated: false,
        host,
        ipa_start: PROBE_IPA_START,
        bytes: PROBE_BYTES,
        instructions: "HVC #0; HVC #1",
        vm_create_status: None,
        allocate_status: None,
        map_status: None,
        vcpu_create_status: None,
        initial_pc_set_status: None,
        cpsr_set_status: None,
        first_run_status: None,
        first_exit_reason: None,
        first_exit_syndrome: None,
        first_exit_virtual_address: None,
        first_exit_physical_address: None,
        first_watchdog_cancel_status: None,
        pc_read_status: None,
        pc_after_first_exit: None,
        pc_advance_status: None,
        second_run_status: None,
        second_exit_reason: None,
        second_exit_syndrome: None,
        second_exit_virtual_address: None,
        second_exit_physical_address: None,
        second_watchdog_cancel_status: None,
        vcpu_destroy_status: None,
        unmap_status: None,
        vm_destroy_status: None,
        deallocate_status: None,
        blockers,
    }
}

pub fn probe_hvf_mmio_read_exit(
    allow_mmio: bool,
    host: HvfHostCapabilities,
) -> HvfMmioReadExitProbe {
    let mut blockers = Vec::new();

    if !allow_mmio {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_MMIO_READ=1 or pass --allow-mmio to run one unmapped LDR read and observe the MMIO/data-abort exit".to_string(),
        );
        return mmio_read_exit_probe_result(false, false, host, blockers);
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return mmio_read_exit_probe_result(true, false, host, blockers);
    }

    let mut memory_allocated = false;
    let mut memory_mapped = false;
    let mut vcpu_created = false;
    let mut pc_set = false;
    let mut cpsr_set = false;
    let mut address_register_set = false;
    let mut run_attempted = false;
    let mut mmio_exit_observed = false;
    let mut vcpu_destroyed = false;
    let mut memory_unmapped = false;
    let mut vm_destroyed = false;
    let mut memory_deallocated = false;

    let mut allocate_status = None;
    let mut map_status = None;
    let mut vcpu_create_status = None;
    let mut pc_set_status = None;
    let mut cpsr_set_status = None;
    let mut address_register_set_status = None;
    let mut run_status = None;
    let mut exit_reason = None;
    let mut exit_syndrome = None;
    let mut exit_virtual_address = None;
    let mut exit_physical_address = None;
    let mut watchdog_cancel_status = None;
    let mut vcpu_destroy_status = None;
    let mut unmap_status = None;
    let mut vm_destroy_status = None;
    let mut deallocate_status = None;

    let mut memory = ptr::null_mut();
    let mut vcpu = 0;
    let mut exit = ptr::null_mut();

    let status = unsafe { hv_vm_create(ptr::null_mut()) };
    let vm_create_status = Some(status);
    let vm_created = status == HV_SUCCESS;
    if !vm_created {
        blockers.push(format!("hv_vm_create failed: {status:#x}"));
    }

    if vm_created {
        let status = unsafe { hv_vm_allocate(&mut memory, PROBE_BYTES, HV_ALLOCATE_DEFAULT) };
        allocate_status = Some(status);
        memory_allocated = status == HV_SUCCESS && !memory.is_null();
        if memory_allocated {
            let instruction = AARCH64_LDR_X0_FROM_X1.to_le_bytes();
            unsafe {
                ptr::copy_nonoverlapping(
                    instruction.as_ptr(),
                    memory.cast::<u8>(),
                    instruction.len(),
                );
            }
        } else {
            blockers.push(format!("hv_vm_allocate failed: {status:#x}"));
        }
    }

    if vm_created && memory_allocated {
        let status = unsafe {
            hv_vm_map(
                memory,
                PROBE_IPA_START,
                PROBE_BYTES,
                HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC,
            )
        };
        map_status = Some(status);
        memory_mapped = status == HV_SUCCESS;
        if !memory_mapped {
            blockers.push(format!("hv_vm_map failed: {status:#x}"));
        }
    }

    if vm_created && memory_mapped {
        let status = unsafe { hv_vcpu_create(&mut vcpu, &mut exit, ptr::null_mut()) };
        vcpu_create_status = Some(status);
        vcpu_created = status == HV_SUCCESS;
        if !vcpu_created {
            blockers.push(format!("hv_vcpu_create failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, PROBE_IPA_START) };
        pc_set_status = Some(status);
        pc_set = status == HV_SUCCESS;
        if !pc_set {
            blockers.push(format!("hv_vcpu_set_reg(PC) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_CPSR, AARCH64_PSTATE_EL1H_DAIF_MASKED) };
        cpsr_set_status = Some(status);
        cpsr_set = status == HV_SUCCESS;
        if !cpsr_set {
            blockers.push(format!("hv_vcpu_set_reg(CPSR) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X1, PROBE_MMIO_IPA) };
        address_register_set_status = Some(status);
        address_register_set = status == HV_SUCCESS;
        if !address_register_set {
            blockers.push(format!("hv_vcpu_set_reg(X1) failed: {status:#x}"));
        }
    }

    if vcpu_created && pc_set && cpsr_set && address_register_set {
        run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        run_status = Some(observation.run_status);
        exit_reason = observation.exit_reason;
        exit_syndrome = observation.exit_syndrome;
        exit_virtual_address = observation.exit_virtual_address;
        exit_physical_address = observation.exit_physical_address;
        watchdog_cancel_status = observation.watchdog_cancel_status;
        if watchdog_cancel_status.is_some() {
            blockers.push("MMIO read watchdog fired before exception exit".to_string());
        }

        if observation.run_status == HV_SUCCESS {
            if exit_reason.is_none() {
                blockers
                    .push("hv_vcpu_run returned success without an exit info pointer".to_string());
            } else {
                mmio_exit_observed = exit_reason == Some(HV_EXIT_REASON_EXCEPTION)
                    && (exit_virtual_address == Some(PROBE_MMIO_IPA)
                        || exit_physical_address == Some(PROBE_MMIO_IPA)
                        || exit_syndrome.is_some_and(is_data_abort_syndrome));
                if exit_reason != Some(HV_EXIT_REASON_EXCEPTION) {
                    blockers.push(format!(
                        "hv_vcpu_run returned non-exception exit reason: {}",
                        exit_reason.unwrap_or_default()
                    ));
                }
                if !mmio_exit_observed {
                    blockers.push(format!(
                        "hv_vcpu_run did not report an MMIO/data-abort style exit for {PROBE_MMIO_IPA:#x}"
                    ));
                }
            }
        } else {
            blockers.push(format!("hv_vcpu_run failed: {:#x}", observation.run_status));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_destroy(vcpu) };
        vcpu_destroy_status = Some(status);
        vcpu_destroyed = status == HV_SUCCESS;
        if !vcpu_destroyed {
            blockers.push(format!("hv_vcpu_destroy failed: {status:#x}"));
        }
    }

    if memory_mapped {
        let status = unsafe { hv_vm_unmap(PROBE_IPA_START, PROBE_BYTES) };
        unmap_status = Some(status);
        memory_unmapped = status == HV_SUCCESS;
        if !memory_unmapped {
            blockers.push(format!("hv_vm_unmap failed: {status:#x}"));
        }
    }

    if vm_created {
        let status = unsafe { hv_vm_destroy() };
        vm_destroy_status = Some(status);
        vm_destroyed = status == HV_SUCCESS;
        if !vm_destroyed {
            blockers.push(format!("hv_vm_destroy failed: {status:#x}"));
        }
    }

    if memory_allocated {
        let status = unsafe { hv_vm_deallocate(memory, PROBE_BYTES) };
        deallocate_status = Some(status);
        memory_deallocated = status == HV_SUCCESS;
        if !memory_deallocated {
            blockers.push(format!("hv_vm_deallocate failed: {status:#x}"));
        }
    }

    HvfMmioReadExitProbe {
        allowed: true,
        attempted: true,
        vm_created,
        memory_allocated,
        memory_mapped,
        vcpu_created,
        pc_set,
        cpsr_set,
        address_register_set,
        run_attempted,
        mmio_exit_observed,
        watchdog_cancel_fired: watchdog_cancel_status.is_some(),
        vcpu_destroyed,
        memory_unmapped,
        vm_destroyed,
        memory_deallocated,
        host,
        code_ipa_start: PROBE_IPA_START,
        mmio_ipa: PROBE_MMIO_IPA,
        bytes: PROBE_BYTES,
        instruction: "LDR X0, [X1]",
        vm_create_status,
        allocate_status,
        map_status,
        vcpu_create_status,
        pc_set_status,
        cpsr_set_status,
        address_register_set_status,
        run_status,
        exit_reason,
        exit_syndrome,
        exit_virtual_address,
        exit_physical_address,
        watchdog_cancel_status,
        vcpu_destroy_status,
        unmap_status,
        vm_destroy_status,
        deallocate_status,
        blockers,
    }
}

fn mmio_read_exit_probe_result(
    allowed: bool,
    attempted: bool,
    host: HvfHostCapabilities,
    blockers: Vec<String>,
) -> HvfMmioReadExitProbe {
    HvfMmioReadExitProbe {
        allowed,
        attempted,
        vm_created: false,
        memory_allocated: false,
        memory_mapped: false,
        vcpu_created: false,
        pc_set: false,
        cpsr_set: false,
        address_register_set: false,
        run_attempted: false,
        mmio_exit_observed: false,
        watchdog_cancel_fired: false,
        vcpu_destroyed: false,
        memory_unmapped: false,
        vm_destroyed: false,
        memory_deallocated: false,
        host,
        code_ipa_start: PROBE_IPA_START,
        mmio_ipa: PROBE_MMIO_IPA,
        bytes: PROBE_BYTES,
        instruction: "LDR X0, [X1]",
        vm_create_status: None,
        allocate_status: None,
        map_status: None,
        vcpu_create_status: None,
        pc_set_status: None,
        cpsr_set_status: None,
        address_register_set_status: None,
        run_status: None,
        exit_reason: None,
        exit_syndrome: None,
        exit_virtual_address: None,
        exit_physical_address: None,
        watchdog_cancel_status: None,
        vcpu_destroy_status: None,
        unmap_status: None,
        vm_destroy_status: None,
        deallocate_status: None,
        blockers,
    }
}

pub fn probe_hvf_mmio_read_emulation(
    allow_emulate: bool,
    host: HvfHostCapabilities,
) -> HvfMmioReadEmulationProbe {
    let mut blockers = Vec::new();

    if !allow_emulate {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_MMIO_EMULATION=1 or pass --allow-emulate to handle one unmapped LDR read, inject X0, advance PC, and continue to HVC".to_string(),
        );
        return mmio_read_emulation_probe_result(false, false, host, blockers);
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return mmio_read_emulation_probe_result(true, false, host, blockers);
    }

    let mut memory_allocated = false;
    let mut memory_mapped = false;
    let mut vcpu_created = false;
    let mut pc_set = false;
    let mut cpsr_set = false;
    let mut address_register_set = false;
    let mut first_run_attempted = false;
    let mut mmio_exit_observed = false;
    let mut pc_read_after_mmio_exit = false;
    let mut emulated_value_injected = false;
    let mut pc_advanced = false;
    let mut second_run_attempted = false;
    let mut continuation_exit_observed = false;
    let mut emulated_value_preserved = false;
    let mut vcpu_destroyed = false;
    let mut memory_unmapped = false;
    let mut vm_destroyed = false;
    let mut memory_deallocated = false;

    let mut allocate_status = None;
    let mut map_status = None;
    let mut vcpu_create_status = None;
    let mut pc_set_status = None;
    let mut cpsr_set_status = None;
    let mut address_register_set_status = None;
    let mut first_run_status = None;
    let mut mmio_exit_reason = None;
    let mut mmio_exit_syndrome = None;
    let mut mmio_exit_virtual_address = None;
    let mut mmio_exit_physical_address = None;
    let mut first_watchdog_cancel_status = None;
    let mut pc_read_status = None;
    let mut pc_after_mmio_exit = None;
    let mut emulated_value_set_status = None;
    let mut pc_advance_status = None;
    let mut second_run_status = None;
    let mut continuation_exit_reason = None;
    let mut continuation_exit_syndrome = None;
    let mut continuation_exit_virtual_address = None;
    let mut continuation_exit_physical_address = None;
    let mut second_watchdog_cancel_status = None;
    let mut emulated_value_read_status = None;
    let mut emulated_value_after_continue = None;
    let mut vcpu_destroy_status = None;
    let mut unmap_status = None;
    let mut vm_destroy_status = None;
    let mut deallocate_status = None;

    let mut memory = ptr::null_mut();
    let mut vcpu = 0;
    let mut exit = ptr::null_mut();

    let status = unsafe { hv_vm_create(ptr::null_mut()) };
    let vm_create_status = Some(status);
    let vm_created = status == HV_SUCCESS;
    if !vm_created {
        blockers.push(format!("hv_vm_create failed: {status:#x}"));
    }

    if vm_created {
        let status = unsafe { hv_vm_allocate(&mut memory, PROBE_BYTES, HV_ALLOCATE_DEFAULT) };
        allocate_status = Some(status);
        memory_allocated = status == HV_SUCCESS && !memory.is_null();
        if memory_allocated {
            let load = AARCH64_LDR_X0_FROM_X1.to_le_bytes();
            let hvc = AARCH64_HVC_0.to_le_bytes();
            unsafe {
                ptr::copy_nonoverlapping(load.as_ptr(), memory.cast::<u8>(), load.len());
                ptr::copy_nonoverlapping(
                    hvc.as_ptr(),
                    memory.cast::<u8>().add(load.len()),
                    hvc.len(),
                );
            }
        } else {
            blockers.push(format!("hv_vm_allocate failed: {status:#x}"));
        }
    }

    if vm_created && memory_allocated {
        let status = unsafe {
            hv_vm_map(
                memory,
                PROBE_IPA_START,
                PROBE_BYTES,
                HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC,
            )
        };
        map_status = Some(status);
        memory_mapped = status == HV_SUCCESS;
        if !memory_mapped {
            blockers.push(format!("hv_vm_map failed: {status:#x}"));
        }
    }

    if vm_created && memory_mapped {
        let status = unsafe { hv_vcpu_create(&mut vcpu, &mut exit, ptr::null_mut()) };
        vcpu_create_status = Some(status);
        vcpu_created = status == HV_SUCCESS;
        if !vcpu_created {
            blockers.push(format!("hv_vcpu_create failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, PROBE_IPA_START) };
        pc_set_status = Some(status);
        pc_set = status == HV_SUCCESS;
        if !pc_set {
            blockers.push(format!("hv_vcpu_set_reg(PC) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_CPSR, AARCH64_PSTATE_EL1H_DAIF_MASKED) };
        cpsr_set_status = Some(status);
        cpsr_set = status == HV_SUCCESS;
        if !cpsr_set {
            blockers.push(format!("hv_vcpu_set_reg(CPSR) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X1, PROBE_MMIO_IPA) };
        address_register_set_status = Some(status);
        address_register_set = status == HV_SUCCESS;
        if !address_register_set {
            blockers.push(format!("hv_vcpu_set_reg(X1) failed: {status:#x}"));
        }
    }

    if vcpu_created && pc_set && cpsr_set && address_register_set {
        first_run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        first_run_status = Some(observation.run_status);
        mmio_exit_reason = observation.exit_reason;
        mmio_exit_syndrome = observation.exit_syndrome;
        mmio_exit_virtual_address = observation.exit_virtual_address;
        mmio_exit_physical_address = observation.exit_physical_address;
        first_watchdog_cancel_status = observation.watchdog_cancel_status;
        if first_watchdog_cancel_status.is_some() {
            blockers
                .push("MMIO emulation first run watchdog fired before exception exit".to_string());
        }

        if observation.run_status == HV_SUCCESS {
            if mmio_exit_reason.is_none() {
                blockers.push(
                    "first hv_vcpu_run returned success without an exit info pointer".to_string(),
                );
            } else {
                mmio_exit_observed = mmio_exit_reason == Some(HV_EXIT_REASON_EXCEPTION)
                    && (mmio_exit_virtual_address == Some(PROBE_MMIO_IPA)
                        || mmio_exit_physical_address == Some(PROBE_MMIO_IPA)
                        || mmio_exit_syndrome.is_some_and(is_data_abort_syndrome));
                if !mmio_exit_observed {
                    blockers.push(format!(
                        "first hv_vcpu_run did not report an MMIO/data-abort style exit for {PROBE_MMIO_IPA:#x}"
                    ));
                }
            }
        } else {
            blockers.push(format!(
                "first hv_vcpu_run failed: {:#x}",
                observation.run_status
            ));
        }
    }

    if mmio_exit_observed {
        let mut pc = 0;
        let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc) };
        pc_read_status = Some(status);
        pc_read_after_mmio_exit = status == HV_SUCCESS;
        if pc_read_after_mmio_exit {
            pc_after_mmio_exit = Some(pc);
        } else {
            blockers.push(format!("hv_vcpu_get_reg(PC) failed: {status:#x}"));
        }
    }

    if mmio_exit_observed {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X0, EMULATED_MMIO_READ_VALUE) };
        emulated_value_set_status = Some(status);
        emulated_value_injected = status == HV_SUCCESS;
        if !emulated_value_injected {
            blockers.push(format!("hv_vcpu_set_reg(X0) failed: {status:#x}"));
        }
    }

    if pc_read_after_mmio_exit && emulated_value_injected {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, PROBE_IPA_START + 4) };
        pc_advance_status = Some(status);
        pc_advanced = status == HV_SUCCESS;
        if !pc_advanced {
            blockers.push(format!("hv_vcpu_set_reg(PC + 4) failed: {status:#x}"));
        }
    }

    if pc_advanced {
        second_run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        second_run_status = Some(observation.run_status);
        continuation_exit_reason = observation.exit_reason;
        continuation_exit_syndrome = observation.exit_syndrome;
        continuation_exit_virtual_address = observation.exit_virtual_address;
        continuation_exit_physical_address = observation.exit_physical_address;
        second_watchdog_cancel_status = observation.watchdog_cancel_status;
        if second_watchdog_cancel_status.is_some() {
            blockers.push("MMIO emulation second run watchdog fired before HVC exit".to_string());
        }

        if observation.run_status == HV_SUCCESS {
            if continuation_exit_reason.is_none() {
                blockers.push(
                    "second hv_vcpu_run returned success without an exit info pointer".to_string(),
                );
            } else {
                continuation_exit_observed = continuation_exit_reason
                    == Some(HV_EXIT_REASON_EXCEPTION)
                    && continuation_exit_syndrome == Some(AARCH64_HVC_0_SYNDROME);
                if !continuation_exit_observed {
                    blockers.push(format!(
                        "second hv_vcpu_run did not reach HVC continuation exit; syndrome: {}",
                        continuation_exit_syndrome.map_or_else(
                            || "not observed".to_string(),
                            |value| { format!("{value:#x}") }
                        )
                    ));
                }
            }
        } else {
            blockers.push(format!(
                "second hv_vcpu_run failed: {:#x}",
                observation.run_status
            ));
        }
    }

    if continuation_exit_observed {
        let mut value = 0;
        let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_X0, &mut value) };
        emulated_value_read_status = Some(status);
        if status == HV_SUCCESS {
            emulated_value_after_continue = Some(value);
            emulated_value_preserved = value == EMULATED_MMIO_READ_VALUE;
            if !emulated_value_preserved {
                blockers.push(format!(
                    "emulated MMIO value changed before continuation HVC: {value:#x}"
                ));
            }
        } else {
            blockers.push(format!("hv_vcpu_get_reg(X0) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_destroy(vcpu) };
        vcpu_destroy_status = Some(status);
        vcpu_destroyed = status == HV_SUCCESS;
        if !vcpu_destroyed {
            blockers.push(format!("hv_vcpu_destroy failed: {status:#x}"));
        }
    }

    if memory_mapped {
        let status = unsafe { hv_vm_unmap(PROBE_IPA_START, PROBE_BYTES) };
        unmap_status = Some(status);
        memory_unmapped = status == HV_SUCCESS;
        if !memory_unmapped {
            blockers.push(format!("hv_vm_unmap failed: {status:#x}"));
        }
    }

    if vm_created {
        let status = unsafe { hv_vm_destroy() };
        vm_destroy_status = Some(status);
        vm_destroyed = status == HV_SUCCESS;
        if !vm_destroyed {
            blockers.push(format!("hv_vm_destroy failed: {status:#x}"));
        }
    }

    if memory_allocated {
        let status = unsafe { hv_vm_deallocate(memory, PROBE_BYTES) };
        deallocate_status = Some(status);
        memory_deallocated = status == HV_SUCCESS;
        if !memory_deallocated {
            blockers.push(format!("hv_vm_deallocate failed: {status:#x}"));
        }
    }

    let watchdog_cancel_fired =
        first_watchdog_cancel_status.is_some() || second_watchdog_cancel_status.is_some();

    HvfMmioReadEmulationProbe {
        allowed: true,
        attempted: true,
        vm_created,
        memory_allocated,
        memory_mapped,
        vcpu_created,
        pc_set,
        cpsr_set,
        address_register_set,
        first_run_attempted,
        mmio_exit_observed,
        pc_read_after_mmio_exit,
        emulated_value_injected,
        pc_advanced,
        second_run_attempted,
        continuation_exit_observed,
        emulated_value_preserved,
        watchdog_cancel_fired,
        vcpu_destroyed,
        memory_unmapped,
        vm_destroyed,
        memory_deallocated,
        host,
        code_ipa_start: PROBE_IPA_START,
        mmio_ipa: PROBE_MMIO_IPA,
        bytes: PROBE_BYTES,
        instructions: "LDR X0, [X1]; HVC #0",
        emulated_value: EMULATED_MMIO_READ_VALUE,
        vm_create_status,
        allocate_status,
        map_status,
        vcpu_create_status,
        pc_set_status,
        cpsr_set_status,
        address_register_set_status,
        first_run_status,
        mmio_exit_reason,
        mmio_exit_syndrome,
        mmio_exit_virtual_address,
        mmio_exit_physical_address,
        first_watchdog_cancel_status,
        pc_read_status,
        pc_after_mmio_exit,
        emulated_value_set_status,
        pc_advance_status,
        second_run_status,
        continuation_exit_reason,
        continuation_exit_syndrome,
        continuation_exit_virtual_address,
        continuation_exit_physical_address,
        second_watchdog_cancel_status,
        emulated_value_read_status,
        emulated_value_after_continue,
        vcpu_destroy_status,
        unmap_status,
        vm_destroy_status,
        deallocate_status,
        blockers,
    }
}

fn mmio_read_emulation_probe_result(
    allowed: bool,
    attempted: bool,
    host: HvfHostCapabilities,
    blockers: Vec<String>,
) -> HvfMmioReadEmulationProbe {
    HvfMmioReadEmulationProbe {
        allowed,
        attempted,
        vm_created: false,
        memory_allocated: false,
        memory_mapped: false,
        vcpu_created: false,
        pc_set: false,
        cpsr_set: false,
        address_register_set: false,
        first_run_attempted: false,
        mmio_exit_observed: false,
        pc_read_after_mmio_exit: false,
        emulated_value_injected: false,
        pc_advanced: false,
        second_run_attempted: false,
        continuation_exit_observed: false,
        emulated_value_preserved: false,
        watchdog_cancel_fired: false,
        vcpu_destroyed: false,
        memory_unmapped: false,
        vm_destroyed: false,
        memory_deallocated: false,
        host,
        code_ipa_start: PROBE_IPA_START,
        mmio_ipa: PROBE_MMIO_IPA,
        bytes: PROBE_BYTES,
        instructions: "LDR X0, [X1]; HVC #0",
        emulated_value: EMULATED_MMIO_READ_VALUE,
        vm_create_status: None,
        allocate_status: None,
        map_status: None,
        vcpu_create_status: None,
        pc_set_status: None,
        cpsr_set_status: None,
        address_register_set_status: None,
        first_run_status: None,
        mmio_exit_reason: None,
        mmio_exit_syndrome: None,
        mmio_exit_virtual_address: None,
        mmio_exit_physical_address: None,
        first_watchdog_cancel_status: None,
        pc_read_status: None,
        pc_after_mmio_exit: None,
        emulated_value_set_status: None,
        pc_advance_status: None,
        second_run_status: None,
        continuation_exit_reason: None,
        continuation_exit_syndrome: None,
        continuation_exit_virtual_address: None,
        continuation_exit_physical_address: None,
        second_watchdog_cancel_status: None,
        emulated_value_read_status: None,
        emulated_value_after_continue: None,
        vcpu_destroy_status: None,
        unmap_status: None,
        vm_destroy_status: None,
        deallocate_status: None,
        blockers,
    }
}

pub fn probe_hvf_mmio_write_emulation(
    allow_emulate: bool,
    host: HvfHostCapabilities,
) -> HvfMmioWriteEmulationProbe {
    let mut blockers = Vec::new();

    if !allow_emulate {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_MMIO_WRITE_EMULATION=1 or pass --allow-emulate to handle one unmapped STR write, capture X0, advance PC, and continue to HVC".to_string(),
        );
        return mmio_write_emulation_probe_result(false, false, host, blockers);
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return mmio_write_emulation_probe_result(true, false, host, blockers);
    }

    let mut memory_allocated = false;
    let mut memory_mapped = false;
    let mut vcpu_created = false;
    let mut pc_set = false;
    let mut cpsr_set = false;
    let mut write_value_register_set = false;
    let mut address_register_set = false;
    let mut first_run_attempted = false;
    let mut mmio_exit_observed = false;
    let mut pc_read_after_mmio_exit = false;
    let mut write_value_captured = false;
    let mut pc_advanced = false;
    let mut second_run_attempted = false;
    let mut continuation_exit_observed = false;
    let mut write_value_preserved = false;
    let mut vcpu_destroyed = false;
    let mut memory_unmapped = false;
    let mut vm_destroyed = false;
    let mut memory_deallocated = false;

    let mut allocate_status = None;
    let mut map_status = None;
    let mut vcpu_create_status = None;
    let mut pc_set_status = None;
    let mut cpsr_set_status = None;
    let mut write_value_register_set_status = None;
    let mut address_register_set_status = None;
    let mut first_run_status = None;
    let mut mmio_exit_reason = None;
    let mut mmio_exit_syndrome = None;
    let mut mmio_exit_virtual_address = None;
    let mut mmio_exit_physical_address = None;
    let mut first_watchdog_cancel_status = None;
    let mut pc_read_status = None;
    let mut pc_after_mmio_exit = None;
    let mut write_value_capture_status = None;
    let mut captured_write_value = None;
    let mut pc_advance_status = None;
    let mut second_run_status = None;
    let mut continuation_exit_reason = None;
    let mut continuation_exit_syndrome = None;
    let mut continuation_exit_virtual_address = None;
    let mut continuation_exit_physical_address = None;
    let mut second_watchdog_cancel_status = None;
    let mut write_value_after_continue_status = None;
    let mut write_value_after_continue = None;
    let mut vcpu_destroy_status = None;
    let mut unmap_status = None;
    let mut vm_destroy_status = None;
    let mut deallocate_status = None;

    let mut memory = ptr::null_mut();
    let mut vcpu = 0;
    let mut exit = ptr::null_mut();

    let status = unsafe { hv_vm_create(ptr::null_mut()) };
    let vm_create_status = Some(status);
    let vm_created = status == HV_SUCCESS;
    if !vm_created {
        blockers.push(format!("hv_vm_create failed: {status:#x}"));
    }

    if vm_created {
        let status = unsafe { hv_vm_allocate(&mut memory, PROBE_BYTES, HV_ALLOCATE_DEFAULT) };
        allocate_status = Some(status);
        memory_allocated = status == HV_SUCCESS && !memory.is_null();
        if memory_allocated {
            let store = AARCH64_STR_X0_TO_X1.to_le_bytes();
            let hvc = AARCH64_HVC_0.to_le_bytes();
            unsafe {
                ptr::copy_nonoverlapping(store.as_ptr(), memory.cast::<u8>(), store.len());
                ptr::copy_nonoverlapping(
                    hvc.as_ptr(),
                    memory.cast::<u8>().add(store.len()),
                    hvc.len(),
                );
            }
        } else {
            blockers.push(format!("hv_vm_allocate failed: {status:#x}"));
        }
    }

    if vm_created && memory_allocated {
        let status = unsafe {
            hv_vm_map(
                memory,
                PROBE_IPA_START,
                PROBE_BYTES,
                HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC,
            )
        };
        map_status = Some(status);
        memory_mapped = status == HV_SUCCESS;
        if !memory_mapped {
            blockers.push(format!("hv_vm_map failed: {status:#x}"));
        }
    }

    if vm_created && memory_mapped {
        let status = unsafe { hv_vcpu_create(&mut vcpu, &mut exit, ptr::null_mut()) };
        vcpu_create_status = Some(status);
        vcpu_created = status == HV_SUCCESS;
        if !vcpu_created {
            blockers.push(format!("hv_vcpu_create failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, PROBE_IPA_START) };
        pc_set_status = Some(status);
        pc_set = status == HV_SUCCESS;
        if !pc_set {
            blockers.push(format!("hv_vcpu_set_reg(PC) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_CPSR, AARCH64_PSTATE_EL1H_DAIF_MASKED) };
        cpsr_set_status = Some(status);
        cpsr_set = status == HV_SUCCESS;
        if !cpsr_set {
            blockers.push(format!("hv_vcpu_set_reg(CPSR) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X0, EMULATED_MMIO_WRITE_VALUE) };
        write_value_register_set_status = Some(status);
        write_value_register_set = status == HV_SUCCESS;
        if !write_value_register_set {
            blockers.push(format!("hv_vcpu_set_reg(X0) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X1, PROBE_MMIO_IPA) };
        address_register_set_status = Some(status);
        address_register_set = status == HV_SUCCESS;
        if !address_register_set {
            blockers.push(format!("hv_vcpu_set_reg(X1) failed: {status:#x}"));
        }
    }

    if vcpu_created && pc_set && cpsr_set && write_value_register_set && address_register_set {
        first_run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        first_run_status = Some(observation.run_status);
        mmio_exit_reason = observation.exit_reason;
        mmio_exit_syndrome = observation.exit_syndrome;
        mmio_exit_virtual_address = observation.exit_virtual_address;
        mmio_exit_physical_address = observation.exit_physical_address;
        first_watchdog_cancel_status = observation.watchdog_cancel_status;
        if first_watchdog_cancel_status.is_some() {
            blockers.push(
                "MMIO write emulation first run watchdog fired before exception exit".to_string(),
            );
        }

        if observation.run_status == HV_SUCCESS {
            if mmio_exit_reason.is_none() {
                blockers.push(
                    "first hv_vcpu_run returned success without an exit info pointer".to_string(),
                );
            } else {
                mmio_exit_observed = mmio_exit_reason == Some(HV_EXIT_REASON_EXCEPTION)
                    && (mmio_exit_virtual_address == Some(PROBE_MMIO_IPA)
                        || mmio_exit_physical_address == Some(PROBE_MMIO_IPA)
                        || mmio_exit_syndrome.is_some_and(is_data_abort_syndrome));
                if !mmio_exit_observed {
                    blockers.push(format!(
                        "first hv_vcpu_run did not report an MMIO/data-abort style write exit for {PROBE_MMIO_IPA:#x}"
                    ));
                }
            }
        } else {
            blockers.push(format!(
                "first hv_vcpu_run failed: {:#x}",
                observation.run_status
            ));
        }
    }

    if mmio_exit_observed {
        let mut pc = 0;
        let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc) };
        pc_read_status = Some(status);
        pc_read_after_mmio_exit = status == HV_SUCCESS;
        if pc_read_after_mmio_exit {
            pc_after_mmio_exit = Some(pc);
        } else {
            blockers.push(format!("hv_vcpu_get_reg(PC) failed: {status:#x}"));
        }
    }

    if mmio_exit_observed {
        let mut value = 0;
        let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_X0, &mut value) };
        write_value_capture_status = Some(status);
        if status == HV_SUCCESS {
            captured_write_value = Some(value);
            write_value_captured = value == EMULATED_MMIO_WRITE_VALUE;
            if !write_value_captured {
                blockers.push(format!(
                    "captured MMIO write value did not match X0 seed: {value:#x}"
                ));
            }
        } else {
            blockers.push(format!("hv_vcpu_get_reg(X0) failed: {status:#x}"));
        }
    }

    if pc_read_after_mmio_exit && write_value_captured {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, PROBE_IPA_START + 4) };
        pc_advance_status = Some(status);
        pc_advanced = status == HV_SUCCESS;
        if !pc_advanced {
            blockers.push(format!("hv_vcpu_set_reg(PC + 4) failed: {status:#x}"));
        }
    }

    if pc_advanced {
        second_run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        second_run_status = Some(observation.run_status);
        continuation_exit_reason = observation.exit_reason;
        continuation_exit_syndrome = observation.exit_syndrome;
        continuation_exit_virtual_address = observation.exit_virtual_address;
        continuation_exit_physical_address = observation.exit_physical_address;
        second_watchdog_cancel_status = observation.watchdog_cancel_status;
        if second_watchdog_cancel_status.is_some() {
            blockers
                .push("MMIO write emulation second run watchdog fired before HVC exit".to_string());
        }

        if observation.run_status == HV_SUCCESS {
            if continuation_exit_reason.is_none() {
                blockers.push(
                    "second hv_vcpu_run returned success without an exit info pointer".to_string(),
                );
            } else {
                continuation_exit_observed = continuation_exit_reason
                    == Some(HV_EXIT_REASON_EXCEPTION)
                    && continuation_exit_syndrome == Some(AARCH64_HVC_0_SYNDROME);
                if !continuation_exit_observed {
                    blockers.push(format!(
                        "second hv_vcpu_run did not reach HVC continuation exit; syndrome: {}",
                        continuation_exit_syndrome.map_or_else(
                            || "not observed".to_string(),
                            |value| { format!("{value:#x}") }
                        )
                    ));
                }
            }
        } else {
            blockers.push(format!(
                "second hv_vcpu_run failed: {:#x}",
                observation.run_status
            ));
        }
    }

    if continuation_exit_observed {
        let mut value = 0;
        let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_X0, &mut value) };
        write_value_after_continue_status = Some(status);
        if status == HV_SUCCESS {
            write_value_after_continue = Some(value);
            write_value_preserved = value == EMULATED_MMIO_WRITE_VALUE;
            if !write_value_preserved {
                blockers.push(format!(
                    "MMIO write value changed before continuation HVC: {value:#x}"
                ));
            }
        } else {
            blockers.push(format!("hv_vcpu_get_reg(X0) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_destroy(vcpu) };
        vcpu_destroy_status = Some(status);
        vcpu_destroyed = status == HV_SUCCESS;
        if !vcpu_destroyed {
            blockers.push(format!("hv_vcpu_destroy failed: {status:#x}"));
        }
    }

    if memory_mapped {
        let status = unsafe { hv_vm_unmap(PROBE_IPA_START, PROBE_BYTES) };
        unmap_status = Some(status);
        memory_unmapped = status == HV_SUCCESS;
        if !memory_unmapped {
            blockers.push(format!("hv_vm_unmap failed: {status:#x}"));
        }
    }

    if vm_created {
        let status = unsafe { hv_vm_destroy() };
        vm_destroy_status = Some(status);
        vm_destroyed = status == HV_SUCCESS;
        if !vm_destroyed {
            blockers.push(format!("hv_vm_destroy failed: {status:#x}"));
        }
    }

    if memory_allocated {
        let status = unsafe { hv_vm_deallocate(memory, PROBE_BYTES) };
        deallocate_status = Some(status);
        memory_deallocated = status == HV_SUCCESS;
        if !memory_deallocated {
            blockers.push(format!("hv_vm_deallocate failed: {status:#x}"));
        }
    }

    let watchdog_cancel_fired =
        first_watchdog_cancel_status.is_some() || second_watchdog_cancel_status.is_some();

    HvfMmioWriteEmulationProbe {
        allowed: true,
        attempted: true,
        vm_created,
        memory_allocated,
        memory_mapped,
        vcpu_created,
        pc_set,
        cpsr_set,
        write_value_register_set,
        address_register_set,
        first_run_attempted,
        mmio_exit_observed,
        pc_read_after_mmio_exit,
        write_value_captured,
        pc_advanced,
        second_run_attempted,
        continuation_exit_observed,
        write_value_preserved,
        watchdog_cancel_fired,
        vcpu_destroyed,
        memory_unmapped,
        vm_destroyed,
        memory_deallocated,
        host,
        code_ipa_start: PROBE_IPA_START,
        mmio_ipa: PROBE_MMIO_IPA,
        bytes: PROBE_BYTES,
        instructions: "STR X0, [X1]; HVC #0",
        write_value: EMULATED_MMIO_WRITE_VALUE,
        vm_create_status,
        allocate_status,
        map_status,
        vcpu_create_status,
        pc_set_status,
        cpsr_set_status,
        write_value_register_set_status,
        address_register_set_status,
        first_run_status,
        mmio_exit_reason,
        mmio_exit_syndrome,
        mmio_exit_virtual_address,
        mmio_exit_physical_address,
        first_watchdog_cancel_status,
        pc_read_status,
        pc_after_mmio_exit,
        write_value_capture_status,
        captured_write_value,
        pc_advance_status,
        second_run_status,
        continuation_exit_reason,
        continuation_exit_syndrome,
        continuation_exit_virtual_address,
        continuation_exit_physical_address,
        second_watchdog_cancel_status,
        write_value_after_continue_status,
        write_value_after_continue,
        vcpu_destroy_status,
        unmap_status,
        vm_destroy_status,
        deallocate_status,
        blockers,
    }
}

fn mmio_write_emulation_probe_result(
    allowed: bool,
    attempted: bool,
    host: HvfHostCapabilities,
    blockers: Vec<String>,
) -> HvfMmioWriteEmulationProbe {
    HvfMmioWriteEmulationProbe {
        allowed,
        attempted,
        vm_created: false,
        memory_allocated: false,
        memory_mapped: false,
        vcpu_created: false,
        pc_set: false,
        cpsr_set: false,
        write_value_register_set: false,
        address_register_set: false,
        first_run_attempted: false,
        mmio_exit_observed: false,
        pc_read_after_mmio_exit: false,
        write_value_captured: false,
        pc_advanced: false,
        second_run_attempted: false,
        continuation_exit_observed: false,
        write_value_preserved: false,
        watchdog_cancel_fired: false,
        vcpu_destroyed: false,
        memory_unmapped: false,
        vm_destroyed: false,
        memory_deallocated: false,
        host,
        code_ipa_start: PROBE_IPA_START,
        mmio_ipa: PROBE_MMIO_IPA,
        bytes: PROBE_BYTES,
        instructions: "STR X0, [X1]; HVC #0",
        write_value: EMULATED_MMIO_WRITE_VALUE,
        vm_create_status: None,
        allocate_status: None,
        map_status: None,
        vcpu_create_status: None,
        pc_set_status: None,
        cpsr_set_status: None,
        write_value_register_set_status: None,
        address_register_set_status: None,
        first_run_status: None,
        mmio_exit_reason: None,
        mmio_exit_syndrome: None,
        mmio_exit_virtual_address: None,
        mmio_exit_physical_address: None,
        first_watchdog_cancel_status: None,
        pc_read_status: None,
        pc_after_mmio_exit: None,
        write_value_capture_status: None,
        captured_write_value: None,
        pc_advance_status: None,
        second_run_status: None,
        continuation_exit_reason: None,
        continuation_exit_syndrome: None,
        continuation_exit_virtual_address: None,
        continuation_exit_physical_address: None,
        second_watchdog_cancel_status: None,
        write_value_after_continue_status: None,
        write_value_after_continue: None,
        vcpu_destroy_status: None,
        unmap_status: None,
        vm_destroy_status: None,
        deallocate_status: None,
        blockers,
    }
}

pub fn probe_hvf_mmio_serial_device(
    allow_device: bool,
    host: HvfHostCapabilities,
) -> HvfMmioSerialDeviceProbe {
    let mut blockers = Vec::new();

    if !allow_device {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_MMIO_SERIAL_DEVICE=1 or pass --allow-device to emulate one serial data write, one status read, and one HVC continuation".to_string(),
        );
        return mmio_serial_device_probe_result(false, false, host, blockers);
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return mmio_serial_device_probe_result(true, false, host, blockers);
    }

    let mut memory_allocated = false;
    let mut memory_mapped = false;
    let mut vcpu_created = false;
    let mut pc_set = false;
    let mut cpsr_set = false;
    let mut write_value_register_set = false;
    let mut data_address_register_set = false;
    let mut status_address_register_set = false;
    let mut write_run_attempted = false;
    let mut write_exit_observed = false;
    let mut write_handled_by_device = false;
    let mut write_value_captured = false;
    let mut pc_read_after_write = false;
    let mut pc_advanced_after_write = false;
    let mut status_run_attempted = false;
    let mut status_exit_observed = false;
    let mut status_handled_by_device = false;
    let mut status_value_injected = false;
    let mut pc_read_after_status = false;
    let mut pc_advanced_after_status = false;
    let mut continuation_run_attempted = false;
    let mut continuation_exit_observed = false;
    let mut status_value_preserved = false;
    let mut vcpu_destroyed = false;
    let mut memory_unmapped = false;
    let mut vm_destroyed = false;
    let mut memory_deallocated = false;

    let mut allocate_status = None;
    let mut map_status = None;
    let mut vcpu_create_status = None;
    let mut pc_set_status = None;
    let mut cpsr_set_status = None;
    let mut write_value_register_set_status = None;
    let mut data_address_register_set_status = None;
    let mut status_address_register_set_status = None;
    let mut write_run_status = None;
    let mut write_exit_reason = None;
    let mut write_exit_syndrome = None;
    let mut write_exit_virtual_address = None;
    let mut write_exit_physical_address = None;
    let mut write_watchdog_cancel_status = None;
    let mut write_value_capture_status = None;
    let mut captured_write_value = None;
    let mut captured_byte = None;
    let mut pc_read_after_write_status = None;
    let mut pc_after_write_exit = None;
    let mut pc_advance_after_write_status = None;
    let mut status_run_status = None;
    let mut status_exit_reason = None;
    let mut status_exit_syndrome = None;
    let mut status_exit_virtual_address = None;
    let mut status_exit_physical_address = None;
    let mut status_watchdog_cancel_status = None;
    let mut status_value_set_status = None;
    let mut pc_read_after_status_status = None;
    let mut pc_after_status_exit = None;
    let mut pc_advance_after_status_status = None;
    let mut continuation_run_status = None;
    let mut continuation_exit_reason = None;
    let mut continuation_exit_syndrome = None;
    let mut continuation_exit_virtual_address = None;
    let mut continuation_exit_physical_address = None;
    let mut continuation_watchdog_cancel_status = None;
    let mut status_value_after_continue_status = None;
    let mut status_value_after_continue = None;
    let mut vcpu_destroy_status = None;
    let mut unmap_status = None;
    let mut vm_destroy_status = None;
    let mut deallocate_status = None;

    let mut memory = ptr::null_mut();
    let mut vcpu = 0;
    let mut exit = ptr::null_mut();
    let mut mmio_bus = MmioBus::default();
    mmio_bus.attach(Box::new(Pl011UartDevice::new(
        PROBE_MMIO_IPA,
        SERIAL_MMIO_STATUS_VALUE,
    )));
    let device_bus_created = true;
    let device_bus_device_count = mmio_bus.device_count();

    let status = unsafe { hv_vm_create(ptr::null_mut()) };
    let vm_create_status = Some(status);
    let vm_created = status == HV_SUCCESS;
    if !vm_created {
        blockers.push(format!("hv_vm_create failed: {status:#x}"));
    }

    if vm_created {
        let status = unsafe { hv_vm_allocate(&mut memory, PROBE_BYTES, HV_ALLOCATE_DEFAULT) };
        allocate_status = Some(status);
        memory_allocated = status == HV_SUCCESS && !memory.is_null();
        if memory_allocated {
            let store_data = AARCH64_STR_X0_TO_X1.to_le_bytes();
            let load_status = AARCH64_LDR_X0_FROM_X2.to_le_bytes();
            let hvc = AARCH64_HVC_0.to_le_bytes();
            unsafe {
                ptr::copy_nonoverlapping(
                    store_data.as_ptr(),
                    memory.cast::<u8>(),
                    store_data.len(),
                );
                ptr::copy_nonoverlapping(
                    load_status.as_ptr(),
                    memory.cast::<u8>().add(store_data.len()),
                    load_status.len(),
                );
                ptr::copy_nonoverlapping(
                    hvc.as_ptr(),
                    memory
                        .cast::<u8>()
                        .add(store_data.len() + load_status.len()),
                    hvc.len(),
                );
            }
        } else {
            blockers.push(format!("hv_vm_allocate failed: {status:#x}"));
        }
    }

    if vm_created && memory_allocated {
        let status = unsafe {
            hv_vm_map(
                memory,
                PROBE_IPA_START,
                PROBE_BYTES,
                HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC,
            )
        };
        map_status = Some(status);
        memory_mapped = status == HV_SUCCESS;
        if !memory_mapped {
            blockers.push(format!("hv_vm_map failed: {status:#x}"));
        }
    }

    if vm_created && memory_mapped {
        let status = unsafe { hv_vcpu_create(&mut vcpu, &mut exit, ptr::null_mut()) };
        vcpu_create_status = Some(status);
        vcpu_created = status == HV_SUCCESS;
        if !vcpu_created {
            blockers.push(format!("hv_vcpu_create failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, PROBE_IPA_START) };
        pc_set_status = Some(status);
        pc_set = status == HV_SUCCESS;
        if !pc_set {
            blockers.push(format!("hv_vcpu_set_reg(PC) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_CPSR, AARCH64_PSTATE_EL1H_DAIF_MASKED) };
        cpsr_set_status = Some(status);
        cpsr_set = status == HV_SUCCESS;
        if !cpsr_set {
            blockers.push(format!("hv_vcpu_set_reg(CPSR) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X0, SERIAL_MMIO_WRITE_VALUE) };
        write_value_register_set_status = Some(status);
        write_value_register_set = status == HV_SUCCESS;
        if !write_value_register_set {
            blockers.push(format!("hv_vcpu_set_reg(X0) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X1, SERIAL_MMIO_DATA_IPA) };
        data_address_register_set_status = Some(status);
        data_address_register_set = status == HV_SUCCESS;
        if !data_address_register_set {
            blockers.push(format!("hv_vcpu_set_reg(X1) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X2, SERIAL_MMIO_STATUS_IPA) };
        status_address_register_set_status = Some(status);
        status_address_register_set = status == HV_SUCCESS;
        if !status_address_register_set {
            blockers.push(format!("hv_vcpu_set_reg(X2) failed: {status:#x}"));
        }
    }

    if vcpu_created
        && pc_set
        && cpsr_set
        && write_value_register_set
        && data_address_register_set
        && status_address_register_set
    {
        write_run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        write_run_status = Some(observation.run_status);
        write_exit_reason = observation.exit_reason;
        write_exit_syndrome = observation.exit_syndrome;
        write_exit_virtual_address = observation.exit_virtual_address;
        write_exit_physical_address = observation.exit_physical_address;
        write_watchdog_cancel_status = observation.watchdog_cancel_status;
        if write_watchdog_cancel_status.is_some() {
            blockers.push("serial data write watchdog fired before exception exit".to_string());
        }

        if observation.run_status == HV_SUCCESS {
            if write_exit_reason.is_none() {
                blockers.push(
                    "serial data write returned success without an exit info pointer".to_string(),
                );
            } else {
                write_exit_observed = write_exit_reason == Some(HV_EXIT_REASON_EXCEPTION)
                    && (write_exit_virtual_address == Some(SERIAL_MMIO_DATA_IPA)
                        || write_exit_physical_address == Some(SERIAL_MMIO_DATA_IPA)
                        || write_exit_syndrome.is_some_and(is_data_abort_syndrome));
                if !write_exit_observed {
                    blockers.push(format!(
                        "serial data write did not exit at data IPA {SERIAL_MMIO_DATA_IPA:#x}"
                    ));
                }
            }
        } else {
            blockers.push(format!(
                "serial data write hv_vcpu_run failed: {:#x}",
                observation.run_status
            ));
        }
    }

    if write_exit_observed {
        let mut value = 0;
        let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_X0, &mut value) };
        write_value_capture_status = Some(status);
        if status == HV_SUCCESS {
            match mmio_bus.dispatch(MmioAccess::write(SERIAL_MMIO_DATA_IPA, value, 8)) {
                MmioAction::WriteAccepted { value, byte } => {
                    write_handled_by_device = true;
                    captured_write_value = Some(value);
                    captured_byte = Some(byte);
                    write_value_captured = value == SERIAL_MMIO_WRITE_VALUE;
                    if !write_value_captured {
                        blockers.push(format!(
                            "serial data write captured unexpected value: {value:#x}"
                        ));
                    }
                }
                MmioAction::Unhandled | MmioAction::ReadValue(_) => {
                    blockers.push(format!(
                        "serial data write was not handled by the MMIO device bus at {SERIAL_MMIO_DATA_IPA:#x}"
                    ));
                }
            }
        } else {
            blockers.push(format!("hv_vcpu_get_reg(X0) failed: {status:#x}"));
        }
    }

    if write_exit_observed {
        let mut pc = 0;
        let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc) };
        pc_read_after_write_status = Some(status);
        pc_read_after_write = status == HV_SUCCESS;
        if pc_read_after_write {
            pc_after_write_exit = Some(pc);
        } else {
            blockers.push(format!(
                "hv_vcpu_get_reg(PC after write) failed: {status:#x}"
            ));
        }
    }

    if pc_read_after_write && write_value_captured {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, PROBE_IPA_START + 4) };
        pc_advance_after_write_status = Some(status);
        pc_advanced_after_write = status == HV_SUCCESS;
        if !pc_advanced_after_write {
            blockers.push(format!("hv_vcpu_set_reg(PC + 4) failed: {status:#x}"));
        }
    }

    if pc_advanced_after_write {
        status_run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        status_run_status = Some(observation.run_status);
        status_exit_reason = observation.exit_reason;
        status_exit_syndrome = observation.exit_syndrome;
        status_exit_virtual_address = observation.exit_virtual_address;
        status_exit_physical_address = observation.exit_physical_address;
        status_watchdog_cancel_status = observation.watchdog_cancel_status;
        if status_watchdog_cancel_status.is_some() {
            blockers.push("serial status read watchdog fired before exception exit".to_string());
        }

        if observation.run_status == HV_SUCCESS {
            if status_exit_reason.is_none() {
                blockers.push(
                    "serial status read returned success without an exit info pointer".to_string(),
                );
            } else {
                status_exit_observed = status_exit_reason == Some(HV_EXIT_REASON_EXCEPTION)
                    && (status_exit_virtual_address == Some(SERIAL_MMIO_STATUS_IPA)
                        || status_exit_physical_address == Some(SERIAL_MMIO_STATUS_IPA)
                        || status_exit_syndrome.is_some_and(is_data_abort_syndrome));
                if !status_exit_observed {
                    blockers.push(format!(
                        "serial status read did not exit at status IPA {SERIAL_MMIO_STATUS_IPA:#x}"
                    ));
                }
            }
        } else {
            blockers.push(format!(
                "serial status read hv_vcpu_run failed: {:#x}",
                observation.run_status
            ));
        }
    }

    if status_exit_observed {
        match mmio_bus.dispatch(MmioAccess::read(SERIAL_MMIO_STATUS_IPA, 8)) {
            MmioAction::ReadValue(value) => {
                status_handled_by_device = true;
                let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X0, value) };
                status_value_set_status = Some(status);
                status_value_injected = status == HV_SUCCESS;
                if value != SERIAL_MMIO_STATUS_VALUE {
                    blockers.push(format!(
                        "serial status device returned unexpected value: {value:#x}"
                    ));
                }
                if !status_value_injected {
                    blockers.push(format!("hv_vcpu_set_reg(X0 status) failed: {status:#x}"));
                }
            }
            MmioAction::Unhandled | MmioAction::WriteAccepted { .. } => {
                blockers.push(format!(
                    "serial status read was not handled by the MMIO device bus at {SERIAL_MMIO_STATUS_IPA:#x}"
                ));
            }
        }
    }

    if status_exit_observed {
        let mut pc = 0;
        let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc) };
        pc_read_after_status_status = Some(status);
        pc_read_after_status = status == HV_SUCCESS;
        if pc_read_after_status {
            pc_after_status_exit = Some(pc);
        } else {
            blockers.push(format!(
                "hv_vcpu_get_reg(PC after status) failed: {status:#x}"
            ));
        }
    }

    if pc_read_after_status && status_value_injected {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, PROBE_IPA_START + 8) };
        pc_advance_after_status_status = Some(status);
        pc_advanced_after_status = status == HV_SUCCESS;
        if !pc_advanced_after_status {
            blockers.push(format!("hv_vcpu_set_reg(PC + 8) failed: {status:#x}"));
        }
    }

    if pc_advanced_after_status {
        continuation_run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        continuation_run_status = Some(observation.run_status);
        continuation_exit_reason = observation.exit_reason;
        continuation_exit_syndrome = observation.exit_syndrome;
        continuation_exit_virtual_address = observation.exit_virtual_address;
        continuation_exit_physical_address = observation.exit_physical_address;
        continuation_watchdog_cancel_status = observation.watchdog_cancel_status;
        if continuation_watchdog_cancel_status.is_some() {
            blockers.push("serial continuation watchdog fired before HVC exit".to_string());
        }

        if observation.run_status == HV_SUCCESS {
            if continuation_exit_reason.is_none() {
                blockers.push(
                    "serial continuation returned success without an exit info pointer".to_string(),
                );
            } else {
                continuation_exit_observed = continuation_exit_reason
                    == Some(HV_EXIT_REASON_EXCEPTION)
                    && continuation_exit_syndrome == Some(AARCH64_HVC_0_SYNDROME);
                if !continuation_exit_observed {
                    blockers.push(format!(
                        "serial continuation did not reach HVC; syndrome: {}",
                        continuation_exit_syndrome.map_or_else(
                            || "not observed".to_string(),
                            |value| { format!("{value:#x}") }
                        )
                    ));
                }
            }
        } else {
            blockers.push(format!(
                "serial continuation hv_vcpu_run failed: {:#x}",
                observation.run_status
            ));
        }
    }

    if continuation_exit_observed {
        let mut value = 0;
        let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_X0, &mut value) };
        status_value_after_continue_status = Some(status);
        if status == HV_SUCCESS {
            status_value_after_continue = Some(value);
            status_value_preserved = value == SERIAL_MMIO_STATUS_VALUE;
            if !status_value_preserved {
                blockers.push(format!(
                    "serial status value changed before continuation HVC: {value:#x}"
                ));
            }
        } else {
            blockers.push(format!(
                "hv_vcpu_get_reg(X0 after continue) failed: {status:#x}"
            ));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_destroy(vcpu) };
        vcpu_destroy_status = Some(status);
        vcpu_destroyed = status == HV_SUCCESS;
        if !vcpu_destroyed {
            blockers.push(format!("hv_vcpu_destroy failed: {status:#x}"));
        }
    }

    if memory_mapped {
        let status = unsafe { hv_vm_unmap(PROBE_IPA_START, PROBE_BYTES) };
        unmap_status = Some(status);
        memory_unmapped = status == HV_SUCCESS;
        if !memory_unmapped {
            blockers.push(format!("hv_vm_unmap failed: {status:#x}"));
        }
    }

    if vm_created {
        let status = unsafe { hv_vm_destroy() };
        vm_destroy_status = Some(status);
        vm_destroyed = status == HV_SUCCESS;
        if !vm_destroyed {
            blockers.push(format!("hv_vm_destroy failed: {status:#x}"));
        }
    }

    if memory_allocated {
        let status = unsafe { hv_vm_deallocate(memory, PROBE_BYTES) };
        deallocate_status = Some(status);
        memory_deallocated = status == HV_SUCCESS;
        if !memory_deallocated {
            blockers.push(format!("hv_vm_deallocate failed: {status:#x}"));
        }
    }

    let watchdog_cancel_fired = write_watchdog_cancel_status.is_some()
        || status_watchdog_cancel_status.is_some()
        || continuation_watchdog_cancel_status.is_some();

    HvfMmioSerialDeviceProbe {
        allowed: true,
        attempted: true,
        vm_created,
        memory_allocated,
        memory_mapped,
        vcpu_created,
        pc_set,
        cpsr_set,
        write_value_register_set,
        data_address_register_set,
        status_address_register_set,
        device_bus_created,
        device_bus_device_count,
        write_run_attempted,
        write_exit_observed,
        write_handled_by_device,
        write_value_captured,
        pc_advanced_after_write,
        status_run_attempted,
        status_exit_observed,
        status_handled_by_device,
        status_value_injected,
        pc_advanced_after_status,
        continuation_run_attempted,
        continuation_exit_observed,
        status_value_preserved,
        watchdog_cancel_fired,
        vcpu_destroyed,
        memory_unmapped,
        vm_destroyed,
        memory_deallocated,
        host,
        device_model: PL011_UART_MODEL,
        code_ipa_start: PROBE_IPA_START,
        data_ipa: SERIAL_MMIO_DATA_IPA,
        status_ipa: SERIAL_MMIO_STATUS_IPA,
        bytes: PROBE_BYTES,
        instructions: "STR X0, [X1]; LDR X0, [X2]; HVC #0",
        serial_write_value: SERIAL_MMIO_WRITE_VALUE,
        serial_status_value: SERIAL_MMIO_STATUS_VALUE,
        captured_write_value,
        captured_byte,
        vm_create_status,
        allocate_status,
        map_status,
        vcpu_create_status,
        pc_set_status,
        cpsr_set_status,
        write_value_register_set_status,
        data_address_register_set_status,
        status_address_register_set_status,
        write_run_status,
        write_exit_reason,
        write_exit_syndrome,
        write_exit_virtual_address,
        write_exit_physical_address,
        write_watchdog_cancel_status,
        write_value_capture_status,
        pc_read_after_write_status,
        pc_after_write_exit,
        pc_advance_after_write_status,
        status_run_status,
        status_exit_reason,
        status_exit_syndrome,
        status_exit_virtual_address,
        status_exit_physical_address,
        status_watchdog_cancel_status,
        status_value_set_status,
        pc_read_after_status_status,
        pc_after_status_exit,
        pc_advance_after_status_status,
        continuation_run_status,
        continuation_exit_reason,
        continuation_exit_syndrome,
        continuation_exit_virtual_address,
        continuation_exit_physical_address,
        continuation_watchdog_cancel_status,
        status_value_after_continue_status,
        status_value_after_continue,
        vcpu_destroy_status,
        unmap_status,
        vm_destroy_status,
        deallocate_status,
        blockers,
    }
}

fn mmio_serial_device_probe_result(
    allowed: bool,
    attempted: bool,
    host: HvfHostCapabilities,
    blockers: Vec<String>,
) -> HvfMmioSerialDeviceProbe {
    HvfMmioSerialDeviceProbe {
        allowed,
        attempted,
        vm_created: false,
        memory_allocated: false,
        memory_mapped: false,
        vcpu_created: false,
        pc_set: false,
        cpsr_set: false,
        write_value_register_set: false,
        data_address_register_set: false,
        status_address_register_set: false,
        device_bus_created: false,
        device_bus_device_count: 0,
        write_run_attempted: false,
        write_exit_observed: false,
        write_handled_by_device: false,
        write_value_captured: false,
        pc_advanced_after_write: false,
        status_run_attempted: false,
        status_exit_observed: false,
        status_handled_by_device: false,
        status_value_injected: false,
        pc_advanced_after_status: false,
        continuation_run_attempted: false,
        continuation_exit_observed: false,
        status_value_preserved: false,
        watchdog_cancel_fired: false,
        vcpu_destroyed: false,
        memory_unmapped: false,
        vm_destroyed: false,
        memory_deallocated: false,
        host,
        device_model: PL011_UART_MODEL,
        code_ipa_start: PROBE_IPA_START,
        data_ipa: SERIAL_MMIO_DATA_IPA,
        status_ipa: SERIAL_MMIO_STATUS_IPA,
        bytes: PROBE_BYTES,
        instructions: "STR X0, [X1]; LDR X0, [X2]; HVC #0",
        serial_write_value: SERIAL_MMIO_WRITE_VALUE,
        serial_status_value: SERIAL_MMIO_STATUS_VALUE,
        captured_write_value: None,
        captured_byte: None,
        vm_create_status: None,
        allocate_status: None,
        map_status: None,
        vcpu_create_status: None,
        pc_set_status: None,
        cpsr_set_status: None,
        write_value_register_set_status: None,
        data_address_register_set_status: None,
        status_address_register_set_status: None,
        write_run_status: None,
        write_exit_reason: None,
        write_exit_syndrome: None,
        write_exit_virtual_address: None,
        write_exit_physical_address: None,
        write_watchdog_cancel_status: None,
        write_value_capture_status: None,
        pc_read_after_write_status: None,
        pc_after_write_exit: None,
        pc_advance_after_write_status: None,
        status_run_status: None,
        status_exit_reason: None,
        status_exit_syndrome: None,
        status_exit_virtual_address: None,
        status_exit_physical_address: None,
        status_watchdog_cancel_status: None,
        status_value_set_status: None,
        pc_read_after_status_status: None,
        pc_after_status_exit: None,
        pc_advance_after_status_status: None,
        continuation_run_status: None,
        continuation_exit_reason: None,
        continuation_exit_syndrome: None,
        continuation_exit_virtual_address: None,
        continuation_exit_physical_address: None,
        continuation_watchdog_cancel_status: None,
        status_value_after_continue_status: None,
        status_value_after_continue: None,
        vcpu_destroy_status: None,
        unmap_status: None,
        vm_destroy_status: None,
        deallocate_status: None,
        blockers,
    }
}

pub fn probe_hvf_mmio_rtc_device(
    allow_device: bool,
    host: HvfHostCapabilities,
) -> HvfMmioRtcDeviceProbe {
    let mut blockers = Vec::new();

    if !allow_device {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_MMIO_RTC_DEVICE=1 or pass --allow-device to emulate a PL031 RTC read through the multi-device MMIO bus".to_string(),
        );
        return mmio_rtc_device_probe_result(false, false, host, blockers);
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return mmio_rtc_device_probe_result(true, false, host, blockers);
    }

    let mut memory_allocated = false;
    let mut memory_mapped = false;
    let mut vcpu_created = false;
    let mut pc_set = false;
    let mut cpsr_set = false;
    let mut rtc_address_register_set = false;
    let mut first_run_attempted = false;
    let mut rtc_exit_observed = false;
    let mut rtc_handled_by_device = false;
    let mut rtc_value_injected = false;
    let mut pc_read_after_rtc_exit = false;
    let mut pc_advanced = false;
    let mut second_run_attempted = false;
    let mut continuation_exit_observed = false;
    let mut rtc_value_preserved = false;
    let mut vcpu_destroyed = false;
    let mut memory_unmapped = false;
    let mut vm_destroyed = false;
    let mut memory_deallocated = false;

    let mut allocate_status = None;
    let mut map_status = None;
    let mut vcpu_create_status = None;
    let mut pc_set_status = None;
    let mut cpsr_set_status = None;
    let mut rtc_address_register_set_status = None;
    let mut first_run_status = None;
    let mut rtc_exit_reason = None;
    let mut rtc_exit_syndrome = None;
    let mut rtc_exit_virtual_address = None;
    let mut rtc_exit_physical_address = None;
    let mut first_watchdog_cancel_status = None;
    let mut rtc_value_set_status = None;
    let mut pc_read_status = None;
    let mut pc_after_rtc_exit = None;
    let mut pc_advance_status = None;
    let mut second_run_status = None;
    let mut continuation_exit_reason = None;
    let mut continuation_exit_syndrome = None;
    let mut continuation_exit_virtual_address = None;
    let mut continuation_exit_physical_address = None;
    let mut second_watchdog_cancel_status = None;
    let mut rtc_value_after_continue_status = None;
    let mut rtc_value_after_continue = None;
    let mut vcpu_destroy_status = None;
    let mut unmap_status = None;
    let mut vm_destroy_status = None;
    let mut deallocate_status = None;

    let mut memory = ptr::null_mut();
    let mut vcpu = 0;
    let mut exit = ptr::null_mut();
    let mut mmio_bus = MmioBus::default();
    mmio_bus.attach(Box::new(Pl011UartDevice::new(
        PROBE_MMIO_IPA,
        SERIAL_MMIO_STATUS_VALUE,
    )));
    mmio_bus.attach(Box::new(Pl031RtcDevice::new(
        RTC_MMIO_IPA,
        RTC_MMIO_READ_VALUE,
    )));
    let device_bus_created = true;
    let device_bus_device_count = mmio_bus.device_count();

    let status = unsafe { hv_vm_create(ptr::null_mut()) };
    let vm_create_status = Some(status);
    let vm_created = status == HV_SUCCESS;
    if !vm_created {
        blockers.push(format!("hv_vm_create failed: {status:#x}"));
    }

    if vm_created {
        let status = unsafe { hv_vm_allocate(&mut memory, PROBE_BYTES, HV_ALLOCATE_DEFAULT) };
        allocate_status = Some(status);
        memory_allocated = status == HV_SUCCESS && !memory.is_null();
        if memory_allocated {
            let load_rtc = AARCH64_LDR_X0_FROM_X1.to_le_bytes();
            let hvc = AARCH64_HVC_0.to_le_bytes();
            unsafe {
                ptr::copy_nonoverlapping(load_rtc.as_ptr(), memory.cast::<u8>(), load_rtc.len());
                ptr::copy_nonoverlapping(
                    hvc.as_ptr(),
                    memory.cast::<u8>().add(load_rtc.len()),
                    hvc.len(),
                );
            }
        } else {
            blockers.push(format!("hv_vm_allocate failed: {status:#x}"));
        }
    }

    if vm_created && memory_allocated {
        let status = unsafe {
            hv_vm_map(
                memory,
                PROBE_IPA_START,
                PROBE_BYTES,
                HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC,
            )
        };
        map_status = Some(status);
        memory_mapped = status == HV_SUCCESS;
        if !memory_mapped {
            blockers.push(format!("hv_vm_map failed: {status:#x}"));
        }
    }

    if vm_created && memory_mapped {
        let status = unsafe { hv_vcpu_create(&mut vcpu, &mut exit, ptr::null_mut()) };
        vcpu_create_status = Some(status);
        vcpu_created = status == HV_SUCCESS;
        if !vcpu_created {
            blockers.push(format!("hv_vcpu_create failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, PROBE_IPA_START) };
        pc_set_status = Some(status);
        pc_set = status == HV_SUCCESS;
        if !pc_set {
            blockers.push(format!("hv_vcpu_set_reg(PC) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_CPSR, AARCH64_PSTATE_EL1H_DAIF_MASKED) };
        cpsr_set_status = Some(status);
        cpsr_set = status == HV_SUCCESS;
        if !cpsr_set {
            blockers.push(format!("hv_vcpu_set_reg(CPSR) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X1, RTC_MMIO_IPA) };
        rtc_address_register_set_status = Some(status);
        rtc_address_register_set = status == HV_SUCCESS;
        if !rtc_address_register_set {
            blockers.push(format!("hv_vcpu_set_reg(X1 RTC) failed: {status:#x}"));
        }
    }

    if vcpu_created && pc_set && cpsr_set && rtc_address_register_set {
        first_run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        first_run_status = Some(observation.run_status);
        rtc_exit_reason = observation.exit_reason;
        rtc_exit_syndrome = observation.exit_syndrome;
        rtc_exit_virtual_address = observation.exit_virtual_address;
        rtc_exit_physical_address = observation.exit_physical_address;
        first_watchdog_cancel_status = observation.watchdog_cancel_status;
        if first_watchdog_cancel_status.is_some() {
            blockers.push("RTC read watchdog fired before exception exit".to_string());
        }

        if observation.run_status == HV_SUCCESS {
            if rtc_exit_reason.is_none() {
                blockers.push("RTC read returned success without an exit info pointer".to_string());
            } else {
                rtc_exit_observed = rtc_exit_reason == Some(HV_EXIT_REASON_EXCEPTION)
                    && (rtc_exit_virtual_address == Some(RTC_MMIO_IPA)
                        || rtc_exit_physical_address == Some(RTC_MMIO_IPA)
                        || rtc_exit_syndrome.is_some_and(is_data_abort_syndrome));
                if !rtc_exit_observed {
                    blockers.push(format!(
                        "RTC read did not exit at RTC IPA {RTC_MMIO_IPA:#x}"
                    ));
                }
            }
        } else {
            blockers.push(format!(
                "RTC read hv_vcpu_run failed: {:#x}",
                observation.run_status
            ));
        }
    }

    if rtc_exit_observed {
        match mmio_bus.dispatch(MmioAccess::read(RTC_MMIO_IPA, 8)) {
            MmioAction::ReadValue(value) => {
                rtc_handled_by_device = true;
                let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X0, value) };
                rtc_value_set_status = Some(status);
                rtc_value_injected = status == HV_SUCCESS;
                if value != RTC_MMIO_READ_VALUE {
                    blockers.push(format!("RTC device returned unexpected value: {value:#x}"));
                }
                if !rtc_value_injected {
                    blockers.push(format!("hv_vcpu_set_reg(X0 RTC) failed: {status:#x}"));
                }
            }
            MmioAction::Unhandled | MmioAction::WriteAccepted { .. } => {
                blockers.push(format!(
                    "RTC read was not handled by the MMIO device bus at {RTC_MMIO_IPA:#x}"
                ));
            }
        }
    }

    if rtc_exit_observed {
        let mut pc = 0;
        let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc) };
        pc_read_status = Some(status);
        pc_read_after_rtc_exit = status == HV_SUCCESS;
        if pc_read_after_rtc_exit {
            pc_after_rtc_exit = Some(pc);
        } else {
            blockers.push(format!("hv_vcpu_get_reg(PC after RTC) failed: {status:#x}"));
        }
    }

    if pc_read_after_rtc_exit && rtc_value_injected {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, PROBE_IPA_START + 4) };
        pc_advance_status = Some(status);
        pc_advanced = status == HV_SUCCESS;
        if !pc_advanced {
            blockers.push(format!("hv_vcpu_set_reg(PC + 4) failed: {status:#x}"));
        }
    }

    if pc_advanced {
        second_run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        second_run_status = Some(observation.run_status);
        continuation_exit_reason = observation.exit_reason;
        continuation_exit_syndrome = observation.exit_syndrome;
        continuation_exit_virtual_address = observation.exit_virtual_address;
        continuation_exit_physical_address = observation.exit_physical_address;
        second_watchdog_cancel_status = observation.watchdog_cancel_status;
        if second_watchdog_cancel_status.is_some() {
            blockers.push("RTC continuation watchdog fired before HVC exit".to_string());
        }

        if observation.run_status == HV_SUCCESS {
            if continuation_exit_reason.is_none() {
                blockers.push(
                    "RTC continuation returned success without an exit info pointer".to_string(),
                );
            } else {
                continuation_exit_observed = continuation_exit_reason
                    == Some(HV_EXIT_REASON_EXCEPTION)
                    && continuation_exit_syndrome == Some(AARCH64_HVC_0_SYNDROME);
                if !continuation_exit_observed {
                    blockers.push(format!(
                        "RTC continuation did not reach HVC; syndrome: {}",
                        continuation_exit_syndrome.map_or_else(
                            || "not observed".to_string(),
                            |value| format!("{value:#x}")
                        )
                    ));
                }
            }
        } else {
            blockers.push(format!(
                "RTC continuation hv_vcpu_run failed: {:#x}",
                observation.run_status
            ));
        }
    }

    if continuation_exit_observed {
        let mut value = 0;
        let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_X0, &mut value) };
        rtc_value_after_continue_status = Some(status);
        if status == HV_SUCCESS {
            rtc_value_after_continue = Some(value);
            rtc_value_preserved = value == RTC_MMIO_READ_VALUE;
            if !rtc_value_preserved {
                blockers.push(format!(
                    "RTC value changed before continuation HVC: {value:#x}"
                ));
            }
        } else {
            blockers.push(format!(
                "hv_vcpu_get_reg(X0 after RTC continue) failed: {status:#x}"
            ));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_destroy(vcpu) };
        vcpu_destroy_status = Some(status);
        vcpu_destroyed = status == HV_SUCCESS;
        if !vcpu_destroyed {
            blockers.push(format!("hv_vcpu_destroy failed: {status:#x}"));
        }
    }

    if memory_mapped {
        let status = unsafe { hv_vm_unmap(PROBE_IPA_START, PROBE_BYTES) };
        unmap_status = Some(status);
        memory_unmapped = status == HV_SUCCESS;
        if !memory_unmapped {
            blockers.push(format!("hv_vm_unmap failed: {status:#x}"));
        }
    }

    if vm_created {
        let status = unsafe { hv_vm_destroy() };
        vm_destroy_status = Some(status);
        vm_destroyed = status == HV_SUCCESS;
        if !vm_destroyed {
            blockers.push(format!("hv_vm_destroy failed: {status:#x}"));
        }
    }

    if memory_allocated {
        let status = unsafe { hv_vm_deallocate(memory, PROBE_BYTES) };
        deallocate_status = Some(status);
        memory_deallocated = status == HV_SUCCESS;
        if !memory_deallocated {
            blockers.push(format!("hv_vm_deallocate failed: {status:#x}"));
        }
    }

    let watchdog_cancel_fired =
        first_watchdog_cancel_status.is_some() || second_watchdog_cancel_status.is_some();

    HvfMmioRtcDeviceProbe {
        allowed: true,
        attempted: true,
        vm_created,
        memory_allocated,
        memory_mapped,
        vcpu_created,
        pc_set,
        cpsr_set,
        rtc_address_register_set,
        device_bus_created,
        device_bus_device_count,
        first_run_attempted,
        rtc_exit_observed,
        rtc_handled_by_device,
        rtc_value_injected,
        pc_read_after_rtc_exit,
        pc_advanced,
        second_run_attempted,
        continuation_exit_observed,
        rtc_value_preserved,
        watchdog_cancel_fired,
        vcpu_destroyed,
        memory_unmapped,
        vm_destroyed,
        memory_deallocated,
        host,
        device_models: BOOT_MMIO_DEVICE_MODELS,
        code_ipa_start: PROBE_IPA_START,
        uart_ipa: SERIAL_MMIO_DATA_IPA,
        rtc_ipa: RTC_MMIO_IPA,
        bytes: PROBE_BYTES,
        instructions: "LDR X0, [X1]; HVC #0",
        rtc_value: RTC_MMIO_READ_VALUE,
        vm_create_status,
        allocate_status,
        map_status,
        vcpu_create_status,
        pc_set_status,
        cpsr_set_status,
        rtc_address_register_set_status,
        first_run_status,
        rtc_exit_reason,
        rtc_exit_syndrome,
        rtc_exit_virtual_address,
        rtc_exit_physical_address,
        first_watchdog_cancel_status,
        rtc_value_set_status,
        pc_read_status,
        pc_after_rtc_exit,
        pc_advance_status,
        second_run_status,
        continuation_exit_reason,
        continuation_exit_syndrome,
        continuation_exit_virtual_address,
        continuation_exit_physical_address,
        second_watchdog_cancel_status,
        rtc_value_after_continue_status,
        rtc_value_after_continue,
        vcpu_destroy_status,
        unmap_status,
        vm_destroy_status,
        deallocate_status,
        blockers,
    }
}

fn mmio_rtc_device_probe_result(
    allowed: bool,
    attempted: bool,
    host: HvfHostCapabilities,
    blockers: Vec<String>,
) -> HvfMmioRtcDeviceProbe {
    HvfMmioRtcDeviceProbe {
        allowed,
        attempted,
        vm_created: false,
        memory_allocated: false,
        memory_mapped: false,
        vcpu_created: false,
        pc_set: false,
        cpsr_set: false,
        rtc_address_register_set: false,
        device_bus_created: false,
        device_bus_device_count: 0,
        first_run_attempted: false,
        rtc_exit_observed: false,
        rtc_handled_by_device: false,
        rtc_value_injected: false,
        pc_read_after_rtc_exit: false,
        pc_advanced: false,
        second_run_attempted: false,
        continuation_exit_observed: false,
        rtc_value_preserved: false,
        watchdog_cancel_fired: false,
        vcpu_destroyed: false,
        memory_unmapped: false,
        vm_destroyed: false,
        memory_deallocated: false,
        host,
        device_models: BOOT_MMIO_DEVICE_MODELS,
        code_ipa_start: PROBE_IPA_START,
        uart_ipa: SERIAL_MMIO_DATA_IPA,
        rtc_ipa: RTC_MMIO_IPA,
        bytes: PROBE_BYTES,
        instructions: "LDR X0, [X1]; HVC #0",
        rtc_value: RTC_MMIO_READ_VALUE,
        vm_create_status: None,
        allocate_status: None,
        map_status: None,
        vcpu_create_status: None,
        pc_set_status: None,
        cpsr_set_status: None,
        rtc_address_register_set_status: None,
        first_run_status: None,
        rtc_exit_reason: None,
        rtc_exit_syndrome: None,
        rtc_exit_virtual_address: None,
        rtc_exit_physical_address: None,
        first_watchdog_cancel_status: None,
        rtc_value_set_status: None,
        pc_read_status: None,
        pc_after_rtc_exit: None,
        pc_advance_status: None,
        second_run_status: None,
        continuation_exit_reason: None,
        continuation_exit_syndrome: None,
        continuation_exit_virtual_address: None,
        continuation_exit_physical_address: None,
        second_watchdog_cancel_status: None,
        rtc_value_after_continue_status: None,
        rtc_value_after_continue: None,
        vcpu_destroy_status: None,
        unmap_status: None,
        vm_destroy_status: None,
        deallocate_status: None,
        blockers,
    }
}

#[derive(Clone, Copy)]
struct BlockIdentityRegisterSpec {
    name: &'static str,
    ipa: u64,
    value: u64,
    address_reg: u32,
    instruction: u32,
}

fn block_identity_register_specs() -> [BlockIdentityRegisterSpec; 4] {
    [
        BlockIdentityRegisterSpec {
            name: "magic",
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_MAGIC_VALUE_OFFSET,
            value: VIRTIO_MMIO_MAGIC_VALUE,
            address_reg: HV_REG_X1,
            instruction: AARCH64_LDR_W0_FROM_X1,
        },
        BlockIdentityRegisterSpec {
            name: "version",
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_VERSION_OFFSET,
            value: VIRTIO_MMIO_VERSION_VALUE,
            address_reg: HV_REG_X2,
            instruction: AARCH64_LDR_W0_FROM_X2,
        },
        BlockIdentityRegisterSpec {
            name: "device_id",
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_DEVICE_ID_OFFSET,
            value: VIRTIO_MMIO_BLOCK_DEVICE_ID_VALUE,
            address_reg: HV_REG_X3,
            instruction: AARCH64_LDR_W0_FROM_X3,
        },
        BlockIdentityRegisterSpec {
            name: "vendor_id",
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_VENDOR_ID_OFFSET,
            value: VIRTIO_MMIO_VENDOR_ID_VALUE,
            address_reg: HV_REG_X4,
            instruction: AARCH64_LDR_W0_FROM_X4,
        },
    ]
}

fn block_register_probe_defaults() -> Vec<HvfMmioBlockRegisterProbe> {
    block_identity_register_specs()
        .iter()
        .map(|spec| HvfMmioBlockRegisterProbe {
            name: spec.name,
            ipa: spec.ipa,
            expected_value: spec.value,
            run_attempted: false,
            exit_observed: false,
            handled_by_device: false,
            value_injected: false,
            pc_read_after_exit: false,
            pc_advanced: false,
            run_status: None,
            exit_reason: None,
            exit_syndrome: None,
            exit_virtual_address: None,
            exit_physical_address: None,
            watchdog_cancel_status: None,
            value_set_status: None,
            pc_read_status: None,
            pc_after_exit: None,
            pc_advance_status: None,
        })
        .collect()
}

pub fn probe_hvf_mmio_block_device(
    allow_device: bool,
    host: HvfHostCapabilities,
) -> HvfMmioBlockDeviceProbe {
    let mut blockers = Vec::new();

    if !allow_device {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_MMIO_BLOCK_DEVICE=1 or pass --allow-device to emulate VirtIO-MMIO block identity reads through the MMIO bus".to_string(),
        );
        return mmio_block_device_probe_result(false, false, host, blockers);
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return mmio_block_device_probe_result(true, false, host, blockers);
    }

    let specs = block_identity_register_specs();
    let mut register_reads = block_register_probe_defaults();
    let mut memory_allocated = false;
    let mut memory_mapped = false;
    let mut vcpu_created = false;
    let mut pc_set = false;
    let mut cpsr_set = false;
    let mut register_address_registers_set = false;
    let mut continuation_run_attempted = false;
    let mut continuation_exit_observed = false;
    let mut vendor_value_preserved = false;
    let mut vcpu_destroyed = false;
    let mut memory_unmapped = false;
    let mut vm_destroyed = false;
    let mut memory_deallocated = false;

    let mut allocate_status = None;
    let mut map_status = None;
    let mut vcpu_create_status = None;
    let mut pc_set_status = None;
    let mut cpsr_set_status = None;
    let mut register_address_registers_set_status = vec![None; specs.len()];
    let mut continuation_run_status = None;
    let mut continuation_exit_reason = None;
    let mut continuation_exit_syndrome = None;
    let mut continuation_exit_virtual_address = None;
    let mut continuation_exit_physical_address = None;
    let mut continuation_watchdog_cancel_status = None;
    let mut vendor_value_after_continue_status = None;
    let mut vendor_value_after_continue = None;
    let mut vcpu_destroy_status = None;
    let mut unmap_status = None;
    let mut vm_destroy_status = None;
    let mut deallocate_status = None;

    let mut memory = ptr::null_mut();
    let mut vcpu = 0;
    let mut exit = ptr::null_mut();
    let mut mmio_bus = MmioBus::default();
    mmio_bus.attach(Box::new(Pl011UartDevice::new(
        PROBE_MMIO_IPA,
        SERIAL_MMIO_STATUS_VALUE,
    )));
    mmio_bus.attach(Box::new(Pl031RtcDevice::new(
        RTC_MMIO_IPA,
        RTC_MMIO_READ_VALUE,
    )));
    mmio_bus.attach(Box::new(VirtioMmioBlockDevice::new(BLOCK_MMIO_IPA)));
    let device_bus_created = true;
    let device_bus_device_count = mmio_bus.device_count();

    let status = unsafe { hv_vm_create(ptr::null_mut()) };
    let vm_create_status = Some(status);
    let vm_created = status == HV_SUCCESS;
    if !vm_created {
        blockers.push(format!("hv_vm_create failed: {status:#x}"));
    }

    if vm_created {
        let status = unsafe { hv_vm_allocate(&mut memory, PROBE_BYTES, HV_ALLOCATE_DEFAULT) };
        allocate_status = Some(status);
        memory_allocated = status == HV_SUCCESS && !memory.is_null();
        if memory_allocated {
            for (index, spec) in specs.iter().enumerate() {
                let instruction = spec.instruction.to_le_bytes();
                unsafe {
                    ptr::copy_nonoverlapping(
                        instruction.as_ptr(),
                        memory.cast::<u8>().add(index * instruction.len()),
                        instruction.len(),
                    );
                }
            }
            let hvc = AARCH64_HVC_0.to_le_bytes();
            unsafe {
                ptr::copy_nonoverlapping(
                    hvc.as_ptr(),
                    memory.cast::<u8>().add(specs.len() * hvc.len()),
                    hvc.len(),
                );
            }
        } else {
            blockers.push(format!("hv_vm_allocate failed: {status:#x}"));
        }
    }

    if vm_created && memory_allocated {
        let status = unsafe {
            hv_vm_map(
                memory,
                PROBE_IPA_START,
                PROBE_BYTES,
                HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC,
            )
        };
        map_status = Some(status);
        memory_mapped = status == HV_SUCCESS;
        if !memory_mapped {
            blockers.push(format!("hv_vm_map failed: {status:#x}"));
        }
    }

    if vm_created && memory_mapped {
        let status = unsafe { hv_vcpu_create(&mut vcpu, &mut exit, ptr::null_mut()) };
        vcpu_create_status = Some(status);
        vcpu_created = status == HV_SUCCESS;
        if !vcpu_created {
            blockers.push(format!("hv_vcpu_create failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, PROBE_IPA_START) };
        pc_set_status = Some(status);
        pc_set = status == HV_SUCCESS;
        if !pc_set {
            blockers.push(format!("hv_vcpu_set_reg(PC) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_CPSR, AARCH64_PSTATE_EL1H_DAIF_MASKED) };
        cpsr_set_status = Some(status);
        cpsr_set = status == HV_SUCCESS;
        if !cpsr_set {
            blockers.push(format!("hv_vcpu_set_reg(CPSR) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        for (index, spec) in specs.iter().enumerate() {
            let status = unsafe { hv_vcpu_set_reg(vcpu, spec.address_reg, spec.ipa) };
            register_address_registers_set_status[index] = Some(status);
            if status != HV_SUCCESS {
                blockers.push(format!(
                    "hv_vcpu_set_reg(X{} {}) failed: {status:#x}",
                    index + 1,
                    spec.name
                ));
            }
        }
        register_address_registers_set = register_address_registers_set_status
            .iter()
            .all(|status| *status == Some(HV_SUCCESS));
    }

    let mut can_continue = vcpu_created && pc_set && cpsr_set && register_address_registers_set;
    for (index, spec) in specs.iter().enumerate() {
        if !can_continue {
            break;
        }

        let read = &mut register_reads[index];
        read.run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        read.run_status = Some(observation.run_status);
        read.exit_reason = observation.exit_reason;
        read.exit_syndrome = observation.exit_syndrome;
        read.exit_virtual_address = observation.exit_virtual_address;
        read.exit_physical_address = observation.exit_physical_address;
        read.watchdog_cancel_status = observation.watchdog_cancel_status;
        if read.watchdog_cancel_status.is_some() {
            blockers.push(format!(
                "VirtIO block {} read watchdog fired before exception exit",
                spec.name
            ));
            can_continue = false;
        }

        if observation.run_status == HV_SUCCESS {
            if read.exit_reason.is_none() {
                blockers.push(format!(
                    "VirtIO block {} read returned success without an exit info pointer",
                    spec.name
                ));
                can_continue = false;
            } else {
                read.exit_observed = read.exit_reason == Some(HV_EXIT_REASON_EXCEPTION)
                    && (read.exit_virtual_address == Some(spec.ipa)
                        || read.exit_physical_address == Some(spec.ipa)
                        || read.exit_syndrome.is_some_and(is_data_abort_syndrome));
                if !read.exit_observed {
                    blockers.push(format!(
                        "VirtIO block {} read did not exit at IPA {:#x}",
                        spec.name, spec.ipa
                    ));
                    can_continue = false;
                }
            }
        } else {
            blockers.push(format!(
                "VirtIO block {} hv_vcpu_run failed: {:#x}",
                spec.name, observation.run_status
            ));
            can_continue = false;
        }

        if read.exit_observed {
            match mmio_bus.dispatch(MmioAccess::read(spec.ipa, 4)) {
                MmioAction::ReadValue(value) => {
                    read.handled_by_device = true;
                    if value != spec.value {
                        blockers.push(format!(
                            "VirtIO block {} returned unexpected value: {value:#x}",
                            spec.name
                        ));
                        can_continue = false;
                    }
                    let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X0, value) };
                    read.value_set_status = Some(status);
                    read.value_injected = status == HV_SUCCESS;
                    if !read.value_injected {
                        blockers.push(format!(
                            "hv_vcpu_set_reg(X0 {}) failed: {status:#x}",
                            spec.name
                        ));
                        can_continue = false;
                    }
                }
                MmioAction::Unhandled | MmioAction::WriteAccepted { .. } => {
                    blockers.push(format!(
                        "VirtIO block {} read was not handled by the MMIO device bus at {:#x}",
                        spec.name, spec.ipa
                    ));
                    can_continue = false;
                }
            }
        }

        if read.exit_observed {
            let mut pc = 0;
            let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc) };
            read.pc_read_status = Some(status);
            read.pc_read_after_exit = status == HV_SUCCESS;
            if read.pc_read_after_exit {
                read.pc_after_exit = Some(pc);
            } else {
                blockers.push(format!(
                    "hv_vcpu_get_reg(PC after VirtIO block {}) failed: {status:#x}",
                    spec.name
                ));
                can_continue = false;
            }
        }

        if read.pc_read_after_exit && read.value_injected {
            let next_pc = PROBE_IPA_START + ((index as u64 + 1) * 4);
            let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, next_pc) };
            read.pc_advance_status = Some(status);
            read.pc_advanced = status == HV_SUCCESS;
            if !read.pc_advanced {
                blockers.push(format!(
                    "hv_vcpu_set_reg(PC after VirtIO block {}) failed: {status:#x}",
                    spec.name
                ));
                can_continue = false;
            }
        }
    }

    if register_reads.iter().all(|read| read.pc_advanced) {
        continuation_run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        continuation_run_status = Some(observation.run_status);
        continuation_exit_reason = observation.exit_reason;
        continuation_exit_syndrome = observation.exit_syndrome;
        continuation_exit_virtual_address = observation.exit_virtual_address;
        continuation_exit_physical_address = observation.exit_physical_address;
        continuation_watchdog_cancel_status = observation.watchdog_cancel_status;
        if continuation_watchdog_cancel_status.is_some() {
            blockers.push("VirtIO block continuation watchdog fired before HVC exit".to_string());
        }

        if observation.run_status == HV_SUCCESS {
            if continuation_exit_reason.is_none() {
                blockers.push(
                    "VirtIO block continuation returned success without an exit info pointer"
                        .to_string(),
                );
            } else {
                continuation_exit_observed = continuation_exit_reason
                    == Some(HV_EXIT_REASON_EXCEPTION)
                    && continuation_exit_syndrome == Some(AARCH64_HVC_0_SYNDROME);
                if !continuation_exit_observed {
                    blockers.push(format!(
                        "VirtIO block continuation did not reach HVC; syndrome: {}",
                        continuation_exit_syndrome.map_or_else(
                            || "not observed".to_string(),
                            |value| format!("{value:#x}")
                        )
                    ));
                }
            }
        } else {
            blockers.push(format!(
                "VirtIO block continuation hv_vcpu_run failed: {:#x}",
                observation.run_status
            ));
        }
    }

    if continuation_exit_observed {
        let mut value = 0;
        let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_X0, &mut value) };
        vendor_value_after_continue_status = Some(status);
        if status == HV_SUCCESS {
            vendor_value_after_continue = Some(value);
            vendor_value_preserved = value == VIRTIO_MMIO_VENDOR_ID_VALUE;
            if !vendor_value_preserved {
                blockers.push(format!(
                    "VirtIO block vendor value changed before continuation HVC: {value:#x}"
                ));
            }
        } else {
            blockers.push(format!(
                "hv_vcpu_get_reg(X0 after VirtIO block continue) failed: {status:#x}"
            ));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_destroy(vcpu) };
        vcpu_destroy_status = Some(status);
        vcpu_destroyed = status == HV_SUCCESS;
        if !vcpu_destroyed {
            blockers.push(format!("hv_vcpu_destroy failed: {status:#x}"));
        }
    }

    if memory_mapped {
        let status = unsafe { hv_vm_unmap(PROBE_IPA_START, PROBE_BYTES) };
        unmap_status = Some(status);
        memory_unmapped = status == HV_SUCCESS;
        if !memory_unmapped {
            blockers.push(format!("hv_vm_unmap failed: {status:#x}"));
        }
    }

    if vm_created {
        let status = unsafe { hv_vm_destroy() };
        vm_destroy_status = Some(status);
        vm_destroyed = status == HV_SUCCESS;
        if !vm_destroyed {
            blockers.push(format!("hv_vm_destroy failed: {status:#x}"));
        }
    }

    if memory_allocated {
        let status = unsafe { hv_vm_deallocate(memory, PROBE_BYTES) };
        deallocate_status = Some(status);
        memory_deallocated = status == HV_SUCCESS;
        if !memory_deallocated {
            blockers.push(format!("hv_vm_deallocate failed: {status:#x}"));
        }
    }

    let watchdog_cancel_fired = register_reads
        .iter()
        .any(|read| read.watchdog_cancel_status.is_some())
        || continuation_watchdog_cancel_status.is_some();

    HvfMmioBlockDeviceProbe {
        allowed: true,
        attempted: true,
        vm_created,
        memory_allocated,
        memory_mapped,
        vcpu_created,
        pc_set,
        cpsr_set,
        register_address_registers_set,
        device_bus_created,
        device_bus_device_count,
        register_reads,
        continuation_run_attempted,
        continuation_exit_observed,
        vendor_value_preserved,
        watchdog_cancel_fired,
        vcpu_destroyed,
        memory_unmapped,
        vm_destroyed,
        memory_deallocated,
        host,
        device_models: BOOT_MMIO_DEVICE_MODELS,
        code_ipa_start: PROBE_IPA_START,
        block_ipa: BLOCK_MMIO_IPA,
        bytes: PROBE_BYTES,
        instructions: "LDR W0 magic/version/device/vendor; HVC #0",
        magic_value: VIRTIO_MMIO_MAGIC_VALUE,
        version_value: VIRTIO_MMIO_VERSION_VALUE,
        device_id_value: VIRTIO_MMIO_BLOCK_DEVICE_ID_VALUE,
        vendor_id_value: VIRTIO_MMIO_VENDOR_ID_VALUE,
        vm_create_status,
        allocate_status,
        map_status,
        vcpu_create_status,
        pc_set_status,
        cpsr_set_status,
        register_address_registers_set_status,
        continuation_run_status,
        continuation_exit_reason,
        continuation_exit_syndrome,
        continuation_exit_virtual_address,
        continuation_exit_physical_address,
        continuation_watchdog_cancel_status,
        vendor_value_after_continue_status,
        vendor_value_after_continue,
        vcpu_destroy_status,
        unmap_status,
        vm_destroy_status,
        deallocate_status,
        blockers,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BlockQueueAccessKind {
    Read,
    Write,
}

impl BlockQueueAccessKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
        }
    }
}

#[derive(Clone, Copy)]
struct BlockQueueStepSpec {
    name: &'static str,
    access: BlockQueueAccessKind,
    ipa: u64,
    expected_value: Option<u64>,
    write_value: Option<u64>,
    instruction: u32,
}

fn block_queue_step_specs() -> [BlockQueueStepSpec; 26] {
    [
        BlockQueueStepSpec {
            name: "device_features",
            access: BlockQueueAccessKind::Read,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_DEVICE_FEATURES_OFFSET,
            expected_value: Some(VIRTIO_MMIO_BLOCK_DEVICE_FEATURES_VALUE),
            write_value: None,
            instruction: AARCH64_LDR_W0_FROM_X1,
        },
        BlockQueueStepSpec {
            name: "driver_features",
            access: BlockQueueAccessKind::Write,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_DRIVER_FEATURES_OFFSET,
            expected_value: None,
            write_value: Some(VIRTIO_MMIO_BLOCK_DRIVER_FEATURES_VALUE),
            instruction: AARCH64_STR_W0_TO_X1,
        },
        BlockQueueStepSpec {
            name: "status_ack",
            access: BlockQueueAccessKind::Write,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_STATUS_OFFSET,
            expected_value: None,
            write_value: Some(VIRTIO_MMIO_BLOCK_STATUS_ACK_VALUE),
            instruction: AARCH64_STR_W0_TO_X1,
        },
        BlockQueueStepSpec {
            name: "status_driver",
            access: BlockQueueAccessKind::Write,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_STATUS_OFFSET,
            expected_value: None,
            write_value: Some(VIRTIO_MMIO_BLOCK_STATUS_DRIVER_VALUE),
            instruction: AARCH64_STR_W0_TO_X1,
        },
        BlockQueueStepSpec {
            name: "status_features_ok",
            access: BlockQueueAccessKind::Write,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_STATUS_OFFSET,
            expected_value: None,
            write_value: Some(VIRTIO_MMIO_BLOCK_STATUS_FEATURES_OK_VALUE),
            instruction: AARCH64_STR_W0_TO_X1,
        },
        BlockQueueStepSpec {
            name: "queue_select",
            access: BlockQueueAccessKind::Write,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_QUEUE_SEL_OFFSET,
            expected_value: None,
            write_value: Some(VIRTIO_MMIO_BLOCK_QUEUE_SEL_VALUE),
            instruction: AARCH64_STR_W0_TO_X1,
        },
        BlockQueueStepSpec {
            name: "queue_num_max",
            access: BlockQueueAccessKind::Read,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_QUEUE_NUM_MAX_OFFSET,
            expected_value: Some(VIRTIO_MMIO_BLOCK_QUEUE_NUM_MAX_VALUE),
            write_value: None,
            instruction: AARCH64_LDR_W0_FROM_X1,
        },
        BlockQueueStepSpec {
            name: "queue_num",
            access: BlockQueueAccessKind::Write,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_QUEUE_NUM_OFFSET,
            expected_value: None,
            write_value: Some(VIRTIO_MMIO_BLOCK_QUEUE_NUM_VALUE),
            instruction: AARCH64_STR_W0_TO_X1,
        },
        BlockQueueStepSpec {
            name: "queue_desc_low",
            access: BlockQueueAccessKind::Write,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET,
            expected_value: None,
            write_value: Some(VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS & 0xffff_ffff),
            instruction: AARCH64_STR_W0_TO_X1,
        },
        BlockQueueStepSpec {
            name: "queue_desc_high",
            access: BlockQueueAccessKind::Write,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_QUEUE_DESC_HIGH_OFFSET,
            expected_value: None,
            write_value: Some(VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS >> 32),
            instruction: AARCH64_STR_W0_TO_X1,
        },
        BlockQueueStepSpec {
            name: "queue_driver_low",
            access: BlockQueueAccessKind::Write,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET,
            expected_value: None,
            write_value: Some(VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS & 0xffff_ffff),
            instruction: AARCH64_STR_W0_TO_X1,
        },
        BlockQueueStepSpec {
            name: "queue_driver_high",
            access: BlockQueueAccessKind::Write,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_QUEUE_DRIVER_HIGH_OFFSET,
            expected_value: None,
            write_value: Some(VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS >> 32),
            instruction: AARCH64_STR_W0_TO_X1,
        },
        BlockQueueStepSpec {
            name: "queue_device_low",
            access: BlockQueueAccessKind::Write,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET,
            expected_value: None,
            write_value: Some(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS & 0xffff_ffff),
            instruction: AARCH64_STR_W0_TO_X1,
        },
        BlockQueueStepSpec {
            name: "queue_device_high",
            access: BlockQueueAccessKind::Write,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_QUEUE_DEVICE_HIGH_OFFSET,
            expected_value: None,
            write_value: Some(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS >> 32),
            instruction: AARCH64_STR_W0_TO_X1,
        },
        BlockQueueStepSpec {
            name: "queue_ready",
            access: BlockQueueAccessKind::Write,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_QUEUE_READY_OFFSET,
            expected_value: None,
            write_value: Some(VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE),
            instruction: AARCH64_STR_W0_TO_X1,
        },
        BlockQueueStepSpec {
            name: "status_driver_ok",
            access: BlockQueueAccessKind::Write,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_STATUS_OFFSET,
            expected_value: None,
            write_value: Some(VIRTIO_MMIO_BLOCK_STATUS_VALUE),
            instruction: AARCH64_STR_W0_TO_X1,
        },
        BlockQueueStepSpec {
            name: "status",
            access: BlockQueueAccessKind::Read,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_STATUS_OFFSET,
            expected_value: Some(VIRTIO_MMIO_BLOCK_STATUS_VALUE),
            write_value: None,
            instruction: AARCH64_LDR_W0_FROM_X1,
        },
        BlockQueueStepSpec {
            name: "queue_notify",
            access: BlockQueueAccessKind::Write,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET,
            expected_value: None,
            write_value: Some(VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE),
            instruction: AARCH64_STR_W0_TO_X1,
        },
        BlockQueueStepSpec {
            name: "queue_ready",
            access: BlockQueueAccessKind::Read,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_QUEUE_READY_OFFSET,
            expected_value: Some(VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE),
            write_value: None,
            instruction: AARCH64_LDR_W0_FROM_X1,
        },
        BlockQueueStepSpec {
            name: "queue_desc_low",
            access: BlockQueueAccessKind::Read,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET,
            expected_value: Some(VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS & 0xffff_ffff),
            write_value: None,
            instruction: AARCH64_LDR_W0_FROM_X1,
        },
        BlockQueueStepSpec {
            name: "queue_driver_low",
            access: BlockQueueAccessKind::Read,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET,
            expected_value: Some(VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS & 0xffff_ffff),
            write_value: None,
            instruction: AARCH64_LDR_W0_FROM_X1,
        },
        BlockQueueStepSpec {
            name: "queue_device_low",
            access: BlockQueueAccessKind::Read,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET,
            expected_value: Some(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS & 0xffff_ffff),
            write_value: None,
            instruction: AARCH64_LDR_W0_FROM_X1,
        },
        BlockQueueStepSpec {
            name: "interrupt_status",
            access: BlockQueueAccessKind::Read,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET,
            expected_value: Some(VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE),
            write_value: None,
            instruction: AARCH64_LDR_W0_FROM_X1,
        },
        BlockQueueStepSpec {
            name: "config_generation",
            access: BlockQueueAccessKind::Read,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_CONFIG_GENERATION_OFFSET,
            expected_value: Some(VIRTIO_MMIO_BLOCK_CONFIG_GENERATION_VALUE),
            write_value: None,
            instruction: AARCH64_LDR_W0_FROM_X1,
        },
        BlockQueueStepSpec {
            name: "capacity_low",
            access: BlockQueueAccessKind::Read,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_BLOCK_CAPACITY_LOW_OFFSET,
            expected_value: Some(VIRTIO_MMIO_BLOCK_CAPACITY_SECTORS & 0xffff_ffff),
            write_value: None,
            instruction: AARCH64_LDR_W0_FROM_X1,
        },
        BlockQueueStepSpec {
            name: "capacity_high",
            access: BlockQueueAccessKind::Read,
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_BLOCK_CAPACITY_HIGH_OFFSET,
            expected_value: Some(VIRTIO_MMIO_BLOCK_CAPACITY_SECTORS >> 32),
            write_value: None,
            instruction: AARCH64_LDR_W0_FROM_X1,
        },
    ]
}

fn block_queue_step_defaults() -> Vec<HvfMmioBlockQueueStepProbe> {
    block_queue_step_specs()
        .iter()
        .map(|spec| HvfMmioBlockQueueStepProbe {
            name: spec.name,
            access: spec.access.as_str(),
            ipa: spec.ipa,
            expected_value: spec.expected_value,
            write_value: spec.write_value,
            run_attempted: false,
            address_register_set: false,
            write_value_register_set: false,
            exit_observed: false,
            handled_by_device: false,
            value_injected: false,
            write_accepted: false,
            pc_read_after_exit: false,
            pc_advanced: false,
            captured_write_value: None,
            run_status: None,
            address_register_set_status: None,
            write_value_register_set_status: None,
            exit_reason: None,
            exit_syndrome: None,
            exit_virtual_address: None,
            exit_physical_address: None,
            watchdog_cancel_status: None,
            value_set_status: None,
            pc_read_status: None,
            pc_after_exit: None,
            pc_advance_status: None,
        })
        .collect()
}

pub fn probe_hvf_mmio_block_queue(
    allow_device: bool,
    disk_path: Option<PathBuf>,
    iso_path: Option<PathBuf>,
    writable_disk_path: Option<PathBuf>,
    host: HvfHostCapabilities,
) -> HvfMmioBlockQueueProbe {
    let mut blockers = Vec::new();
    let block_backing = if let Some(path) = writable_disk_path.as_ref() {
        VirtioBlockProbeBackingRef::HostFileWritable(path)
    } else if let Some(path) = iso_path.as_ref() {
        VirtioBlockProbeBackingRef::HostIsoReadOnly(path)
    } else if let Some(path) = disk_path.as_ref() {
        VirtioBlockProbeBackingRef::HostFile(path)
    } else {
        VirtioBlockProbeBackingRef::Synthetic
    };
    let block_backing_kind = block_backing.kind();
    let block_backing_path = block_backing.path().cloned();

    if !allow_device {
        blockers.push(
            "set BRIDGEVM_HVF_ALLOW_MMIO_BLOCK_QUEUE=1 or pass --allow-device to emulate VirtIO-MMIO block queue/config/address/notify registers through the MMIO bus".to_string(),
        );
        return mmio_block_queue_probe_result(
            false,
            false,
            host,
            block_backing_kind,
            block_backing_path,
            blockers,
        );
    }

    if !host.available {
        blockers.push("Hypervisor.framework host capabilities are not available".to_string());
        return mmio_block_queue_probe_result(
            true,
            false,
            host,
            block_backing_kind,
            block_backing_path,
            blockers,
        );
    }

    let specs = block_queue_step_specs();
    let mut steps = block_queue_step_defaults();
    let mut memory_allocated = false;
    let mut memory_mapped = false;
    let mut vcpu_created = false;
    let mut pc_set = false;
    let mut cpsr_set = false;
    let mut continuation_run_attempted = false;
    let mut continuation_exit_observed = false;
    let mut capacity_high_value_preserved = false;
    let mut vcpu_destroyed = false;
    let mut memory_unmapped = false;
    let mut vm_destroyed = false;
    let mut memory_deallocated = false;

    let mut allocate_status = None;
    let mut map_status = None;
    let mut vcpu_create_status = None;
    let mut pc_set_status = None;
    let mut cpsr_set_status = None;
    let mut continuation_run_status = None;
    let mut continuation_exit_reason = None;
    let mut continuation_exit_syndrome = None;
    let mut continuation_exit_virtual_address = None;
    let mut continuation_exit_physical_address = None;
    let mut continuation_watchdog_cancel_status = None;
    let mut capacity_high_after_continue_status = None;
    let mut capacity_high_after_continue = None;
    let mut request_ring_seeded = false;
    let mut request_completed_after_notify = false;
    let mut request_descriptor_index = None;
    let mut request_sector = None;
    let mut request_byte_offset = None;
    let mut request_data_bytes = None;
    let mut request_data_prefix = Vec::new();
    let mut request_status = None;
    let mut request_used_index = None;
    let mut request_used_len = None;
    let mut request_interrupt_status = None;
    let mut write_completed_after_notify = false;
    let mut write_request_type = None;
    let mut write_sector = None;
    let mut write_byte_offset = None;
    let mut write_data_bytes = None;
    let mut write_data_prefix = Vec::new();
    let mut write_status = None;
    let mut write_used_index = None;
    let mut write_used_len = None;
    let mut flush_completed_after_notify = false;
    let mut flush_request_type = None;
    let mut flush_status = None;
    let mut flush_used_index = None;
    let mut flush_used_len = None;
    let mut persisted_data_prefix = Vec::new();
    let mut vcpu_destroy_status = None;
    let mut unmap_status = None;
    let mut vm_destroy_status = None;
    let mut deallocate_status = None;

    let mut memory = ptr::null_mut();
    let mut vcpu = 0;
    let mut exit = ptr::null_mut();
    let mut mmio_bus = MmioBus::default();
    mmio_bus.attach(Box::new(Pl011UartDevice::new(
        PROBE_MMIO_IPA,
        SERIAL_MMIO_STATUS_VALUE,
    )));
    mmio_bus.attach(Box::new(Pl031RtcDevice::new(
        RTC_MMIO_IPA,
        RTC_MMIO_READ_VALUE,
    )));
    mmio_bus.attach(Box::new(VirtioMmioBlockDevice::new(BLOCK_MMIO_IPA)));
    let device_bus_created = true;
    let device_bus_device_count = mmio_bus.device_count();

    let status = unsafe { hv_vm_create(ptr::null_mut()) };
    let vm_create_status = Some(status);
    let vm_created = status == HV_SUCCESS;
    if !vm_created {
        blockers.push(format!("hv_vm_create failed: {status:#x}"));
    }

    if vm_created {
        let status = unsafe { hv_vm_allocate(&mut memory, PROBE_BYTES, HV_ALLOCATE_DEFAULT) };
        allocate_status = Some(status);
        memory_allocated = status == HV_SUCCESS && !memory.is_null();
        if memory_allocated {
            for (index, spec) in specs.iter().enumerate() {
                let instruction = spec.instruction.to_le_bytes();
                unsafe {
                    ptr::copy_nonoverlapping(
                        instruction.as_ptr(),
                        memory.cast::<u8>().add(index * instruction.len()),
                        instruction.len(),
                    );
                }
            }
            let hvc = AARCH64_HVC_0.to_le_bytes();
            unsafe {
                ptr::copy_nonoverlapping(
                    hvc.as_ptr(),
                    memory.cast::<u8>().add(specs.len() * hvc.len()),
                    hvc.len(),
                );
            }
            let seed_result: Result<(), VirtioBlockRequestError> = unsafe {
                let bytes = std::slice::from_raw_parts_mut(memory.cast::<u8>(), PROBE_BYTES);
                let mut guest_memory = VirtioGuestMemory::new(PROBE_IPA_START, bytes);
                seed_synthetic_virtio_block_read_request(&mut guest_memory)
            };
            match seed_result {
                Ok(()) => request_ring_seeded = true,
                Err(error) => blockers.push(format!(
                    "failed to seed synthetic VirtIO block request ring: {}",
                    error.render_blocker()
                )),
            }
        } else {
            blockers.push(format!("hv_vm_allocate failed: {status:#x}"));
        }
    }

    if vm_created && memory_allocated {
        let status = unsafe {
            hv_vm_map(
                memory,
                PROBE_IPA_START,
                PROBE_BYTES,
                HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC,
            )
        };
        map_status = Some(status);
        memory_mapped = status == HV_SUCCESS;
        if !memory_mapped {
            blockers.push(format!("hv_vm_map failed: {status:#x}"));
        }
    }

    if vm_created && memory_mapped {
        let status = unsafe { hv_vcpu_create(&mut vcpu, &mut exit, ptr::null_mut()) };
        vcpu_create_status = Some(status);
        vcpu_created = status == HV_SUCCESS;
        if !vcpu_created {
            blockers.push(format!("hv_vcpu_create failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, PROBE_IPA_START) };
        pc_set_status = Some(status);
        pc_set = status == HV_SUCCESS;
        if !pc_set {
            blockers.push(format!("hv_vcpu_set_reg(PC) failed: {status:#x}"));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_CPSR, AARCH64_PSTATE_EL1H_DAIF_MASKED) };
        cpsr_set_status = Some(status);
        cpsr_set = status == HV_SUCCESS;
        if !cpsr_set {
            blockers.push(format!("hv_vcpu_set_reg(CPSR) failed: {status:#x}"));
        }
    }

    let mut can_continue = vcpu_created && pc_set && cpsr_set;
    for (index, spec) in specs.iter().enumerate() {
        if !can_continue {
            break;
        }

        let step = &mut steps[index];
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X1, spec.ipa) };
        step.address_register_set_status = Some(status);
        step.address_register_set = status == HV_SUCCESS;
        if !step.address_register_set {
            blockers.push(format!(
                "hv_vcpu_set_reg(X1 {}) failed: {status:#x}",
                spec.name
            ));
            break;
        }

        if let Some(value) = spec.write_value {
            let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X0, value) };
            step.write_value_register_set_status = Some(status);
            step.write_value_register_set = status == HV_SUCCESS;
            if !step.write_value_register_set {
                blockers.push(format!(
                    "hv_vcpu_set_reg(X0 {}) failed: {status:#x}",
                    spec.name
                ));
                break;
            }
        }

        step.run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        step.run_status = Some(observation.run_status);
        step.exit_reason = observation.exit_reason;
        step.exit_syndrome = observation.exit_syndrome;
        step.exit_virtual_address = observation.exit_virtual_address;
        step.exit_physical_address = observation.exit_physical_address;
        step.watchdog_cancel_status = observation.watchdog_cancel_status;
        if step.watchdog_cancel_status.is_some() {
            blockers.push(format!(
                "VirtIO block queue/config {} {} watchdog fired before exception exit",
                spec.access.as_str(),
                spec.name
            ));
            can_continue = false;
        }

        if observation.run_status == HV_SUCCESS {
            if step.exit_reason.is_none() {
                blockers.push(format!(
                    "VirtIO block queue/config {} {} returned success without an exit info pointer",
                    spec.access.as_str(),
                    spec.name
                ));
                can_continue = false;
            } else {
                step.exit_observed = step.exit_reason == Some(HV_EXIT_REASON_EXCEPTION)
                    && (step.exit_virtual_address == Some(spec.ipa)
                        || step.exit_physical_address == Some(spec.ipa)
                        || step.exit_syndrome.is_some_and(is_data_abort_syndrome));
                if !step.exit_observed {
                    blockers.push(format!(
                        "VirtIO block queue/config {} {} did not exit at IPA {:#x}",
                        spec.access.as_str(),
                        spec.name,
                        spec.ipa
                    ));
                    can_continue = false;
                }
            }
        } else {
            blockers.push(format!(
                "VirtIO block queue/config {} {} hv_vcpu_run failed: {:#x}",
                spec.access.as_str(),
                spec.name,
                observation.run_status
            ));
            can_continue = false;
        }

        if step.exit_observed {
            match spec.access {
                BlockQueueAccessKind::Read => {
                    match mmio_bus.dispatch(MmioAccess::read(spec.ipa, 4)) {
                        MmioAction::ReadValue(value) => {
                            step.handled_by_device = true;
                            if Some(value) != spec.expected_value {
                                blockers.push(format!(
                                    "VirtIO block queue/config {} read returned unexpected value: {value:#x}",
                                    spec.name
                                ));
                                can_continue = false;
                            }
                            let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X0, value) };
                            step.value_set_status = Some(status);
                            step.value_injected = status == HV_SUCCESS;
                            if !step.value_injected {
                                blockers.push(format!(
                                    "hv_vcpu_set_reg(X0 read {}) failed: {status:#x}",
                                    spec.name
                                ));
                                can_continue = false;
                            }
                        }
                        MmioAction::Unhandled | MmioAction::WriteAccepted { .. } => {
                            blockers.push(format!(
                                "VirtIO block queue/config {} read was not handled by the MMIO device bus at {:#x}",
                                spec.name, spec.ipa
                            ));
                            can_continue = false;
                        }
                    }
                }
                BlockQueueAccessKind::Write => {
                    let value = spec.write_value.expect("write step has a seed value");
                    match mmio_bus.dispatch(MmioAccess::write(spec.ipa, value, 4)) {
                        MmioAction::WriteAccepted {
                            value: accepted_value,
                            ..
                        } => {
                            step.handled_by_device = true;
                            step.captured_write_value = Some(accepted_value);
                            step.write_accepted = accepted_value == value;
                            if !step.write_accepted {
                                blockers.push(format!(
                                    "VirtIO block queue/config {} write accepted unexpected value: {accepted_value:#x}",
                                    spec.name
                                ));
                                can_continue = false;
                            }
                            if spec.name == "queue_notify"
                                && step.write_accepted
                                && request_ring_seeded
                            {
                                match mmio_bus.find_device_mut::<VirtioMmioBlockDevice>() {
                                    Some(block) => {
                                        let completion_result: Result<
                                            VirtioBlockQueueProbeCompletion,
                                            VirtioBlockRequestError,
                                        > = unsafe {
                                            let bytes = std::slice::from_raw_parts_mut(
                                                memory.cast::<u8>(),
                                                PROBE_BYTES,
                                            );
                                            let mut guest_memory =
                                                VirtioGuestMemory::new(PROBE_IPA_START, bytes);
                                            match block_backing {
                                                VirtioBlockProbeBackingRef::HostFileWritable(
                                                    path,
                                                ) => {
                                                    complete_probe_virtio_block_writable_file_requests(
                                                        block,
                                                        &mut guest_memory,
                                                        path,
                                                    )
                                                    .map(VirtioBlockQueueProbeCompletion::Writable)
                                                }
                                                _ => complete_probe_virtio_block_request(
                                                    block,
                                                    &mut guest_memory,
                                                    block_backing,
                                                )
                                                .map(VirtioBlockQueueProbeCompletion::ReadOnly),
                                            }
                                        };
                                        match completion_result {
                                            Ok(VirtioBlockQueueProbeCompletion::ReadOnly(
                                                probe_completion,
                                            )) => {
                                                request_completed_after_notify = true;
                                                request_descriptor_index = Some(
                                                    probe_completion.completion.descriptor_index,
                                                );
                                                request_sector =
                                                    Some(probe_completion.completion.sector);
                                                request_byte_offset =
                                                    Some(probe_completion.byte_offset);
                                                request_data_bytes =
                                                    Some(probe_completion.completion.data_bytes);
                                                request_data_prefix = probe_completion.data_prefix;
                                                request_status = Some(probe_completion.status);
                                                request_used_index =
                                                    Some(probe_completion.completion.used_index);
                                                request_used_len = Some(probe_completion.used_len);
                                                request_interrupt_status = Some(
                                                    probe_completion.completion.interrupt_status,
                                                );
                                            }
                                            Ok(VirtioBlockQueueProbeCompletion::Writable(
                                                probe_completion,
                                            )) => {
                                                let initial_read = probe_completion.initial_read;
                                                request_completed_after_notify = true;
                                                request_descriptor_index =
                                                    Some(initial_read.completion.descriptor_index);
                                                request_sector =
                                                    Some(initial_read.completion.sector);
                                                request_byte_offset =
                                                    Some(initial_read.byte_offset);
                                                request_data_bytes =
                                                    Some(initial_read.completion.data_bytes);
                                                request_data_prefix = initial_read.data_prefix;
                                                request_status = Some(initial_read.status);
                                                request_used_index =
                                                    Some(initial_read.completion.used_index);
                                                request_used_len = Some(initial_read.used_len);
                                                request_interrupt_status =
                                                    Some(initial_read.completion.interrupt_status);

                                                write_completed_after_notify = true;
                                                write_request_type = Some(
                                                    probe_completion.write_completion.request_type,
                                                );
                                                write_sector =
                                                    Some(probe_completion.write_completion.sector);
                                                write_byte_offset =
                                                    Some(probe_completion.write_byte_offset);
                                                write_data_bytes = Some(
                                                    probe_completion.write_completion.data_bytes,
                                                );
                                                write_data_prefix =
                                                    probe_completion.write_data_prefix;
                                                write_status = Some(probe_completion.write_status);
                                                write_used_index = Some(
                                                    probe_completion.write_completion.used_index,
                                                );
                                                write_used_len =
                                                    Some(probe_completion.write_used_len);

                                                flush_completed_after_notify = true;
                                                flush_request_type = Some(
                                                    probe_completion.flush_completion.request_type,
                                                );
                                                flush_status = Some(probe_completion.flush_status);
                                                flush_used_index = Some(
                                                    probe_completion.flush_completion.used_index,
                                                );
                                                flush_used_len =
                                                    Some(probe_completion.flush_used_len);
                                                persisted_data_prefix =
                                                    probe_completion.persisted_data_prefix;
                                            }
                                            Err(error) => {
                                                blockers.push(format!(
                                                    "VirtIO block request completion after queue_notify failed: {}",
                                                    error.render_blocker()
                                                ));
                                                can_continue = false;
                                            }
                                        }
                                    }
                                    None => {
                                        blockers.push(
                                            "VirtIO block request completion after queue_notify could not find the block device on the MMIO bus"
                                                .to_string(),
                                        );
                                        can_continue = false;
                                    }
                                }
                            } else if spec.name == "queue_notify" && step.write_accepted {
                                blockers.push(
                                    "VirtIO block request ring was not seeded before queue_notify"
                                        .to_string(),
                                );
                                can_continue = false;
                            }
                        }
                        MmioAction::ReadValue(_) | MmioAction::Unhandled => {
                            blockers.push(format!(
                                "VirtIO block queue/config {} write was not handled by the MMIO device bus at {:#x}",
                                spec.name, spec.ipa
                            ));
                            can_continue = false;
                        }
                    }
                }
            }
        }

        if step.exit_observed {
            let mut pc = 0;
            let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc) };
            step.pc_read_status = Some(status);
            step.pc_read_after_exit = status == HV_SUCCESS;
            if step.pc_read_after_exit {
                step.pc_after_exit = Some(pc);
            } else {
                blockers.push(format!(
                    "hv_vcpu_get_reg(PC after VirtIO block queue/config {} {}) failed: {status:#x}",
                    spec.access.as_str(),
                    spec.name
                ));
                can_continue = false;
            }
        }

        let step_completed = match spec.access {
            BlockQueueAccessKind::Read => step.value_injected,
            BlockQueueAccessKind::Write => step.write_accepted,
        };
        if step.pc_read_after_exit && step_completed {
            let next_pc = PROBE_IPA_START + ((index as u64 + 1) * 4);
            let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, next_pc) };
            step.pc_advance_status = Some(status);
            step.pc_advanced = status == HV_SUCCESS;
            if !step.pc_advanced {
                blockers.push(format!(
                    "hv_vcpu_set_reg(PC after VirtIO block queue/config {} {}) failed: {status:#x}",
                    spec.access.as_str(),
                    spec.name
                ));
                can_continue = false;
            }
        }
    }

    if steps.iter().all(|step| step.pc_advanced) {
        continuation_run_attempted = true;
        let observation = run_vcpu_once_with_watchdog(vcpu, exit);
        continuation_run_status = Some(observation.run_status);
        continuation_exit_reason = observation.exit_reason;
        continuation_exit_syndrome = observation.exit_syndrome;
        continuation_exit_virtual_address = observation.exit_virtual_address;
        continuation_exit_physical_address = observation.exit_physical_address;
        continuation_watchdog_cancel_status = observation.watchdog_cancel_status;
        if continuation_watchdog_cancel_status.is_some() {
            blockers.push(
                "VirtIO block queue/config continuation watchdog fired before HVC exit".to_string(),
            );
        }

        if observation.run_status == HV_SUCCESS {
            if continuation_exit_reason.is_none() {
                blockers.push(
                    "VirtIO block queue/config continuation returned success without an exit info pointer"
                        .to_string(),
                );
            } else {
                continuation_exit_observed = continuation_exit_reason
                    == Some(HV_EXIT_REASON_EXCEPTION)
                    && continuation_exit_syndrome == Some(AARCH64_HVC_0_SYNDROME);
                if !continuation_exit_observed {
                    blockers.push(format!(
                        "VirtIO block queue/config continuation did not reach HVC; syndrome: {}",
                        continuation_exit_syndrome.map_or_else(
                            || "not observed".to_string(),
                            |value| format!("{value:#x}")
                        )
                    ));
                }
            }
        } else {
            blockers.push(format!(
                "VirtIO block queue/config continuation hv_vcpu_run failed: {:#x}",
                observation.run_status
            ));
        }
    }

    if continuation_exit_observed {
        let mut value = 0;
        let status = unsafe { hv_vcpu_get_reg(vcpu, HV_REG_X0, &mut value) };
        capacity_high_after_continue_status = Some(status);
        if status == HV_SUCCESS {
            capacity_high_after_continue = Some(value);
            capacity_high_value_preserved = value == (VIRTIO_MMIO_BLOCK_CAPACITY_SECTORS >> 32);
            if !capacity_high_value_preserved {
                blockers.push(format!(
                    "VirtIO block capacity high value changed before continuation HVC: {value:#x}"
                ));
            }
        } else {
            blockers.push(format!(
                "hv_vcpu_get_reg(X0 after VirtIO block queue/config continue) failed: {status:#x}"
            ));
        }
    }

    if vcpu_created {
        let status = unsafe { hv_vcpu_destroy(vcpu) };
        vcpu_destroy_status = Some(status);
        vcpu_destroyed = status == HV_SUCCESS;
        if !vcpu_destroyed {
            blockers.push(format!("hv_vcpu_destroy failed: {status:#x}"));
        }
    }

    if memory_mapped {
        let status = unsafe { hv_vm_unmap(PROBE_IPA_START, PROBE_BYTES) };
        unmap_status = Some(status);
        memory_unmapped = status == HV_SUCCESS;
        if !memory_unmapped {
            blockers.push(format!("hv_vm_unmap failed: {status:#x}"));
        }
    }

    if vm_created {
        let status = unsafe { hv_vm_destroy() };
        vm_destroy_status = Some(status);
        vm_destroyed = status == HV_SUCCESS;
        if !vm_destroyed {
            blockers.push(format!("hv_vm_destroy failed: {status:#x}"));
        }
    }

    if memory_allocated {
        let status = unsafe { hv_vm_deallocate(memory, PROBE_BYTES) };
        deallocate_status = Some(status);
        memory_deallocated = status == HV_SUCCESS;
        if !memory_deallocated {
            blockers.push(format!("hv_vm_deallocate failed: {status:#x}"));
        }
    }

    let watchdog_cancel_fired = steps
        .iter()
        .any(|step| step.watchdog_cancel_status.is_some())
        || continuation_watchdog_cancel_status.is_some();

    HvfMmioBlockQueueProbe {
        allowed: true,
        attempted: true,
        vm_created,
        memory_allocated,
        memory_mapped,
        vcpu_created,
        pc_set,
        cpsr_set,
        device_bus_created,
        device_bus_device_count,
        steps,
        continuation_run_attempted,
        continuation_exit_observed,
        capacity_high_value_preserved,
        watchdog_cancel_fired,
        vcpu_destroyed,
        memory_unmapped,
        vm_destroyed,
        memory_deallocated,
        host,
        device_models: BLOCK_QUEUE_MMIO_DEVICE_MODELS,
        code_ipa_start: PROBE_IPA_START,
        block_ipa: BLOCK_MMIO_IPA,
        bytes: PROBE_BYTES,
        instructions: "LDR/STR W0 VirtIO-MMIO queue/config/address/notify registers; HVC #0",
        device_features_value: VIRTIO_MMIO_BLOCK_DEVICE_FEATURES_VALUE,
        driver_features_value: VIRTIO_MMIO_BLOCK_DRIVER_FEATURES_VALUE,
        queue_select_value: VIRTIO_MMIO_BLOCK_QUEUE_SEL_VALUE,
        queue_num_max_value: VIRTIO_MMIO_BLOCK_QUEUE_NUM_MAX_VALUE,
        queue_num_value: VIRTIO_MMIO_BLOCK_QUEUE_NUM_VALUE,
        queue_ready_value: VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE,
        queue_desc_address: VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS,
        queue_driver_address: VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS,
        queue_device_address: VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS,
        queue_notify_value: VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
        interrupt_status_value: VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE,
        block_backing_kind,
        block_backing_path,
        request_ring_seeded,
        request_completed_after_notify,
        request_descriptor_index,
        request_sector,
        request_byte_offset,
        request_data_bytes,
        request_data_prefix,
        request_status,
        request_used_index,
        request_used_len,
        request_interrupt_status,
        write_completed_after_notify,
        write_request_type,
        write_sector,
        write_byte_offset,
        write_data_bytes,
        write_data_prefix,
        write_status,
        write_used_index,
        write_used_len,
        flush_completed_after_notify,
        flush_request_type,
        flush_status,
        flush_used_index,
        flush_used_len,
        persisted_data_prefix,
        status_value: VIRTIO_MMIO_BLOCK_STATUS_VALUE,
        capacity_sectors: VIRTIO_MMIO_BLOCK_CAPACITY_SECTORS,
        vm_create_status,
        allocate_status,
        map_status,
        vcpu_create_status,
        pc_set_status,
        cpsr_set_status,
        continuation_run_status,
        continuation_exit_reason,
        continuation_exit_syndrome,
        continuation_exit_virtual_address,
        continuation_exit_physical_address,
        continuation_watchdog_cancel_status,
        capacity_high_after_continue_status,
        capacity_high_after_continue,
        vcpu_destroy_status,
        unmap_status,
        vm_destroy_status,
        deallocate_status,
        blockers,
    }
}

fn mmio_block_queue_probe_result(
    allowed: bool,
    attempted: bool,
    host: HvfHostCapabilities,
    block_backing_kind: &'static str,
    block_backing_path: Option<PathBuf>,
    blockers: Vec<String>,
) -> HvfMmioBlockQueueProbe {
    HvfMmioBlockQueueProbe {
        allowed,
        attempted,
        vm_created: false,
        memory_allocated: false,
        memory_mapped: false,
        vcpu_created: false,
        pc_set: false,
        cpsr_set: false,
        device_bus_created: false,
        device_bus_device_count: 0,
        steps: block_queue_step_defaults(),
        continuation_run_attempted: false,
        continuation_exit_observed: false,
        capacity_high_value_preserved: false,
        watchdog_cancel_fired: false,
        vcpu_destroyed: false,
        memory_unmapped: false,
        vm_destroyed: false,
        memory_deallocated: false,
        host,
        device_models: BLOCK_QUEUE_MMIO_DEVICE_MODELS,
        code_ipa_start: PROBE_IPA_START,
        block_ipa: BLOCK_MMIO_IPA,
        bytes: PROBE_BYTES,
        instructions: "LDR/STR W0 VirtIO-MMIO queue/config/address/notify registers; HVC #0",
        device_features_value: VIRTIO_MMIO_BLOCK_DEVICE_FEATURES_VALUE,
        driver_features_value: VIRTIO_MMIO_BLOCK_DRIVER_FEATURES_VALUE,
        queue_select_value: VIRTIO_MMIO_BLOCK_QUEUE_SEL_VALUE,
        queue_num_max_value: VIRTIO_MMIO_BLOCK_QUEUE_NUM_MAX_VALUE,
        queue_num_value: VIRTIO_MMIO_BLOCK_QUEUE_NUM_VALUE,
        queue_ready_value: VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE,
        queue_desc_address: VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS,
        queue_driver_address: VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS,
        queue_device_address: VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS,
        queue_notify_value: VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
        interrupt_status_value: VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE,
        block_backing_kind,
        block_backing_path,
        request_ring_seeded: false,
        request_completed_after_notify: false,
        request_descriptor_index: None,
        request_sector: None,
        request_byte_offset: None,
        request_data_bytes: None,
        request_data_prefix: Vec::new(),
        request_status: None,
        request_used_index: None,
        request_used_len: None,
        request_interrupt_status: None,
        write_completed_after_notify: false,
        write_request_type: None,
        write_sector: None,
        write_byte_offset: None,
        write_data_bytes: None,
        write_data_prefix: Vec::new(),
        write_status: None,
        write_used_index: None,
        write_used_len: None,
        flush_completed_after_notify: false,
        flush_request_type: None,
        flush_status: None,
        flush_used_index: None,
        flush_used_len: None,
        persisted_data_prefix: Vec::new(),
        status_value: VIRTIO_MMIO_BLOCK_STATUS_VALUE,
        capacity_sectors: VIRTIO_MMIO_BLOCK_CAPACITY_SECTORS,
        vm_create_status: None,
        allocate_status: None,
        map_status: None,
        vcpu_create_status: None,
        pc_set_status: None,
        cpsr_set_status: None,
        continuation_run_status: None,
        continuation_exit_reason: None,
        continuation_exit_syndrome: None,
        continuation_exit_virtual_address: None,
        continuation_exit_physical_address: None,
        continuation_watchdog_cancel_status: None,
        capacity_high_after_continue_status: None,
        capacity_high_after_continue: None,
        vcpu_destroy_status: None,
        unmap_status: None,
        vm_destroy_status: None,
        deallocate_status: None,
        blockers,
    }
}

fn mmio_block_device_probe_result(
    allowed: bool,
    attempted: bool,
    host: HvfHostCapabilities,
    blockers: Vec<String>,
) -> HvfMmioBlockDeviceProbe {
    HvfMmioBlockDeviceProbe {
        allowed,
        attempted,
        vm_created: false,
        memory_allocated: false,
        memory_mapped: false,
        vcpu_created: false,
        pc_set: false,
        cpsr_set: false,
        register_address_registers_set: false,
        device_bus_created: false,
        device_bus_device_count: 0,
        register_reads: block_register_probe_defaults(),
        continuation_run_attempted: false,
        continuation_exit_observed: false,
        vendor_value_preserved: false,
        watchdog_cancel_fired: false,
        vcpu_destroyed: false,
        memory_unmapped: false,
        vm_destroyed: false,
        memory_deallocated: false,
        host,
        device_models: BOOT_MMIO_DEVICE_MODELS,
        code_ipa_start: PROBE_IPA_START,
        block_ipa: BLOCK_MMIO_IPA,
        bytes: PROBE_BYTES,
        instructions: "LDR W0 magic/version/device/vendor; HVC #0",
        magic_value: VIRTIO_MMIO_MAGIC_VALUE,
        version_value: VIRTIO_MMIO_VERSION_VALUE,
        device_id_value: VIRTIO_MMIO_BLOCK_DEVICE_ID_VALUE,
        vendor_id_value: VIRTIO_MMIO_VENDOR_ID_VALUE,
        vm_create_status: None,
        allocate_status: None,
        map_status: None,
        vcpu_create_status: None,
        pc_set_status: None,
        cpsr_set_status: None,
        register_address_registers_set_status: vec![None; 4],
        continuation_run_status: None,
        continuation_exit_reason: None,
        continuation_exit_syndrome: None,
        continuation_exit_virtual_address: None,
        continuation_exit_physical_address: None,
        continuation_watchdog_cancel_status: None,
        vendor_value_after_continue_status: None,
        vendor_value_after_continue: None,
        vcpu_destroy_status: None,
        unmap_status: None,
        vm_destroy_status: None,
        deallocate_status: None,
        blockers,
    }
}
