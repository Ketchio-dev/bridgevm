//! Process NVMe queues and replay MSI-X vectors unmasked by a BAR write.

use super::*;

impl BridgeVmPcPlatform {
    pub(crate) fn process_nvme_queues(&mut self, mem: &mut dyn GuestMemoryMut) {
        self.nvme_completion_scratch.clear();
        self.nvme
            .process_into(mem, &mut self.nvme_completion_scratch);
        self.queue_nvme_completion_msix();
        self.flush_nvme_pending_msix();
    }
}
