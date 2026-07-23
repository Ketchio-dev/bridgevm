//! Split out of windows_arm.rs by responsibility.

use super::*;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;

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
    fn firmware_post_repair_interaction_classifier_labels_timer_and_virtio_mmio() {
        let vtimer_exit = WindowsArmUefiFirmwareRunLoopExit {
            run_status: Some(0),
            exit_reason: Some(2),
            ..test_firmware_run_loop_exit()
        };
        assert_eq!(
            windows_arm_firmware_post_repair_interaction_kind(&[], &vtimer_exit),
            "vtimer"
        );
        assert!(!windows_arm_firmware_post_repair_is_device_interaction(
            "vtimer"
        ));

        let installer_iso = PathBuf::from("/tmp/Win11_Arm64.iso");
        let block_devices = windows_arm_firmware_block_devices(Some(installer_iso), None);
        let virtio_exit = WindowsArmUefiFirmwareRunLoopExit {
            run_status: Some(0),
            exit_reason: Some(1),
            exit_syndrome: Some(0x93c0_8006),
            exit_physical_address: Some(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA),
            ..test_firmware_run_loop_exit()
        };
        assert_eq!(
            windows_arm_firmware_post_repair_interaction_kind(&block_devices, &virtio_exit),
            "mmio:virtio-installer-iso"
        );
        assert!(windows_arm_firmware_post_repair_is_device_interaction(
            "mmio:virtio-installer-iso"
        ));
        assert!(windows_arm_firmware_post_repair_is_device_interaction(
            "sysreg:trap"
        ));
        assert!(!windows_arm_firmware_post_repair_is_device_interaction(
            "exception:non-mmio"
        ));
    }

    #[test]
    fn post_repair_device_interaction_skips_diagnostic_vector_continuation() {
        let mut telemetry = LowVectorPostRepairTelemetry::default();
        let diagnostic_exit = WindowsArmUefiFirmwareRunLoopExit {
            index: 4,
            run_status: Some(0),
            exit_reason: Some(1),
            exit_syndrome: Some(0x5a00_0001),
            pc_after_exit: Some(0x200204),
            ..test_firmware_run_loop_exit()
        };
        telemetry.observe_first_exit(&[], &diagnostic_exit);
        telemetry.observe_device_interaction(&[], &diagnostic_exit);

        assert!(telemetry.first_exit.observed);
        assert_eq!(telemetry.first_exit.index, Some(4));
        assert_eq!(telemetry.first_exit.interaction_kind, "exception:non-mmio");
        assert!(!telemetry.first_device_interaction.observed);

        let installer_iso = PathBuf::from("/tmp/Win11_Arm64.iso");
        let block_devices = windows_arm_firmware_block_devices(Some(installer_iso), None);
        let virtio_exit = WindowsArmUefiFirmwareRunLoopExit {
            index: 7,
            run_status: Some(0),
            exit_reason: Some(1),
            exit_syndrome: Some(0x93c0_8006),
            exit_physical_address: Some(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA),
            pc_after_exit: Some(0x8001234),
            ..test_firmware_run_loop_exit()
        };
        telemetry.observe_device_interaction(&block_devices, &virtio_exit);

        assert!(telemetry.first_device_interaction.observed);
        assert_eq!(telemetry.first_device_interaction.index, Some(7));
        assert_eq!(
            telemetry.first_device_interaction.interaction_kind,
            "mmio:virtio-installer-iso"
        );
        assert_eq!(telemetry.first_device_interaction.pc, Some(0x8001234));
    }

    #[test]
    fn post_repair_exit_telemetry_records_access_metadata() {
        let installer_iso = PathBuf::from("/tmp/Win11_Arm64.iso");
        let block_devices = windows_arm_firmware_block_devices(Some(installer_iso), None);
        let virtio_exit = WindowsArmUefiFirmwareRunLoopExit {
            index: 9,
            run_status: Some(HV_SUCCESS_VALUE),
            exit_reason: Some(HV_EXIT_REASON_EXCEPTION_VALUE),
            exit_syndrome: Some(0x93c0_8006),
            exit_physical_address: Some(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + 0x10),
            pc_after_exit: Some(0x800_4321),
            ..test_firmware_run_loop_exit()
        };

        let telemetry = LowVectorPostRepairExitTelemetry::observed(&block_devices, &virtio_exit);
        assert_eq!(telemetry.interaction_kind, "mmio:virtio-installer-iso");
        assert_eq!(telemetry.access.kind, "mmio");
        assert_eq!(telemetry.access.direction, "read");
        assert_eq!(
            telemetry.access.address,
            Some(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + 0x10)
        );
        assert_eq!(telemetry.access.sysreg, None);
        assert_eq!(telemetry.access.syndrome, Some(0x93c0_8006));

        let mut output = String::new();
        append_low_vector_post_repair_exit_telemetry(
            &mut output,
            "Post-repair first device interaction",
            &telemetry,
            "Post-repair first device interaction kind",
            Some(&virtio_exit),
        );
        assert!(output.contains("Post-repair first device interaction access kind: mmio"));
        assert!(output.contains("Post-repair first device interaction access direction: read"));
        assert!(output.contains("Post-repair first device interaction access address: 0x10002010"));
        assert!(output.contains("Post-repair first device interaction access sysreg: not observed"));
        assert!(output.contains("Post-repair first device interaction access syndrome: 0x93c08006"));

        let sysreg_exit = WindowsArmUefiFirmwareRunLoopExit {
            index: 10,
            run_status: Some(HV_SUCCESS_VALUE),
            exit_reason: Some(HV_EXIT_REASON_EXCEPTION_VALUE),
            exit_syndrome: Some(sysreg_trap_syndrome(true, 2, 3, 0, 12, 12, 0)),
            pc_after_exit: Some(0x800_4567),
            ..test_firmware_run_loop_exit()
        };
        let telemetry = LowVectorPostRepairExitTelemetry::observed(&[], &sysreg_exit);
        assert_eq!(telemetry.interaction_kind, "sysreg:trap");
        assert_eq!(telemetry.access.kind, "icc-sysreg");
        assert_eq!(telemetry.access.direction, "read");
        assert_eq!(telemetry.access.address, None);
        assert_eq!(telemetry.access.sysreg, Some(ICC_IAR1_EL1_SYSREG));
        assert!(telemetry.access.syndrome.is_some());
    }
}
