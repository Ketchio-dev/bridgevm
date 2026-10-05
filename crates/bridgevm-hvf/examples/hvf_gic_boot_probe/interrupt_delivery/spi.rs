//! SPI provider attempts and fail-closed run-loop continuation.

use crate::*;

#[derive(Clone, Copy)]
pub(crate) struct SpiDeliveryFailure {
    pub(crate) intid: u32,
    pub(crate) level: bool,
    pub(crate) status: HvReturn,
    pub(crate) counts: DeliveryCounts,
}

pub(crate) struct SpiDeliveryError {
    pub(crate) context: DrainContext,
    pub(crate) failure: SpiDeliveryFailure,
}

impl std::fmt::Display for SpiDeliveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let failure = self.failure;
        write!(
            f,
            "SPI delivery failed: intid={} level={} status={:#x} location={} exit={} pc={:#x}",
            failure.intid,
            failure.level,
            failure.status,
            self.context.location.as_str(),
            self.context.exit,
            self.context.pc
        )
    }
}

pub(crate) fn deliver_spi_levels(
    scratch: &mut Vec<(u32, bool)>,
    mut deliver: impl FnMut(u32, bool) -> HvReturn,
) -> Result<DeliveryCounts, SpiDeliveryFailure> {
    let mut counts = DeliveryCounts::default();
    for &(intid, level) in scratch.iter() {
        let status = deliver(intid, level);
        counts.record_status(status);
        if status != 0 {
            let error = SpiDeliveryFailure {
                intid,
                level,
                status,
                counts,
            };
            scratch.clear();
            return Err(error);
        }
    }
    scratch.clear();
    Ok(counts)
}

#[cfg(test)]
#[path = "spi_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "spi_platform_tests.rs"]
mod platform_tests;
