//! xHCI BAR execution using the shared specification-derived controller model.

use super::*;

impl BridgeVmPcPlatform {
    pub(crate) fn xhci_bar_access(
        &mut self,
        offset: u64,
        op: MmioOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> MmioOutcome {
        match op {
            MmioOp::Read { size } => MmioOutcome::ReadValue(self.xhci.mmio_read(offset, size)),
            MmioOp::Write { size, value } => {
                if self.xhci.mmio_write_with_mem(offset, size, value, mem) {
                    self.queue_xhci_completion_msix();
                }
                self.flush_xhci_pending_msix();
                MmioOutcome::WriteAck
            }
        }
    }
}

#[cfg(test)]
#[path = "platform_pc_xhci_tests.rs"]
mod tests;
