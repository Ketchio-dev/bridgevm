//! Diagnostic exception-vector routing and ERET resume handling.
//!
//! Split out of the single 12,111-line apple.rs backend.

use super::*;
use crate::*;

pub(crate) fn route_diagnostic_hvc_exit_through_eret_landing(
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

pub(crate) fn resume_diagnostic_eret_to_original_context(
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

pub(crate) fn arm_diagnostic_eret_resume(
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

pub(crate) fn executable_diagnostic_vector_route() -> DiagnosticVectorRoute {
    DiagnosticVectorRoute {
        vbar_el1: WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA,
        sync_pc: WINDOWS_ARM_EXECUTABLE_DIAGNOSTIC_VECTOR_IPA
            + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64,
    }
}

pub(crate) fn low_vector_diagnostic_page_route() -> DiagnosticVectorRoute {
    DiagnosticVectorRoute {
        vbar_el1: 0,
        sync_pc: WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64,
    }
}

pub(crate) fn recommended_vector_base_diagnostic_route(vbar_el1: u64) -> DiagnosticVectorRoute {
    DiagnosticVectorRoute {
        vbar_el1,
        sync_pc: vbar_el1 + WINDOWS_ARM_DIAGNOSTIC_VECTOR_CURRENT_EL_SPX_SYNC_OFFSET as u64,
    }
}

pub(crate) fn diagnostic_vector_hvc_eret_recovery_target(
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

pub(crate) fn diagnostic_vector_eret_landing_stop(
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

pub(crate) fn executable_diagnostic_vector_hvc_eret_recovery_target(
    exit: &WindowsArmUefiFirmwareRunLoopExit,
) -> Option<(u64, u64)> {
    diagnostic_vector_hvc_eret_recovery_target(exit, executable_diagnostic_vector_route())
}

pub(crate) fn executable_diagnostic_vector_eret_landing_stop(
    exit: &WindowsArmUefiFirmwareRunLoopExit,
) -> bool {
    diagnostic_vector_eret_landing_stop(exit, executable_diagnostic_vector_route())
}

pub(crate) fn low_vector_diagnostic_page_hvc_eret_recovery_target(
    exit: &WindowsArmUefiFirmwareRunLoopExit,
) -> Option<(u64, u64)> {
    diagnostic_vector_hvc_eret_recovery_target(exit, low_vector_diagnostic_page_route())
}

pub(crate) fn low_vector_diagnostic_page_eret_landing_stop(
    exit: &WindowsArmUefiFirmwareRunLoopExit,
) -> bool {
    diagnostic_vector_eret_landing_stop(exit, low_vector_diagnostic_page_route())
}
