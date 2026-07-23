//! Split out of windows_arm.rs by responsibility.

use super::*;

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

    #[test]
    fn firmware_run_loop_diagnoses_empty_recommended_vector_base_sync_slot() {
        let mut exit = test_firmware_run_loop_exit();
        exit.vbar_el1_after_exit = Some(WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA);
        exit.pc_after_exit = Some(
            WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA
                + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64,
        );
        exit.instruction_word_after_exit = Some(0);
        exit.pc_stage1_leaf_level_after_exit = Some(2);
        exit.pc_stage1_leaf_descriptor_after_exit = Some(0x200f8d);
        exit.pc_stage1_leaf_descriptor_kind_after_exit = "block";
        exit.pc_stage1_leaf_pxn_after_exit = Some(false);
        exit.pc_stage1_leaf_uxn_after_exit = Some(false);

        assert_eq!(
            windows_arm_firmware_run_loop_exit_diagnosis_kind(&exit),
            WindowsArmFirmwareRunLoopDiagnosis::RecommendedVectorBaseEmptySyncSlot
        );
        assert_eq!(
            windows_arm_firmware_run_loop_exit_diagnosis(&exit),
            "recommended-vector-base-empty-sync-slot"
        );
    }
}
