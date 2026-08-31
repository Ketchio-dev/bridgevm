use super::*;
use crate::platform_virt::FlatGuestRam;

impl BridgeVmPcPlatform {
    pub(super) fn on_mmio_without_dma(&mut self, gpa: u64, op: MmioOp) -> MmioOutcome {
        let mut mem = FlatGuestRam::new(board::RAM_BASE, 0x10000);
        self.on_mmio(gpa, op, &mut mem)
    }
}
