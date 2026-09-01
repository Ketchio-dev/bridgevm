//! Minimal terminal vCPU state retained for a failed Windows boot diagnostic.

use super::hvf::*;
pub(super) struct VcpuState {
    pub(super) pc: u64,
    pub(super) cpsr: u64,
    pub(super) exit_reason: u32,
    pub(super) syndrome: u64,
    pub(super) virtual_address: u64,
    pub(super) physical_address: u64,
    pub(super) x0: u64,
    pub(super) x1: u64,
    pub(super) x2: u64,
}

pub(super) unsafe fn capture(vcpu: HvVcpu, exit: *mut HvVcpuExit) -> Result<VcpuState, String> {
    let read = |register: u32, label: &str| -> Result<u64, String> {
        let mut value = 0;
        status(label, hv_vcpu_get_reg(vcpu, register, &mut value))?;
        Ok(value)
    };
    Ok(VcpuState {
        pc: read(HV_REG_PC, "read terminal PC")?,
        cpsr: read(HV_REG_CPSR, "read terminal CPSR")?,
        exit_reason: (*exit).reason,
        syndrome: (*exit).exception.syndrome,
        virtual_address: (*exit).exception.virtual_address,
        physical_address: (*exit).exception.physical_address,
        x0: read(0, "read terminal x0")?,
        x1: read(1, "read terminal x1")?,
        x2: read(2, "read terminal x2")?,
    })
}
