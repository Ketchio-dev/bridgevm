//! PCIe message aggregation for the independent BridgeVM PC runtime.

use super::*;

impl BridgeVmPcPlatform {
    pub fn drain_pending_msix_into(&mut self, out: &mut Vec<MsixMessage>) {
        out.append(&mut self.pending_msix);
    }

    pub(crate) fn queue_nvme_completion_msix(&mut self) {
        let control = self.pcie.nvme_msix_control();
        for completion in &self.nvme_completion_scratch {
            if let Some(message) =
                self.nvme
                    .raise_msix(completion.vector, control.enabled, control.function_masked)
            {
                self.pending_msix.push(message);
            }
        }
        self.nvme_completion_scratch.clear();
    }

    pub(crate) fn flush_nvme_pending_msix(&mut self) {
        let control = self.pcie.nvme_msix_control();
        self.nvme.drain_pending_msix_into(
            control.enabled,
            control.function_masked,
            &mut self.pending_msix,
        );
    }

    pub(crate) fn queue_xhci_completion_msix(&mut self) {
        let control = self.pcie.xhci_msix_control();
        self.xhci.raise_pending_interrupter_msix_into(
            control.enabled,
            control.function_masked,
            &mut self.pending_msix,
        );
    }

    pub(crate) fn flush_xhci_pending_msix(&mut self) {
        let control = self.pcie.xhci_msix_control();
        self.xhci.drain_pending_msix_into(
            control.enabled,
            control.function_masked,
            &mut self.pending_msix,
        );
    }
}

#[cfg(test)]
#[path = "platform_pc_interrupts_tests.rs"]
mod tests;
