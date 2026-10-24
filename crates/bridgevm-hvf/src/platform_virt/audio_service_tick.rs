//! What a host thread needs to service HDA DMA without a vCPU exit: the
//! controller's stream-activity flag, and a poll that hands back only HDA's
//! MSI. The probe's service tick (examples/hvf_gic_boot_probe/
//! hda_service_tick.rs) runs the poll under the platform lock and sends the
//! message after releasing it. A vCPU drain's `poll_hda` (audio_device.rs)
//! instead moves the message into the shared pending list that the drain
//! delivers; a host thread must not drain that list, or it would take other
//! devices' messages, and a message it left there would wait for a vCPU exit.
//! These live beside audio_device.rs, which is at its structural budget.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::hda::{HdaController, HdaStreamActivity};
use crate::msix::MsixMessage;
use std::sync::Arc;
use std::time::Instant;

impl VirtPlatform {
    /// The HDA stream-activity flag; `None` without HDA. Controller and
    /// platform resets keep the same handle.
    pub fn hda_stream_activity(&self) -> Option<Arc<HdaStreamActivity>> {
        self.hda.as_ref().map(HdaController::stream_activity)
    }

    /// Set the host time to `now`, advance HDA playback DMA to it, and take
    /// HDA's MSI when one became due with MSI enabled. Other devices' pending
    /// messages and SPI levels stay where they are, for the vCPU drains.
    pub fn poll_hda_taking_msi(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        now: Instant,
    ) -> Option<MsixMessage> {
        self.set_host_now(now);
        let config = self.pcie.hda_msi_config();
        let hda = self.hda.as_mut()?;
        hda.poll(mem, Some(now));
        let mut taken = Vec::new();
        hda.drain_pending_msi_into(config.enabled, config.address, config.data, &mut taken);
        taken.pop()
    }
}
