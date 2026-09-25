//! Owner-thread, status-aware timer and GIC observations for every stopped vCPU.
//!
//! The watchdog may request an exit, but it cannot read these registers. The
//! caller captures before destroying its own HVF vCPU and publishes only the
//! resulting plain values to the primary thread.

use crate::*;

const HV_SUCCESS: HvReturn = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CapturedRegister {
    pub(crate) status: HvReturn,
    pub(crate) value: u64,
}

impl CapturedRegister {
    pub(crate) const NOT_READ: Self = Self {
        status: -1,
        value: 0,
    };

    pub(crate) fn value(self) -> Option<u64> {
        (self.status == HV_SUCCESS).then_some(self.value)
    }

    pub(crate) fn general(vcpu: HvVcpuT, reg: u32) -> Self {
        let mut value = 0;
        // SAFETY: the caller owns the stopped, live vCPU; the output is a stack local.
        let status = unsafe { hv_vcpu_get_reg(vcpu, reg, &mut value) };
        Self { status, value }
    }

    pub(crate) fn system(vcpu: HvVcpuT, reg: u16) -> Self {
        let mut value = 0;
        // SAFETY: the caller owns the stopped, live vCPU; the output is a stack local.
        let status = unsafe { hv_vcpu_get_sys_reg(vcpu, reg, &mut value) };
        Self { status, value }
    }

    fn vtimer_offset(vcpu: HvVcpuT) -> Self {
        let mut value = 0;
        // SAFETY: the caller owns the stopped, live vCPU; the output is a stack local.
        let status = unsafe { hv_vcpu_get_vtimer_offset(vcpu, &mut value) };
        Self { status, value }
    }

    fn display(self) -> String {
        self.value().map_or_else(
            || format!("<read-failed:{:#x}>", self.status),
            |value| format!("{value:#x}"),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CapturedMask {
    pub(crate) status: HvReturn,
    pub(crate) value: bool,
}

impl CapturedMask {
    const NOT_READ: Self = Self {
        status: -1,
        value: false,
    };

    fn capture(vcpu: HvVcpuT) -> Self {
        let mut value = false;
        // SAFETY: the caller owns the stopped, live vCPU; the output is a stack local.
        let status = unsafe { hv_vcpu_get_vtimer_mask(vcpu, &mut value) };
        Self { status, value }
    }

    fn display(self) -> String {
        if self.status == HV_SUCCESS {
            self.value.to_string()
        } else {
            format!("<read-failed:{:#x}>", self.status)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OwnerInterruptState {
    pub(crate) cntv_ctl: CapturedRegister,
    pub(crate) cntv_cval: CapturedRegister,
    pub(crate) cntp_ctl: CapturedRegister,
    pub(crate) cntp_cval: CapturedRegister,
    pub(crate) vtimer_offset: CapturedRegister,
    pub(crate) host_vtimer_mask: CapturedMask,
    pub(crate) host_counter: u64,
    pub(crate) gic: crate::gic_irq_state::GicIrqState,
}

impl Default for OwnerInterruptState {
    fn default() -> Self {
        Self {
            cntv_ctl: CapturedRegister::NOT_READ,
            cntv_cval: CapturedRegister::NOT_READ,
            cntp_ctl: CapturedRegister::NOT_READ,
            cntp_cval: CapturedRegister::NOT_READ,
            vtimer_offset: CapturedRegister::NOT_READ,
            host_vtimer_mask: CapturedMask::NOT_READ,
            host_counter: 0,
            gic: Default::default(),
        }
    }
}

impl OwnerInterruptState {
    /// Only call after `hv_vcpu_run` returns, on this vCPU's owning thread.
    pub(crate) fn capture_on_owner_thread(vcpu: HvVcpuT) -> Self {
        Self {
            cntv_ctl: CapturedRegister::system(vcpu, HV_SYS_REG_CNTV_CTL_EL0),
            cntv_cval: CapturedRegister::system(vcpu, HV_SYS_REG_CNTV_CVAL_EL0),
            cntp_ctl: CapturedRegister::system(vcpu, HV_SYS_REG_CNTP_CTL_EL0),
            cntp_cval: CapturedRegister::system(vcpu, HV_SYS_REG_CNTP_CVAL_EL0),
            vtimer_offset: CapturedRegister::vtimer_offset(vcpu),
            host_vtimer_mask: CapturedMask::capture(vcpu),
            host_counter: host_cntvct(),
            // SAFETY: this is the owning thread and `vcpu` is still live.
            gic: unsafe { crate::gic_irq_state::capture(vcpu) },
        }
    }

    pub(crate) fn render(
        &self,
        index: u64,
        generation: u64,
        expected_mpidr: u64,
        measured_mpidr: CapturedRegister,
        cpsr: CapturedRegister,
    ) -> Vec<String> {
        let guest_now = self.vtimer_offset.value().map_or_else(
            || "<unavailable:vtimer-offset-read>".to_string(),
            |offset| format!("{:#x}", self.host_counter.wrapping_sub(offset)),
        );
        let irq_masked = cpsr.value().map_or_else(
            || "<unavailable:cpsr-read>".to_string(),
            |value| ((value >> 7) & 1 != 0).to_string(),
        );
        let mut lines = vec![
            format!(
                "VCPU-IRQ[{index}]: generation={generation} expected_mpidr={expected_mpidr:#x} measured_mpidr={} pstate_irq_masked={irq_masked}",
                measured_mpidr.display()
            ),
            format!(
                "VCPU-IRQ[{index}]: CNTV_CTL={} CNTV_CVAL={} CNTP_CTL={} CNTP_CVAL={} vtimer_offset={} guest_now={} host_vtimer_masked={}",
                self.cntv_ctl.display(), self.cntv_cval.display(),
                self.cntp_ctl.display(), self.cntp_cval.display(),
                self.vtimer_offset.display(), guest_now, self.host_vtimer_mask.display()
            ),
        ];
        lines.extend(
            crate::gic_irq_state::render(&self.gic)
                .into_iter()
                .map(|line| format!("VCPU-IRQ[{index}] {line}")),
        );
        lines
    }
}

#[cfg(test)]
#[path = "owner_interrupt_tests.rs"]
mod tests;
