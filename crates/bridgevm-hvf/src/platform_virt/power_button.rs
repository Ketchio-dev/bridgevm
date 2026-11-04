//! The ACPI power button: a host press and the guest's GED selector reads.

use super::*;

impl VirtPlatform {
    /// Press the ACPI power button once. Windows reads the GED selector and
    /// applies its power-button action, which needs no guest agent.
    pub fn press_power_button(&mut self) {
        self.acpi_ged
            .press_power_button(&mut self.pending_spi_levels);
    }

    pub(crate) fn acpi_ged_access(&mut self, offset: u64, op: MmioOp) -> MmioOutcome {
        match op {
            MmioOp::Read { size } => {
                let events = self
                    .acpi_ged
                    .mmio_read(offset, size, &mut self.pending_spi_levels);
                if events != 0 {
                    println!("BVPOWER guest read events={events:#x}");
                }
                MmioOutcome::ReadValue(events)
            }
            MmioOp::Write { .. } => MmioOutcome::WriteAck,
        }
    }
}
