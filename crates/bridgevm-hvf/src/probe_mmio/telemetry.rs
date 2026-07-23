//! Split out of probe_mmio.rs by responsibility.

use super::*;
use crate::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LowVectorPostRepairAccessTelemetry {
    pub(crate) kind: &'static str,
    pub(crate) direction: &'static str,
    pub(crate) address: Option<u64>,
    pub(crate) sysreg: Option<u16>,
    pub(crate) syndrome: Option<u64>,
}

impl Default for LowVectorPostRepairAccessTelemetry {
    fn default() -> Self {
        Self {
            kind: "not observed",
            direction: "not observed",
            address: None,
            sysreg: None,
            syndrome: None,
        }
    }
}

impl LowVectorPostRepairAccessTelemetry {
    pub(crate) fn observed(exit: &WindowsArmUefiFirmwareRunLoopExit) -> Self {
        let Some(syndrome) = exit.exit_syndrome else {
            return Self {
                kind: "not applicable",
                direction: "not applicable",
                ..Self::default()
            };
        };

        if let Some(access) = decode_mmio_data_abort(syndrome) {
            return Self {
                kind: "mmio",
                direction: access.access_name(),
                address: exit.exit_physical_address.or(exit.exit_virtual_address),
                sysreg: None,
                syndrome: Some(syndrome),
            };
        }

        if let Some(access) = decode_system_register_trap(syndrome) {
            return Self {
                kind: windows_arm_firmware_post_repair_sysreg_access_kind(access.sys_reg),
                direction: access.access_name(),
                address: None,
                sysreg: Some(access.sys_reg),
                syndrome: Some(syndrome),
            };
        }

        Self {
            kind: "exception",
            direction: "not applicable",
            address: None,
            sysreg: None,
            syndrome: Some(syndrome),
        }
    }
}

pub(crate) fn windows_arm_firmware_post_repair_sysreg_access_kind(sys_reg: u16) -> &'static str {
    if windows_arm_firmware_is_icc_sysreg(sys_reg) {
        "icc-sysreg"
    } else {
        "sysreg"
    }
}

pub(crate) fn windows_arm_firmware_is_icc_sysreg(sys_reg: u16) -> bool {
    matches!(
        sys_reg,
        ICC_PMR_EL1_SYSREG
            | ICC_IAR0_EL1_SYSREG
            | ICC_EOIR0_EL1_SYSREG
            | ICC_HPPIR0_EL1_SYSREG
            | ICC_BPR0_EL1_SYSREG
            | ICC_AP0R0_EL1_SYSREG
            | ICC_AP0R1_EL1_SYSREG
            | ICC_AP0R2_EL1_SYSREG
            | ICC_AP0R3_EL1_SYSREG
            | ICC_AP1R0_EL1_SYSREG
            | ICC_AP1R1_EL1_SYSREG
            | ICC_AP1R2_EL1_SYSREG
            | ICC_AP1R3_EL1_SYSREG
            | ICC_DIR_EL1_SYSREG
            | ICC_RPR_EL1_SYSREG
            | ICC_SGI1R_EL1_SYSREG
            | ICC_IAR1_EL1_SYSREG
            | ICC_EOIR1_EL1_SYSREG
            | ICC_HPPIR1_EL1_SYSREG
            | ICC_BPR1_EL1_SYSREG
            | ICC_CTLR_EL1_SYSREG
            | ICC_SRE_EL1_SYSREG
            | ICC_IGRPEN0_EL1_SYSREG
            | ICC_IGRPEN1_EL1_SYSREG
    )
}

