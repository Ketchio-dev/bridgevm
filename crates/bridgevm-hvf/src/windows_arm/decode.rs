//! Split out of windows_arm.rs by responsibility.

use super::*;
use crate::*;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_firmware_run_loop_exit() -> WindowsArmUefiFirmwareRunLoopExit {
        WindowsArmUefiFirmwareRunLoopExit {
            index: 1,
            run_status: None,
            exit_reason: None,
            exit_syndrome: None,
            exit_exception_class: None,
            exit_virtual_address: None,
            exit_physical_address: None,
            pc_after_exit_status: None,
            pc_after_exit: None,
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
        }
    }

    fn sysreg_trap_syndrome(
        is_read: bool,
        register: u8,
        op0: u8,
        op1: u8,
        crn: u8,
        crm: u8,
        op2: u8,
    ) -> u64 {
        (AARCH64_SYSREG_TRAP_EXCEPTION_CLASS << 26)
            | (u64::from(op0) << 20)
            | (u64::from(op2) << 17)
            | (u64::from(op1) << 14)
            | (u64::from(crn) << 10)
            | (u64::from(register) << 5)
            | (u64::from(crm) << 1)
            | u64::from(is_read as u8)
    }

    #[test]
    fn post_repair_unhandled_access_telemetry_records_decode_metadata() {
        let installer_iso = PathBuf::from("/tmp/Win11_Arm64.iso");
        let block_devices = windows_arm_firmware_block_devices(Some(installer_iso), None);
        let virtio_exit = WindowsArmUefiFirmwareRunLoopExit {
            index: 11,
            run_status: Some(HV_SUCCESS_VALUE),
            exit_reason: Some(HV_EXIT_REASON_EXCEPTION_VALUE),
            exit_syndrome: Some(0x93c0_8006),
            exit_physical_address: Some(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + 0x10),
            pc_after_exit: Some(0x800_6789),
            ..test_firmware_run_loop_exit()
        };
        let mmio_access = decode_mmio_data_abort(virtio_exit.exit_syndrome.unwrap()).unwrap();

        let mut telemetry = LowVectorPostRepairTelemetry::default();
        telemetry.observe_unhandled_mmio_access(
            &block_devices,
            &virtio_exit,
            mmio_access,
            WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + 0x10,
            None,
            "device-bus-unhandled-read",
        );

        assert!(telemetry.first_unhandled_access.observed);
        assert_eq!(telemetry.first_unhandled_access.index, Some(11));
        assert_eq!(telemetry.first_unhandled_access.kind, "mmio");
        assert_eq!(telemetry.first_unhandled_access.access, "read");
        assert_eq!(
            telemetry.first_unhandled_access.mmio_ipa,
            Some(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + 0x10)
        );
        assert_eq!(telemetry.first_unhandled_access.mmio_width, Some(8));
        assert_eq!(
            telemetry.first_unhandled_access.mmio_device_kind,
            "virtio-installer-iso"
        );
        assert_eq!(
            telemetry.first_unhandled_access.handler_result,
            "device-bus-unhandled-read"
        );

        let mut output = String::new();
        append_low_vector_post_repair_unhandled_access_telemetry(
            &mut output,
            "Post-repair first unhandled access",
            &telemetry.first_unhandled_access,
        );
        assert!(output.contains("Post-repair first unhandled access observed: true"));
        assert!(output.contains("Post-repair first unhandled access: 11"));
        assert!(output.contains("Post-repair first unhandled access kind: mmio"));
        assert!(output.contains("Post-repair first unhandled access direction: read"));
        assert!(output.contains("Post-repair first unhandled access MMIO IPA: 0x10002010"));
        assert!(output
            .contains("Post-repair first unhandled access MMIO device kind: virtio-installer-iso"));

        let sysreg_exit = WindowsArmUefiFirmwareRunLoopExit {
            index: 12,
            run_status: Some(HV_SUCCESS_VALUE),
            exit_reason: Some(HV_EXIT_REASON_EXCEPTION_VALUE),
            exit_syndrome: Some(sysreg_trap_syndrome(true, 2, 3, 0, 12, 12, 0)),
            pc_after_exit: Some(0x800_9876),
            ..test_firmware_run_loop_exit()
        };
        let sysreg_access =
            decode_system_register_trap(sysreg_exit.exit_syndrome.unwrap()).unwrap();
        let mut telemetry = LowVectorPostRepairTelemetry::default();
        telemetry.observe_unhandled_sysreg_access(
            &sysreg_exit,
            sysreg_access,
            None,
            "sysreg-unhandled",
        );

        assert!(telemetry.first_unhandled_access.observed);
        assert_eq!(telemetry.first_unhandled_access.kind, "icc-sysreg");
        assert_eq!(telemetry.first_unhandled_access.access, "read");
        assert_eq!(
            telemetry.first_unhandled_access.sysreg,
            Some(ICC_IAR1_EL1_SYSREG)
        );
        assert_eq!(telemetry.first_unhandled_access.sysreg_name, "ICC_IAR1_EL1");
        assert_eq!(
            telemetry.first_unhandled_access.handler_result,
            "sysreg-unhandled"
        );
    }

    #[test]
    fn firmware_mmio_data_abort_decoder_handles_aarch64_loads_and_stores() {
        let read = decode_mmio_data_abort(0x93c0_8006).expect("read data abort decodes");
        assert!(!read.is_write);
        assert_eq!(read.access_name(), "read");
        assert_eq!(read.register, 0);
        assert_eq!(read.width, 8);

        let write = decode_mmio_data_abort(0x93c0_8046).expect("write data abort decodes");
        assert!(write.is_write);
        assert_eq!(write.access_name(), "write");
        assert_eq!(write.register, 0);
        assert_eq!(write.width, 8);

        assert_eq!(decode_mmio_data_abort(0x92c0_8006), None);
        assert_eq!(decode_mmio_data_abort(0x93df_8006), None);
    }
}
