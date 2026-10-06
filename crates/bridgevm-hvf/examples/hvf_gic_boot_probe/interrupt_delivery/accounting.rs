//! Drain accounting is committed for every attempted provider prefix.
use crate::*;
impl RunLoopDrainStats {
    pub(crate) fn record_completed_delivery(
        &mut self,
        context: DrainContext,
        spi: DeliveryCounts,
        msix: DeliveryCounts,
    ) {
        self.last_drain_location = Some(context.location.as_str());
        self.last_drain_exit = Some(context.exit);
        self.last_drain_pc = Some(context.pc);
        self.last_drain_msix = msix;
        self.last_drain_spi = spi;
        self.spi.add(spi);
        self.msix.add(msix);

        if spi.has_deliveries() || msix.has_deliveries() {
            let location = context.location.as_str();
            self.last_nonzero_location = Some(location);
            self.last_nonzero_exit = Some(context.exit);
            self.last_nonzero_pc = Some(context.pc);
            if self.trace {
                println!(
                    "G004 IRQ drain: location={location} exit={} pc={:#x} msix drained={} success={} failure={} spi drained={} success={} failure={}",
                    context.exit,
                    context.pc,
                    msix.drained,
                    msix.success,
                    msix.failure,
                    spi.drained,
                    spi.success,
                    spi.failure
                );
            }
        }
    }
}