pub(crate) fn windows_arm_firmware_sysreg_name(sys_reg: u16) -> &'static str {
    match sys_reg {
        ICC_PMR_EL1_SYSREG => "ICC_PMR_EL1",
        ICC_IAR0_EL1_SYSREG => "ICC_IAR0_EL1",
        ICC_EOIR0_EL1_SYSREG => "ICC_EOIR0_EL1",
        ICC_HPPIR0_EL1_SYSREG => "ICC_HPPIR0_EL1",
        ICC_BPR0_EL1_SYSREG => "ICC_BPR0_EL1",
        ICC_AP0R0_EL1_SYSREG => "ICC_AP0R0_EL1",
        ICC_AP0R1_EL1_SYSREG => "ICC_AP0R1_EL1",
        ICC_AP0R2_EL1_SYSREG => "ICC_AP0R2_EL1",
        ICC_AP0R3_EL1_SYSREG => "ICC_AP0R3_EL1",
        ICC_AP1R0_EL1_SYSREG => "ICC_AP1R0_EL1",
        ICC_AP1R1_EL1_SYSREG => "ICC_AP1R1_EL1",
        ICC_AP1R2_EL1_SYSREG => "ICC_AP1R2_EL1",
        ICC_AP1R3_EL1_SYSREG => "ICC_AP1R3_EL1",
        ICC_DIR_EL1_SYSREG => "ICC_DIR_EL1",
        ICC_RPR_EL1_SYSREG => "ICC_RPR_EL1",
        ICC_SGI1R_EL1_SYSREG => "ICC_SGI1R_EL1",
        ICC_IAR1_EL1_SYSREG => "ICC_IAR1_EL1",
        ICC_EOIR1_EL1_SYSREG => "ICC_EOIR1_EL1",
        ICC_HPPIR1_EL1_SYSREG => "ICC_HPPIR1_EL1",
        ICC_BPR1_EL1_SYSREG => "ICC_BPR1_EL1",
        ICC_CTLR_EL1_SYSREG => "ICC_CTLR_EL1",
        ICC_SRE_EL1_SYSREG => "ICC_SRE_EL1",
        ICC_IGRPEN0_EL1_SYSREG => "ICC_IGRPEN0_EL1",
        ICC_IGRPEN1_EL1_SYSREG => "ICC_IGRPEN1_EL1",
        _ => "unknown",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LowVectorPostRepairUnhandledAccessTelemetry {
    pub(crate) observed: bool,
    pub(crate) index: Option<u32>,
    pub(crate) reason: Option<u32>,
    pub(crate) diagnosis: &'static str,
    pub(crate) pc: Option<u64>,
    pub(crate) syndrome: Option<u64>,
    pub(crate) kind: &'static str,
    pub(crate) access: &'static str,
    pub(crate) register: Option<u8>,
    pub(crate) value: Option<u64>,
    pub(crate) handler_result: &'static str,
    pub(crate) mmio_ipa: Option<u64>,
    pub(crate) mmio_width: Option<u8>,
    pub(crate) mmio_device_kind: &'static str,
    pub(crate) sysreg: Option<u16>,
    pub(crate) sysreg_name: &'static str,
    pub(crate) sysreg_op0: Option<u8>,
    pub(crate) sysreg_op1: Option<u8>,
    pub(crate) sysreg_crn: Option<u8>,
    pub(crate) sysreg_crm: Option<u8>,
    pub(crate) sysreg_op2: Option<u8>,
}

impl Default for LowVectorPostRepairUnhandledAccessTelemetry {
    fn default() -> Self {
        Self {
            observed: false,
            index: None,
            reason: None,
            diagnosis: "not observed",
            pc: None,
            syndrome: None,
            kind: "not observed",
            access: "not observed",
            register: None,
            value: None,
            handler_result: "not observed",
            mmio_ipa: None,
            mmio_width: None,
            mmio_device_kind: "not observed",
            sysreg: None,
            sysreg_name: "not observed",
            sysreg_op0: None,
            sysreg_op1: None,
            sysreg_crn: None,
            sysreg_crm: None,
            sysreg_op2: None,
        }
    }
}

impl LowVectorPostRepairUnhandledAccessTelemetry {
    pub(crate) fn mmio(
        block_devices: &[WindowsArmVirtioBlockDeviceMetadata],
        exit: &WindowsArmUefiFirmwareRunLoopExit,
        access: DecodedMmioDataAbort,
        ipa: u64,
        value: Option<u64>,
        handler_result: &'static str,
    ) -> Self {
        Self {
            observed: true,
            index: Some(exit.index),
            reason: exit.exit_reason,
            diagnosis: windows_arm_firmware_run_loop_exit_diagnosis(exit),
            pc: exit.pc_after_exit,
            syndrome: exit.exit_syndrome,
            kind: "mmio",
            access: access.access_name(),
            register: Some(access.register),
            value,
            handler_result,
            mmio_ipa: Some(ipa),
            mmio_width: Some(access.width),
            mmio_device_kind: windows_arm_firmware_mmio_device_kind_label(
                windows_arm_firmware_mmio_device_kind(block_devices, ipa),
            ),
            sysreg: None,
            sysreg_name: "not observed",
            sysreg_op0: None,
            sysreg_op1: None,
            sysreg_crn: None,
            sysreg_crm: None,
            sysreg_op2: None,
        }
    }

    pub(crate) fn sysreg(
        exit: &WindowsArmUefiFirmwareRunLoopExit,
        access: DecodedSystemRegisterAccess,
        value: Option<u64>,
        handler_result: &'static str,
    ) -> Self {
        Self {
            observed: true,
            index: Some(exit.index),
            reason: exit.exit_reason,
            diagnosis: windows_arm_firmware_run_loop_exit_diagnosis(exit),
            pc: exit.pc_after_exit,
            syndrome: exit.exit_syndrome,
            kind: windows_arm_firmware_post_repair_sysreg_access_kind(access.sys_reg),
            access: access.access_name(),
            register: Some(access.register),
            value,
            handler_result,
            mmio_ipa: None,
            mmio_width: None,
            mmio_device_kind: "not observed",
            sysreg: Some(access.sys_reg),
            sysreg_name: windows_arm_firmware_sysreg_name(access.sys_reg),
            sysreg_op0: Some(access.op0),
            sysreg_op1: Some(access.op1),
            sysreg_crn: Some(access.crn),
            sysreg_crm: Some(access.crm),
            sysreg_op2: Some(access.op2),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LowVectorPostRepairExitTelemetry {
    pub(crate) observed: bool,
    pub(crate) index: Option<u32>,
    pub(crate) reason: Option<u32>,
    pub(crate) diagnosis: &'static str,
    pub(crate) pc: Option<u64>,
    pub(crate) interaction_kind: &'static str,
    pub(crate) access: LowVectorPostRepairAccessTelemetry,
}

impl Default for LowVectorPostRepairExitTelemetry {
    fn default() -> Self {
        Self {
            observed: false,
            index: None,
            reason: None,
            diagnosis: "not observed",
            pc: None,
            interaction_kind: "not observed",
            access: LowVectorPostRepairAccessTelemetry::default(),
        }
    }
}

impl LowVectorPostRepairExitTelemetry {
    pub(crate) fn observed(
        block_devices: &[WindowsArmVirtioBlockDeviceMetadata],
        exit: &WindowsArmUefiFirmwareRunLoopExit,
    ) -> Self {
        Self {
            observed: true,
            index: Some(exit.index),
            reason: exit.exit_reason,
            diagnosis: windows_arm_firmware_run_loop_exit_diagnosis(exit),
            pc: exit.pc_after_exit,
            interaction_kind: windows_arm_firmware_post_repair_interaction_kind(
                block_devices,
                exit,
            ),
            access: LowVectorPostRepairAccessTelemetry::observed(exit),
        }
    }

    pub(crate) fn is_device_interaction(&self) -> bool {
        windows_arm_firmware_post_repair_is_device_interaction(self.interaction_kind)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LowVectorPostRepairTelemetry {
    pub(crate) continue_attempted: bool,
    pub(crate) unsupported_exit_observed: bool,
    pub(crate) unsupported_exit_reason: Option<u32>,
    pub(crate) unsupported_exit_diagnosis: &'static str,
    pub(crate) first_exit: LowVectorPostRepairExitTelemetry,
    pub(crate) first_device_interaction: LowVectorPostRepairExitTelemetry,
    pub(crate) first_unhandled_access: LowVectorPostRepairUnhandledAccessTelemetry,
}

impl Default for LowVectorPostRepairTelemetry {
    fn default() -> Self {
        Self {
            continue_attempted: false,
            unsupported_exit_observed: false,
            unsupported_exit_reason: None,
            unsupported_exit_diagnosis: "not observed",
            first_exit: LowVectorPostRepairExitTelemetry::default(),
            first_device_interaction: LowVectorPostRepairExitTelemetry::default(),
            first_unhandled_access: LowVectorPostRepairUnhandledAccessTelemetry::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct LowVectorDiagnosticPageResumeTelemetry {
    pub(crate) attempted: bool,
    pub(crate) armed: bool,
    pub(crate) original_pc: Option<u64>,
    pub(crate) original_elr_el1: Option<u64>,
    pub(crate) original_esr_el1: Option<u64>,
    pub(crate) original_far_el1: Option<u64>,
    pub(crate) original_spsr_el1: Option<u64>,
    pub(crate) original_slot_bytes: Option<[u8; 12]>,
    pub(crate) target_instruction_word_before_eret: Option<u32>,
    pub(crate) target_stage1_leaf_descriptor_before_eret: Option<u64>,
    pub(crate) target_stage1_leaf_kind_before_eret: &'static str,
    pub(crate) target_is_installed_diagnostic_hvc_before_eret: bool,
    pub(crate) elr_el1_set_status: Option<i32>,
    pub(crate) spsr_el1_set_status: Option<i32>,
    pub(crate) cpsr_set_status: Option<i32>,
    pub(crate) pc_set_status: Option<i32>,
}

impl LowVectorDiagnosticPageResumeTelemetry {
    pub(crate) fn new() -> Self {
        Self {
            target_stage1_leaf_kind_before_eret: "not observed",
            ..Self::default()
        }
    }

    pub(crate) fn capture_original_context(&mut self, exit: &WindowsArmUefiFirmwareRunLoopExit) {
        self.original_pc = exit.pc_after_exit;
        self.original_elr_el1 = exit.elr_el1_after_exit;
        self.original_esr_el1 = exit.esr_el1_after_exit;
        self.original_far_el1 = exit.far_el1_after_exit;
        self.original_spsr_el1 = exit.spsr_el1_after_exit;
    }

    pub(crate) fn capture_diagnostic_slot_bytes(&mut self, original_slot_bytes: Option<[u8; 12]>) {
        self.original_slot_bytes = original_slot_bytes;
    }

    pub(crate) fn record_eret_target_snapshot(
        &mut self,
        instruction_word: Option<u32>,
        stage1_leaf_descriptor: Option<u64>,
        stage1_leaf_kind: &'static str,
    ) {
        self.target_instruction_word_before_eret = instruction_word;
        self.target_stage1_leaf_descriptor_before_eret = stage1_leaf_descriptor;
        self.target_stage1_leaf_kind_before_eret = stage1_leaf_kind;
        self.target_is_installed_diagnostic_hvc_before_eret =
            self.target_instruction_word_before_eret == Some(AARCH64_HVC_1_INSTRUCTION)
                && self.target_stage1_leaf_descriptor_before_eret
                    == Some(WINDOWS_ARM_LOW_VECTOR_DIAGNOSTIC_PAGE_DESCRIPTOR);
    }

    pub(crate) fn mark_attempted(&mut self) {
        self.attempted = true;
    }

    pub(crate) fn mark_armed(&mut self) {
        self.armed = true;
    }

    pub(crate) fn record_direct_resume_status(&mut self, cpsr_status: i32, pc_status: i32) {
        self.cpsr_set_status = Some(cpsr_status);
        self.pc_set_status = Some(pc_status);
    }

    pub(crate) fn record_eret_resume_status(
        &mut self,
        elr_status: i32,
        spsr_status: i32,
        pc_status: i32,
    ) {
        self.elr_el1_set_status = Some(elr_status);
        self.spsr_el1_set_status = Some(spsr_status);
        self.pc_set_status = Some(pc_status);
    }
}

impl LowVectorPostRepairTelemetry {
    pub(crate) fn mark_continue_attempted(&mut self) {
        self.continue_attempted = true;
    }

    pub(crate) fn observe_first_exit(
        &mut self,
        block_devices: &[WindowsArmVirtioBlockDeviceMetadata],
        exit: &WindowsArmUefiFirmwareRunLoopExit,
    ) {
        if self.first_exit.observed {
            return;
        }

        self.first_exit = LowVectorPostRepairExitTelemetry::observed(block_devices, exit);
    }

    pub(crate) fn observe_device_interaction(
        &mut self,
        block_devices: &[WindowsArmVirtioBlockDeviceMetadata],
        exit: &WindowsArmUefiFirmwareRunLoopExit,
    ) {
        if self.first_device_interaction.observed {
            return;
        }

        let candidate = LowVectorPostRepairExitTelemetry::observed(block_devices, exit);
        if !candidate.is_device_interaction() {
            return;
        }

        self.first_device_interaction = candidate;
    }

    pub(crate) fn first_device_interaction_is(&self, index: u32) -> bool {
        self.first_device_interaction.observed && self.first_device_interaction.index == Some(index)
    }

    pub(crate) fn observe_unsupported_exit(&mut self, exit: &WindowsArmUefiFirmwareRunLoopExit) {
        self.unsupported_exit_observed = true;
        self.unsupported_exit_reason = exit.exit_reason;
        self.unsupported_exit_diagnosis = windows_arm_firmware_run_loop_exit_diagnosis(exit);
    }

    pub(crate) fn observe_unhandled_mmio_access(
        &mut self,
        block_devices: &[WindowsArmVirtioBlockDeviceMetadata],
        exit: &WindowsArmUefiFirmwareRunLoopExit,
        access: DecodedMmioDataAbort,
        ipa: u64,
        value: Option<u64>,
        handler_result: &'static str,
    ) {
        if self.first_unhandled_access.observed {
            return;
        }

        self.first_unhandled_access = LowVectorPostRepairUnhandledAccessTelemetry::mmio(
            block_devices,
            exit,
            access,
            ipa,
            value,
            handler_result,
        );
    }

    pub(crate) fn observe_unhandled_sysreg_access(
        &mut self,
        exit: &WindowsArmUefiFirmwareRunLoopExit,
        access: DecodedSystemRegisterAccess,
        value: Option<u64>,
        handler_result: &'static str,
    ) {
        if self.first_unhandled_access.observed {
            return;
        }

        self.first_unhandled_access = LowVectorPostRepairUnhandledAccessTelemetry::sysreg(
            exit,
            access,
            value,
            handler_result,
        );
    }
}

pub(crate) fn windows_arm_firmware_post_repair_is_device_interaction(kind: &str) -> bool {
    kind == "sysreg:trap" || kind.starts_with("mmio:")
}

pub(crate) fn windows_arm_firmware_post_repair_interaction_kind(
    block_devices: &[WindowsArmVirtioBlockDeviceMetadata],
    exit: &WindowsArmUefiFirmwareRunLoopExit,
) -> &'static str {
    if exit.run_status != Some(HV_SUCCESS_VALUE) {
        return "hv-run-failure";
    }

    match exit.exit_reason {
        Some(HV_EXIT_REASON_CANCELED_VALUE) => "watchdog-cancel",
        Some(HV_EXIT_REASON_VTIMER_ACTIVATED_VALUE) => "vtimer",
        Some(HV_EXIT_REASON_EXCEPTION_VALUE) => {
            let Some(syndrome) = exit.exit_syndrome else {
                return "exception:missing-syndrome";
            };
            if decode_mmio_data_abort(syndrome).is_some() {
                let Some(ipa) = exit.exit_physical_address.or(exit.exit_virtual_address) else {
                    return "mmio:missing-address";
                };
                return match windows_arm_firmware_mmio_device_kind(block_devices, ipa) {
                    Some(WindowsArmFirmwareMmioDeviceKind::Pl011) => "mmio:pl011",
                    Some(WindowsArmFirmwareMmioDeviceKind::Pl031) => "mmio:pl031",
                    Some(WindowsArmFirmwareMmioDeviceKind::GicDistributor) => "mmio:gicd",
                    Some(WindowsArmFirmwareMmioDeviceKind::GicRedistributor) => "mmio:gicr",
                    Some(WindowsArmFirmwareMmioDeviceKind::VirtioInstallerIso) => {
                        "mmio:virtio-installer-iso"
                    }
                    Some(WindowsArmFirmwareMmioDeviceKind::VirtioTargetDisk) => {
                        "mmio:virtio-target-disk"
                    }
                    None if windows_arm_device_mmio_contains(ipa) => {
                        "mmio:windows-device-window-unclassified"
                    }
                    None => "mmio:outside-windows-device-window",
                };
            }
            if decode_system_register_trap(syndrome).is_some() {
                return "sysreg:trap";
            }
            "exception:non-mmio"
        }
        Some(_) => "exit:unsupported-reason",
        None => "exit:missing-info",
    }
}
