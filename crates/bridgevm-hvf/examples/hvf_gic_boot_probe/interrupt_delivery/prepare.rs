//! Polling and preparation before interrupt delivery completion.

use crate::*;

impl RunLoopDrainStats {
    pub(crate) fn prepare_pending_delivery(
        &mut self,
        platform: &mut VirtPlatform,
        mem: &mut dyn GuestMemoryMut,
        trace: DrainTrace,
        context: DrainContext,
    ) -> Result<PendingDrainDelivery, SpiDeliveryError> {
        self.prepare_pending_delivery_inner(platform, mem, trace, context, true)
    }
    pub(crate) fn prepare_pending_delivery_after_mmio(
        &mut self,
        platform: &mut VirtPlatform,
        mem: &mut dyn GuestMemoryMut,
        trace: DrainTrace,
        context: DrainContext,
        post_drain: MmioPostDrain,
    ) -> Result<PendingDrainDelivery, SpiDeliveryError> {
        self.prepare_pending_delivery_inner(
            platform,
            mem,
            trace,
            context,
            !post_drain.xhci_setup_input_attempted(),
        )
    }
    pub(crate) fn prepare_pending_delivery_inner(
        &mut self,
        platform: &mut VirtPlatform,
        mem: &mut dyn GuestMemoryMut,
        trace: DrainTrace,
        context: DrainContext,
        drain_xhci_setup_input: bool,
    ) -> Result<PendingDrainDelivery, SpiDeliveryError> {
        match context.location {
            DrainLocation::PreRun => self.pre_run_attempts += 1,
            DrainLocation::DataAbort => self.data_abort_attempts += 1,
        }

        // Feed host time to the platform's HID report pacing (the crate holds no
        // clock of its own). Both PreRun and DataAbort drains route through here.
        platform.set_host_now(std::time::Instant::now());
        if drain_xhci_setup_input {
            platform.drain_xhci_setup_input_reports(mem);
        }
        platform.drain_xhci_pointer_input_reports(mem);
        platform.poll_virtio_net(mem);
        platform.poll_virtio_console(mem);
        platform.poll_virtio_gpu_fences(mem);
        platform.poll_hda(mem);
        let spi = deliver_pending_spis(platform, &mut self.pending_spi_scratch, trace.spi);
        let pending = self.pending_spi_delivery(context, spi)?;
        debug_assert!(self.pending_msix_scratch.is_empty());
        self.pending_msix_scratch.clear();
        platform.drain_pending_msix_into(&mut self.pending_msix_scratch);
        Ok(pending)
    }
    pub(crate) fn pending_spi_delivery(
        &mut self,
        context: DrainContext,
        spi: Result<DeliveryCounts, SpiDeliveryFailure>,
    ) -> Result<PendingDrainDelivery, SpiDeliveryError> {
        match spi {
            Ok(spi) => Ok(PendingDrainDelivery { context, spi }),
            Err(failure) => {
                self.spi.add(failure.counts);
                self.last_drain_location = Some(context.location.as_str());
                self.last_drain_exit = Some(context.exit);
                self.last_drain_pc = Some(context.pc);
                self.last_drain_spi = failure.counts;
                self.last_drain_msix = DeliveryCounts::default();
                self.last_nonzero_location = Some(context.location.as_str());
                self.last_nonzero_exit = Some(context.exit);
                self.last_nonzero_pc = Some(context.pc);
                Err(SpiDeliveryError { context, failure })
            }
        }
    }
}

impl RunLoopDrainStats {
    pub(crate) fn prepare_primary_pre_run(
        &mut self,
        platform: &Arc<Mutex<VirtPlatform>>,
        mem: &mut dyn GuestMemoryMut,
        smp_trace: Option<&SmpTrace>,
        bridge: Option<&mut kd_serial_bridge::KdSerialBridge>,
        trace: DrainTrace,
        context: DrainContext,
    ) -> Result<PendingDrainDelivery, SpiDeliveryError> {
        let mut platform = lock_platform(platform, smp_trace, 0, "cpu0 pre-run platform mutex");
        if let Some(bridge) = bridge {
            bridge.pump(&mut platform);
        }
        self.prepare_pending_delivery(&mut platform, mem, trace, context)
    }
    pub(crate) fn prepare_secondary_pre_run(
        &mut self,
        platform: &Arc<Mutex<VirtPlatform>>,
        mem: &mut dyn GuestMemoryMut,
        smp_trace: Option<&SmpTrace>,
        cpu: u64,
        trace: DrainTrace,
        context: DrainContext,
    ) -> Result<PendingDrainDelivery, SpiDeliveryError> {
        let mut platform =
            lock_platform(platform, smp_trace, cpu, "secondary pre-run platform mutex");
        self.prepare_pending_delivery(&mut platform, mem, trace, context)
    }
}
