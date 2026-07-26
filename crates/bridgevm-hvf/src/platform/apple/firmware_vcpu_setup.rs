//! Initial register state for the bounded Windows UEFI firmware vCPU.

use super::*;
use crate::*;

#[derive(Default)]
pub(crate) struct FirmwareVcpuSeedOutcome {
    pub(crate) pc_set: bool,
    pub(crate) x0_dtb_ipa_set: bool,
    pub(crate) cpsr_set: bool,
    pub(crate) sp_el1_set: bool,
    pub(crate) diagnostic_vector_vbar_el1_set: bool,
    pub(crate) interrupt_timer_initialized: bool,
    pub(crate) pc_set_status: Option<HvReturn>,
    pub(crate) x0_dtb_ipa_set_status: Option<HvReturn>,
    pub(crate) cpsr_set_status: Option<HvReturn>,
    pub(crate) sp_el1_set_status: Option<HvReturn>,
    pub(crate) diagnostic_vector_vbar_el1_set_status: Option<HvReturn>,
    pub(crate) vtimer_offset_set_status: Option<HvReturn>,
    pub(crate) cntv_cval_set_status: Option<HvReturn>,
    pub(crate) cntv_ctl_set_status: Option<HvReturn>,
    pub(crate) vtimer_initial_unmask_status: Option<HvReturn>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn seed_firmware_vcpu_registers(
    vcpu: HvVcpu,
    platform_dtb_populated: bool,
    diagnostic_vector_seed_requested: bool,
    diagnostic_vector_populated: bool,
    diagnostic_vector_ipa: u64,
    wire_interrupt_timer: bool,
    sp_el1_seed_ipa: u64,
    cntv_cval_value: u64,
    cntv_ctl_value: u64,
    blockers: &mut Vec<String>,
) -> FirmwareVcpuSeedOutcome {
    let mut outcome = FirmwareVcpuSeedOutcome::default();

    let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_PC, WINDOWS_ARM_UEFI_CODE_IPA) };
    outcome.pc_set_status = Some(status);
    outcome.pc_set = status == HV_SUCCESS;
    if !outcome.pc_set {
        blockers.push(format!("hv_vcpu_set_reg(PC) failed: {status:#x}"));
    }

    if platform_dtb_populated {
        let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_X0, WINDOWS_ARM_PLATFORM_DTB_IPA) };
        outcome.x0_dtb_ipa_set_status = Some(status);
        outcome.x0_dtb_ipa_set = status == HV_SUCCESS;
        if !outcome.x0_dtb_ipa_set {
            blockers.push(format!(
                "hv_vcpu_set_reg(X0=platform DTB IPA) failed: {status:#x}"
            ));
        }
    }

    let status = unsafe { hv_vcpu_set_reg(vcpu, HV_REG_CPSR, AARCH64_PSTATE_EL1H_DAIF_MASKED) };
    outcome.cpsr_set_status = Some(status);
    outcome.cpsr_set = status == HV_SUCCESS;
    if !outcome.cpsr_set {
        blockers.push(format!("hv_vcpu_set_reg(CPSR) failed: {status:#x}"));
    }

    let status = unsafe { hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_SP_EL1, sp_el1_seed_ipa) };
    outcome.sp_el1_set_status = Some(status);
    outcome.sp_el1_set = status == HV_SUCCESS;
    if !outcome.sp_el1_set {
        blockers.push(format!("hv_vcpu_set_sys_reg(SP_EL1) failed: {status:#x}"));
    }

    if diagnostic_vector_seed_requested && diagnostic_vector_populated {
        let status =
            unsafe { hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_VBAR_EL1, diagnostic_vector_ipa) };
        outcome.diagnostic_vector_vbar_el1_set_status = Some(status);
        outcome.diagnostic_vector_vbar_el1_set = status == HV_SUCCESS;
        if !outcome.diagnostic_vector_vbar_el1_set {
            blockers.push(format!(
                "hv_vcpu_set_sys_reg(VBAR_EL1 diagnostic vector) failed: {status:#x}"
            ));
        }
    }

    if wire_interrupt_timer {
        let offset_status =
            unsafe { hv_vcpu_set_vtimer_offset(vcpu, WINDOWS_ARM_VTIMER_OFFSET_VALUE) };
        outcome.vtimer_offset_set_status = Some(offset_status);
        if offset_status != HV_SUCCESS {
            blockers.push(format!(
                "hv_vcpu_set_vtimer_offset for firmware run-loop failed: {offset_status:#x}"
            ));
        }

        let cval_status =
            unsafe { hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_CNTV_CVAL_EL0, cntv_cval_value) };
        outcome.cntv_cval_set_status = Some(cval_status);
        if cval_status != HV_SUCCESS {
            blockers.push(format!(
                "hv_vcpu_set_sys_reg(CNTV_CVAL_EL0) for firmware run-loop failed: {cval_status:#x}"
            ));
        }

        let ctl_status =
            unsafe { hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_CNTV_CTL_EL0, cntv_ctl_value) };
        outcome.cntv_ctl_set_status = Some(ctl_status);
        if ctl_status != HV_SUCCESS {
            blockers.push(format!(
                "hv_vcpu_set_sys_reg(CNTV_CTL_EL0) for firmware run-loop failed: {ctl_status:#x}"
            ));
        }

        let unmask_status = unsafe { hv_vcpu_set_vtimer_mask(vcpu, false) };
        outcome.vtimer_initial_unmask_status = Some(unmask_status);
        if unmask_status != HV_SUCCESS {
            blockers.push(format!(
                "hv_vcpu_set_vtimer_mask(false) for firmware run-loop failed: {unmask_status:#x}"
            ));
        }

        outcome.interrupt_timer_initialized = offset_status == HV_SUCCESS
            && cval_status == HV_SUCCESS
            && ctl_status == HV_SUCCESS
            && unmask_status == HV_SUCCESS;
    }

    outcome
}
