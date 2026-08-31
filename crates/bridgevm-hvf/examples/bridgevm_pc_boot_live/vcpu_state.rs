//! Minimal terminal vCPU state retained for a failed Windows boot diagnostic.

use super::hvf::*;

pub(super) struct VcpuState {
    pub(super) pc: u64,
    pub(super) cpsr: u64,
    pub(super) exit_reason: u32,
    pub(super) syndrome: u64,
    pub(super) virtual_address: u64,
    pub(super) physical_address: u64,
}

pub(super) unsafe fn capture(vcpu: HvVcpu, exit: *mut HvVcpuExit) -> Result<VcpuState, String> {
    let mut pc = 0;
    let mut cpsr = 0;
    status(
        "read terminal PC",
        hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc),
    )?;
    status(
        "read terminal CPSR",
        hv_vcpu_get_reg(vcpu, HV_REG_CPSR, &mut cpsr),
    )?;
    Ok(VcpuState {
        pc,
        cpsr,
        exit_reason: (*exit).reason,
        syndrome: (*exit).exception.syndrome,
        virtual_address: (*exit).exception.virtual_address,
        physical_address: (*exit).exception.physical_address,
    })
}
