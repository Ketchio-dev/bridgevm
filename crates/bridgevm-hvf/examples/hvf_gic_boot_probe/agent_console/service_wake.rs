//! Host-driven vCPU0 wakes: a periodic ticker, and the agent service's wakes.

use super::service_thread::{ServiceSlot, ServiceThread};
use super::*;

/// vCPU0's wakes for host-driven work. The probe's main loop blocks inside
/// hv_vcpu_run, and with the in-kernel GIC an idle desktop guest can go MINUTES
/// without a userspace exit. Work that needs vCPU0 then waits for the guest to
/// kick a virtqueue (live-observed as 5-minute log/service stalls).
///
/// The agent service needs vCPU0 only for guest RAM: to send its bytes, and to
/// poll a ring a full device stopped taking; the service wakes it for those
/// (`spawn_service`). Guest replies, its host work and its tick are its own
/// (service_thread.rs), so it needs no ticker. The
/// ticker serves BOOT_TIMER, the PPM display export and a KD bridge
/// (service_wake_interval.rs). One `fired` flag lets the exit dispatcher tell
/// all these benign wakes apart from the watchdog's EXIT_CANCELED.
pub struct ServiceWake {
    pub(crate) fired: Arc<AtomicBool>,
    pub(crate) started: bool,
    agent_started: bool,
    /// The current boot's agent service thread, for the ctl watcher.
    service: Arc<ServiceSlot>,
}

impl ServiceWake {
    pub fn new() -> Self {
        Self {
            fired: Arc::new(AtomicBool::new(false)),
            started: false,
            agent_started: false,
            service: Arc::default(),
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
    /// This boot's agent service thread, on `platform`. The guest-reply hook
    /// and the ctl watcher are installed once, for the probe's lifetime.
    pub fn spawn_service(
        &mut self,
        vcpu: HvVcpuT,
        harness: AgentConsoleHarness,
        platform: &Arc<std::sync::Mutex<VirtPlatform>>,
    ) -> ServiceThread {
        if !std::mem::replace(&mut self.agent_started, true) {
            super::ctl_wake::start(&self.service);
        }
        let fired = Arc::clone(&self.fired);
        ServiceThread::spawn(harness, Arc::clone(platform), &self.service, move || {
            super::ctl_wake::request_wake(vcpu, &fired);
        })
    }
    pub fn canceled_by_service_wake(&self, exit_reason: u32, watchdog_fired: &AtomicBool) -> bool {
        exit_reason == EXIT_CANCELED
            && self.fired.swap(false, Ordering::SeqCst)
            && !watchdog_fired.load(Ordering::SeqCst)
    }
}
