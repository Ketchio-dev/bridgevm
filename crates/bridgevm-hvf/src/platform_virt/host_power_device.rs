//! The Mac's battery and AC adapter: host readings in, guest page reads out.

use super::*;
use crate::host_power::HostPower;

impl VirtPlatform {
    /// Show a new reading of the Mac's power source. A change raises the GED
    /// event that makes Windows re-read its AC adapter and battery. Does
    /// nothing when the VM has no battery device.
    pub fn set_host_power(&mut self, power: HostPower) {
        let Some(page) = self.host_power.as_mut() else {
            return;
        };
        let events = page.update(power);
        if events != 0 {
            self.acpi_ged.raise(events, &mut self.pending_spi_levels);
        }
    }

    /// The page is read-only: the guest cannot change what the Mac reports.
    pub(crate) fn host_power_access(&mut self, offset: u64, op: MmioOp) -> MmioOutcome {
        let Some(page) = self.host_power.as_ref() else {
            return MmioOutcome::Unmapped;
        };
        match op {
            MmioOp::Read { size } => MmioOutcome::ReadValue(page.mmio_read(offset, size)),
            MmioOp::Write { .. } => MmioOutcome::WriteAck,
        }
    }
}
