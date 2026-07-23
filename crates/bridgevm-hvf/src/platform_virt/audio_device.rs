//! Intel HDA polling, PCM sink installation, and its BAR handler.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::hda::HdaPcmSink;

impl VirtPlatform {
    pub(crate) fn hda_access(
        &mut self,
        offset: u64,
        op: MmioOp,
        mem: &mut dyn GuestMemoryMut,
    ) -> MmioOutcome {
        let outcome = {
            let Some(hda) = self.hda.as_mut() else {
                return MmioOutcome::KnownUnimplemented("intel-hda");
            };
            match op {
                MmioOp::Read { size } => MmioOutcome::ReadValue(hda.mmio_read(offset, size)),
                MmioOp::Write { size, value } => {
                    hda.mmio_write(offset, size, value, mem);
                    MmioOutcome::WriteAck
                }
            }
        };
        self.flush_hda_pending_msi();
        outcome
    }

    /// Advance the host-clock-paced HDA playback stream and flush standard MSI raised
    /// by IOC or DMA errors into the platform's pending-message aggregation.
    pub fn poll_hda(&mut self, mem: &mut dyn GuestMemoryMut) {
        if let Some(hda) = self.hda.as_mut() {
            hda.poll(mem, self.host_now);
        }
        self.flush_hda_pending_msi();
    }

    /// Install or clear the host PCM sink for the optional HDA controller.
    pub fn set_hda_pcm_sink(&mut self, sink: Option<Box<dyn HdaPcmSink>>) {
        if let Some(hda) = self.hda.as_mut() {
            hda.set_pcm_sink(sink);
        }
    }
}
