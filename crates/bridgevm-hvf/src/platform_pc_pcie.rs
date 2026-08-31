use super::*;
use crate::pcie::NVME_BDF;

impl BridgeVmPcPlatform {
    pub(crate) fn pcie_mmio_access(
        &mut self,
        aperture: &'static str,
        gpa: u64,
        op: MmioOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> MmioOutcome {
        let Some(target) = self.pcie.mmio_target(gpa) else {
            return MmioOutcome::KnownUnimplemented(aperture);
        };
        match (target.bdf, target.bar_index, op) {
            (NVME_BDF, 0, MmioOp::Read { size }) => {
                MmioOutcome::ReadValue(self.nvme.mmio_read(target.offset, size))
            }
            (NVME_BDF, 0, MmioOp::Write { size, value }) => {
                self.write_nvme_bar(target.offset, size, value, mem);
                MmioOutcome::WriteAck
            }
            _ => MmioOutcome::KnownUnimplemented(aperture),
        }
    }
}
#[cfg(test)]
#[path = "platform_pc_pcie_tests.rs"]
mod tests;
