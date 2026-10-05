//! Message delivery and completion, with an injectable provider boundary.
use crate::*;

#[derive(Clone, Copy)]
pub(crate) struct MsixDeliveryFailure {
    pub(crate) message: MsixMessage,
    pub(crate) status: HvReturn,
    pub(crate) counts: DeliveryCounts,
}

pub(crate) fn deliver_msix_messages(
    messages: &[MsixMessage],
    mut deliver: impl FnMut(MsixMessage) -> HvReturn,
) -> Result<DeliveryCounts, MsixDeliveryFailure> {
    let mut counts = DeliveryCounts::default();
    for &message in messages {
        let status = deliver(message);
        counts.record_status(status);
        if status != 0 {
            return Err(MsixDeliveryFailure {
                message,
                status,
                counts,
            });
        }
    }
    Ok(counts)
}

pub(crate) fn complete_prepared_interrupt_delivery(
    pending: Result<PendingDrainDelivery, SpiDeliveryError>,
    complete: impl FnOnce(PendingDrainDelivery) -> Result<(), InterruptDeliveryError>,
) -> Result<(), InterruptDeliveryError> {
    let pending = pending.map_err(InterruptDeliveryError::Spi)?;
    complete(pending)
}

#[cfg(test)]
#[path = "msix_platform_tests.rs"]
mod platform_tests;
#[cfg(test)]
#[path = "msix_tests.rs"]
mod tests;
