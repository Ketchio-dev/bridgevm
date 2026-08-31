//! Userspace GICv3 for the boot-live example.
//!
//! The board advertises its GIC at the BridgeVM PC addresses, but the reusable
//! `UserspaceGic` decodes against the shipping `machine` board's GIC bases, so
//! MMIO addresses are translated offset-for-offset (the register layouts and the
//! redistributor stride are identical). With no in-kernel `hv_gic`, the guest's
//! GIC MMIO and `ICC_*` system-register accesses trap to the host and are served
//! here, and pending interrupts are asserted with `hv_vcpu_set_pending_interrupt`.

use super::hvf::*;
use bridgevm_hvf::machine as sh;
use bridgevm_hvf::machine::bridgevm_pc as pc;
use bridgevm_hvf::userspace_gic::UserspaceGic;

pub(super) struct UsGic {
    inner: UserspaceGic,
}

fn translate(addr: u64) -> Option<u64> {
    if pc::GIC_DIST.contains(addr) {
        Some(sh::GIC_DIST.base + (addr - pc::GIC_DIST.base))
    } else if pc::GIC_REDIST.contains(addr) {
        Some(sh::GIC_REDIST.base + (addr - pc::GIC_REDIST.base))
    } else if pc::GIC_MSI_FRAME.contains(addr) {
        Some(sh::GIC_MSI_FRAME.base + (addr - pc::GIC_MSI_FRAME.base))
    } else {
        None
    }
}

impl UsGic {
    pub(super) fn new() -> Self {
        Self {
            inner: UserspaceGic::new(1),
        }
    }

    pub(super) fn owns(addr: u64) -> bool {
        translate(addr).is_some()
    }

    pub(super) fn mmio(&mut self, addr: u64, width: u8, write: Option<u64>) -> u64 {
        match translate(addr) {
            Some(a) => self.inner.mmio(a, width, write).value,
            None => 0,
        }
    }

    pub(super) fn vtimer_fired(&mut self) {
        self.inner.set_vtimer_ppi(0, true);
    }

    /// Assert or deassert the vCPU IRQ line to match the userspace GIC.
    pub(super) unsafe fn refresh(&self, vcpu: HvVcpu) -> Result<(), String> {
        status(
            "assert IRQ line",
            hv_vcpu_set_pending_interrupt(vcpu, HV_INTERRUPT_TYPE_IRQ, self.inner.line_asserted(0)),
        )
    }

    /// Serve a trapped system register (EC 0x18): a GIC CPU-interface register
    /// via the userspace GIC, or a benign passthrough for the other registers
    /// HVF traps at EL1 -- PMU counters hand back a monotonically increasing
    /// value so perf-counter calibration advances, everything else reads zero
    /// and writes are ignored -- so the guest keeps running. Advances PC.
    pub(super) unsafe fn sysreg(&mut self, vcpu: HvVcpu, syndrome: u64) -> Result<(), String> {
        let iss = syndrome & 0x01ff_ffff;
        let op0 = ((iss >> 20) & 0x3) as u16;
        let op2 = ((iss >> 17) & 0x7) as u16;
        let op1 = ((iss >> 14) & 0x7) as u16;
        let crn = ((iss >> 10) & 0xf) as u16;
        let rt = ((iss >> 5) & 0x1f) as u32;
        let crm = ((iss >> 1) & 0xf) as u16;
        let is_read = (iss & 1) != 0;
        let sys_reg = (op0 << 14) | (op1 << 11) | (crn << 7) | (crm << 3) | op2;
        let write_value = if is_read || rt == 31 {
            0
        } else {
            let mut value = 0;
            status("read sysreg source", hv_vcpu_get_reg(vcpu, rt, &mut value))?;
            value
        };
        const CNTFRQ_EL0: u16 = 0xdf00;
        let read_value = match self.inner.sysreg(0, sys_reg, is_read, write_value) {
            Some(result) => result.value,
            None if !is_read => 0,
            // CNTFRQ_EL0 must report a real counter frequency, or the guest's
            // cycles-per-time math divides by zero (brk #0xf004).
            None if sys_reg == CNTFRQ_EL0 => 24_000_000,
            // Timer/PMU counter reads (CRn 9/14) hand back the host monotonic
            // clock so elapsed-time math advances by realistic, non-zero deltas.
            None if crn == 9 || crn == 14 => mach_absolute_time(),
            None => 0,
        };
        if is_read && rt != 31 {
            status("write sysreg dest", hv_vcpu_set_reg(vcpu, rt, read_value))?;
        }
        let mut pc = 0;
        status("read sysreg PC", hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc))?;
        status("advance sysreg PC", hv_vcpu_set_reg(vcpu, HV_REG_PC, pc + 4))
    }
}
