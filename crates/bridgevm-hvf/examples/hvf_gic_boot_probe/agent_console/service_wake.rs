//! Host-driven vCPU wake scheduling for resident services.

use super::*;

/// Periodic vCPU waker for service mode. The probe's main loop blocks inside
/// hv_vcpu_run, and with the in-kernel GIC an idle desktop guest can go MINUTES
/// without a userspace exit — so host-initiated service work (CLIPGET/LS polls,
/// pasteboard pushes, ctl commands, even stdout flushing) froze until the guest
/// happened to kick a virtqueue (live-observed as 5-minute log/service stalls).
/// A steady hv_vcpus_exit heartbeat bounds tick latency the same way
/// RamfbSampleLoop's sample tick does; the fired flag lets the exit dispatcher
/// tell this benign wake apart from the watchdog's EXIT_CANCELED.
pub struct ServiceWake {
    pub(crate) fired: Arc<AtomicBool>,
    pub(crate) started: bool,
}

impl ServiceWake {
    pub fn new() -> Self {
        Self {
            fired: Arc::new(AtomicBool::new(false)),
            started: false,
        }
    }
    /// Idempotently start the ticker thread. Runs for the probe's lifetime
    /// (the vCPU handle stays valid across the reboot loop's resets).
    pub fn ensure_started(&mut self, vcpu: HvVcpuT, interval: Duration) {
        if self.started {
            return;
        }
        self.started = true;
        let fired = Arc::clone(&self.fired);
        std::thread::spawn(move || loop {
            std::thread::sleep(interval);
            fired.store(true, Ordering::SeqCst);
            let v = vcpu;
            // SAFETY: Category 8 - `v` is the live HVF vCPU handle owned by
            // the probe loop, and the pointer is valid for this synchronous
            // call that requests one vCPU to leave `hv_vcpu_run`.
            unsafe { hv_vcpus_exit(&v, 1) };
        });
    }
    pub fn canceled_by_service_wake(&self, exit_reason: u32, watchdog_fired: &AtomicBool) -> bool {
        exit_reason == EXIT_CANCELED
            && self.fired.swap(false, Ordering::SeqCst)
            && !watchdog_fired.load(Ordering::SeqCst)
    }
}
