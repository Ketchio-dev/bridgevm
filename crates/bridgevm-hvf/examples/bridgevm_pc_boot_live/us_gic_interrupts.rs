//! Platform MSI delivery and virtual-timer completion for the userspace GIC.

use super::super::memory::GuestRam;
use super::{hv_vcpu_set_vtimer_mask, status, translate, HvVcpu, UsGic};
use bridgevm_hvf::platform_pc::BridgeVmPcPlatform;
use bridgevm_hvf::platform_virt::{MmioOp, MmioOutcome};
use bridgevm_hvf::userspace_gic::{ICC_DIR_EL1, ICC_EOIR1_EL1, VTIMER_INTID};

impl UsGic {
    pub(in super::super) fn line_asserted(&self) -> bool {
        self.inner.line_asserted(0)
    }

    pub(in super::super) fn platform_mmio(
        &mut self,
        platform: &mut BridgeVmPcPlatform,
        address: u64,
        operation: MmioOp,
        ram: &mut GuestRam<'_>,
    ) -> MmioOutcome {
        let outcome = platform.on_mmio(address, operation, ram);
        platform.drain_pending_msix_into(&mut self.msix_scratch);
        for message in self.msix_scratch.drain(..) {
            let address = translate(message.address).unwrap_or(message.address);
            self.inner.send_msi(address, message.data);
        }
        outcome
    }

    pub(super) unsafe fn finish_vtimer(
        &self,
        vcpu: HvVcpu,
        sys_reg: u16,
        is_read: bool,
        write_value: u64,
    ) -> Result<(), String> {
        let intid = write_value & 0xff_ffff;
        if !is_read
            && (sys_reg == ICC_EOIR1_EL1 || sys_reg == ICC_DIR_EL1)
            && intid == u64::from(VTIMER_INTID)
        {
            status("unmask VTimer", hv_vcpu_set_vtimer_mask(vcpu, false))?;
        }
        Ok(())
    }
}
