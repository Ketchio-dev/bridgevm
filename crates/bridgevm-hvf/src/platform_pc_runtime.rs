//! Reset of mutable device state while retaining attached media.

use super::*;

impl BridgeVmPcPlatform {
    pub fn reset_runtime_state(&mut self) {
        self.uart = Pl011::new();
        self.rtc = Pl031::new();
        self.pcie = PcieEcam::new_with_config(self.pcie_config);
        self.nvme.reset_registers_keep_disks();
        self.xhci = XhciController::new();
        self.pending_msix.clear();
        self.nvme_completion_scratch.clear();
        self.flash_vars.reset_runtime_state();
    }
}
