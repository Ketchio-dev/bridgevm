//! Intel HDA polling, PCM sink installation, and its BAR handler.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::hda::{HdaPcmSink, HdaPcmSource};

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

    /// The launch-time microphone setting: without it the guest sees main's
    /// HDA device, with no input stream or microphone pin (hda/layout.rs).
    pub fn set_hda_microphone(&mut self, on: bool) {
        if let Some(hda) = self.hda.as_mut() {
            hda.set_microphone(on);
        }
    }

    /// Install or clear the host capture source for the HDA input stream; the
    /// guest records silence without one.
    pub fn set_hda_pcm_source(&mut self, source: Option<Box<dyn HdaPcmSource>>) {
        if let Some(hda) = self.hda.as_mut() {
            hda.set_pcm_source(source);
        }
    }
}
