//! Pending interrupt draining, delivery, and run-loop accounting.

use crate::*;

impl DrainLocation {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::PreRun => "pre-run",
            Self::DataAbort => "data-abort",
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct DrainContext {
    pub(crate) location: DrainLocation,
    pub(crate) exit: u64,
    pub(crate) pc: u64,
}

pub(crate) struct PendingDrainDelivery {
    pub(crate) context: DrainContext,
    pub(crate) spi: DeliveryCounts,
}

#[derive(Clone, Copy)]
pub(crate) struct DrainTrace {
    pub(crate) msix: bool,
    pub(crate) spi: bool,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct DeliveryCounts {
    pub(crate) drained: u64,
    pub(crate) success: u64,
    pub(crate) failure: u64,
}

impl DeliveryCounts {
    pub(crate) const fn has_deliveries(self) -> bool {
        self.drained != 0
    }
    pub(crate) fn record_status(&mut self, status: HvReturn) {
        self.drained += 1;
        if status == 0 {
            self.success += 1;
        } else {
            self.failure += 1;
        }
    }
    pub(crate) fn add(&mut self, other: Self) {
        self.drained += other.drained;
        self.success += other.success;
        self.failure += other.failure;
    }
}

pub(crate) struct RunLoopDrainStats {
    pub(crate) trace: bool,
    pub(crate) pre_run_attempts: u64,
    pub(crate) pre_run_skips: u64,
    pub(crate) data_abort_attempts: u64,
    pub(crate) msix: DeliveryCounts,
    pub(crate) spi: DeliveryCounts,
    pub(crate) last_drain_location: Option<&'static str>,
    pub(crate) last_drain_exit: Option<u64>,
    pub(crate) last_drain_pc: Option<u64>,
    pub(crate) last_drain_msix: DeliveryCounts,
    pub(crate) last_drain_spi: DeliveryCounts,
    pub(crate) last_nonzero_location: Option<&'static str>,
    pub(crate) last_nonzero_exit: Option<u64>,
    pub(crate) last_nonzero_pc: Option<u64>,
    pub(crate) pending_msix_scratch: Vec<MsixMessage>,
    pub(crate) pending_spi_scratch: Vec<(u32, bool)>,
}

impl RunLoopDrainStats {
    pub(crate) fn new(trace: bool) -> Self {
        Self {
            trace,
            pre_run_attempts: 0,
            pre_run_skips: 0,
            data_abort_attempts: 0,
            msix: DeliveryCounts {
                drained: 0,
                success: 0,
                failure: 0,
            },
            spi: DeliveryCounts {
                drained: 0,
                success: 0,
                failure: 0,
            },
            last_drain_location: None,
            last_drain_exit: None,
            last_drain_pc: None,
            last_drain_msix: DeliveryCounts {
                drained: 0,
                success: 0,
                failure: 0,
            },
            last_drain_spi: DeliveryCounts {
                drained: 0,
                success: 0,
                failure: 0,
            },
            last_nonzero_location: None,
            last_nonzero_exit: None,
            last_nonzero_pc: None,
            pending_msix_scratch: Vec::new(),
            pending_spi_scratch: Vec::new(),
        }
    }
    pub(crate) fn record_pre_run_skip(&mut self) {
        self.pre_run_skips += 1;
    }
    pub(crate) fn prepare_pending_delivery(
        &mut self,
        platform: &mut VirtPlatform,
        mem: &mut dyn GuestMemoryMut,
        trace: DrainTrace,
        context: DrainContext,
    ) -> PendingDrainDelivery {
        self.prepare_pending_delivery_inner(platform, mem, trace, context, true)
    }
    pub(crate) fn prepare_pending_delivery_after_mmio(
        &mut self,
        platform: &mut VirtPlatform,
        mem: &mut dyn GuestMemoryMut,
        trace: DrainTrace,
        context: DrainContext,
        post_drain: MmioPostDrain,
    ) -> PendingDrainDelivery {
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
    ) -> PendingDrainDelivery {
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
        debug_assert!(self.pending_msix_scratch.is_empty());
        self.pending_msix_scratch.clear();
        platform.drain_pending_msix_into(&mut self.pending_msix_scratch);
        PendingDrainDelivery { context, spi }
    }
    pub(crate) fn complete_pending_delivery(
        &mut self,
        pending: PendingDrainDelivery,
        trace: DrainTrace,
    ) {
        let context = pending.context;
        let spi = pending.spi;
        let msix = deliver_pending_msix(&self.pending_msix_scratch, trace.msix);
        self.pending_msix_scratch.clear();
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
    pub(crate) fn print_summary(&self) {
        let last_drain_exit = self
            .last_drain_exit
            .map_or_else(|| "<none>".to_string(), |exit| exit.to_string());
        let last_drain_pc = self
            .last_drain_pc
            .map_or_else(|| "<none>".to_string(), |pc| format!("{pc:#x}"));
        let last_nonzero_location = self.last_nonzero_location.unwrap_or("<none>");
        println!(
            "G004 IRQ drain attempts: pre-run={} data-abort={} pre-run-skipped={}",
            self.pre_run_attempts, self.data_abort_attempts, self.pre_run_skips
        );
        println!(
            "G004 IRQ drain MSI-X: drained={} success={} failure={}",
            self.msix.drained, self.msix.success, self.msix.failure
        );
        println!(
            "G004 IRQ drain SPI: drained={} success={} failure={}",
            self.spi.drained, self.spi.success, self.spi.failure
        );
        println!(
            "G004 IRQ drain last: exit={} pc={} last_nonzero_location={}",
            last_drain_exit, last_drain_pc, last_nonzero_location
        );
    }
    pub(crate) fn last_drain_was_empty(&self) -> Option<bool> {
        if self.last_drain_location.is_none() {
            None
        } else {
            Some(self.last_drain_msix.drained == 0 && self.last_drain_spi.drained == 0)
        }
    }
}

pub(crate) fn deliver_pending_msix(messages: &[MsixMessage], trace: bool) -> DeliveryCounts {
    let mut counts = DeliveryCounts::default();
    let trace = trace || trace_msix_enabled();
    for message in messages {
        let status = unsafe { hv_gic_send_msi(message.address, message.data) };
        counts.record_status(status);
        if trace {
            println!(
                "MSIX vector {} -> addr {:#x} intid {} status {status:#x}",
                message.vector, message.address, message.data
            );
        }
    }
    counts
}

pub(crate) fn deliver_pending_spis(
    platform: &mut VirtPlatform,
    scratch: &mut Vec<(u32, bool)>,
    trace: bool,
) -> DeliveryCounts {
    let mut counts = DeliveryCounts::default();
    let trace_msix = trace_msix_enabled();
    scratch.clear();
    platform.drain_pending_spi_levels_into(scratch);
    for &(intid, level) in scratch.iter() {
        let status = unsafe { hv_gic_set_spi(intid, level) };
        counts.record_status(status);
        if trace || (status != 0 && trace_msix) {
            println!("SPI intid {intid} level={level} status {status:#x}");
        }
    }
    scratch.clear();
    counts
}
