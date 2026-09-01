use super::*;
impl BridgeVmPcPlatform {
    pub(crate) fn write_nvme_bar(
        &mut self,
        offset: u64,
        size: u8,
        value: u64,
        mem: &mut dyn GuestMemoryMut,
    ) {
        self.nvme.mmio_write(offset, size, value);
        self.nvme_completion_scratch.clear();
        self.nvme
            .process_into(mem, &mut self.nvme_completion_scratch);
        self.queue_nvme_completion_msix();
    }
}
