//! MMIO-register doorbells, also exposed by the modeled PCI BAR alias.

use super::*;
use crate::fwcfg::GuestMemoryMut;

const DRIVER_OK: u32 = 4;

impl VirtioMmioBlock {
    pub(crate) fn notify_mmio_queue(&mut self, queue_index: u64, mem: &mut dyn GuestMemoryMut) {
        if queue_index != 0 {
            return;
        }
        self.stats.notify_count = self.stats.notify_count.saturating_add(1);
        // Legacy interfaces permit early queue use; their contract is unchanged.
        if self.transport == VirtioMmioTransport::Modern && self.status & DRIVER_OK == 0 {
            return;
        }
        self.process_queue(mem);
    }
}
