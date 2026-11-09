//! The guest-agent host service runs on its own thread, off vCPU0.
//!
//! vCPU0 keeps only what needs the platform guard. Under it, `exchange` moves
//! the device's guest bytes into `rx` and sends what the service queued in
//! `tx`, in one call. The rest is `AgentConsoleHarness::advance`, which touches
//! neither the platform nor guest RAM, and runs here: share walks, PUT reads
//! and hashing, GET publishes with their F_FULLFSYNC, clipboard and drops. The
//! thread is the harness's only owner, so the pasteboard's `Cell` never has
//! two (`spawn` needs `AgentConsoleHarness: Send`). Readiness, the NAT's DNS
//! switches and VM pauses cross as atomics.
//!
//! The thread steps when there is news for it, never on every vCPU0 tick: when
//! `exchange` moved bytes or saw a DNS switch or desktop request, on a VM
//! pause, on a ctl append (ctl_wake.rs, through `ServiceSlot`), and otherwise
//! every `SERVICE_TICK`. That tick replaces the vCPU0 wake the agent used to
//! need (service_wake_interval.rs).

use super::*;
use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::virtio_console::MAX_HOST_INBOUND_LEN;
use std::sync::atomic::AtomicU64;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread::{JoinHandle, Thread};

/// The service's own cadence: its host-driven work (clipboard and share polls,
/// heartbeats, timeouts) runs at least this often.
const SERVICE_TICK: Duration = Duration::from_millis(250);

/// The current boot's service thread, for the probe-lifetime ctl watcher.
#[derive(Default)]
pub(super) struct ServiceSlot(Mutex<Option<Thread>>);

impl ServiceSlot {
    pub(super) fn unpark(&self) {
        if let Some(thread) = &*lock(&self.0) {
            thread.unpark();
        }
    }
}

#[derive(Default)]
struct Shared {
    /// Guest bytes vCPU0 drained, for the service.
    rx: Mutex<Vec<u8>>,
    /// Wire bytes the service queued, for vCPU0 to send.
    tx: Mutex<Vec<u8>>,
    desktop_ready: AtomicBool,
    dns_server_changes: AtomicU64,
    paused_ms: AtomicU64,
    stop: AtomicBool,
}

pub struct ServiceThread {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    slot: Arc<ServiceSlot>,
    /// The desktop mode and scale requests as `exchange` last saw them.
    desktop_seen: (u64, u32),
}

impl ServiceThread {
    /// Move `harness` onto its own thread, found through `slot` until dropped;
    /// `wake` asks vCPU0 to exchange soon.
    pub(super) fn spawn(
        harness: AgentConsoleHarness,
        slot: &Arc<ServiceSlot>,
        wake: impl Fn() + Send + 'static,
    ) -> Self {
        Self::start(harness, slot, wake, SERVICE_TICK)
    }

    fn start(
        harness: AgentConsoleHarness,
        slot: &Arc<ServiceSlot>,
        wake: impl Fn() + Send + 'static,
        tick: Duration,
    ) -> Self {
        let shared = Arc::<Shared>::default();
        let thread = std::thread::Builder::new()
            .name("bv-agent-service".into())
            .spawn({
                let shared = Arc::clone(&shared);
                move || serve(harness, &shared, &wake, tick)
            })
            .expect("spawn the agent service thread");
        *lock(&slot.0) = Some(thread.thread().clone());
        Self {
            shared,
            thread: Some(thread),
            slot: Arc::clone(slot),
            desktop_seen: (0, 0),
        }
    }

    /// Under the platform guard: hand the guest's bytes to the service and
    /// send what it queued.
    pub fn exchange(&mut self, platform: &mut VirtPlatform, mem: &mut dyn GuestMemoryMut) {
        self.exchange_with(&mut (platform, mem));
    }

    fn exchange_with(&mut self, wire: &mut impl AgentWire) {
        // While the service holds a full `rx`, the guest's bytes wait in the
        // device, which holds the guest back at the same bound (agent_inbound.rs).
        let drained = {
            let mut rx = lock(&self.shared.rx);
            let before = rx.len();
            if before < MAX_HOST_INBOUND_LEN {
                wire.drain_inbound_into(&mut rx);
            }
            rx.len() - before
        };
        // A full device stopped taking the guest's chains; take those left in the ring.
        if drained >= MAX_HOST_INBOUND_LEN {
            wire.poll();
        }
        let dns_switched = wire.dns_server_changes().is_some_and(|changes| {
            self.shared
                .dns_server_changes
                .swap(changes, Ordering::Relaxed)
                != changes
        });
        // Requests live input made in this tick, before the exchange.
        let desktop = (
            super::desktop_switch::requested_mode(),
            super::desktop_scale::requested_scale(),
        );
        let desktop_requested = std::mem::replace(&mut self.desktop_seen, desktop) != desktop;
        let sending = std::mem::take(&mut *lock(&self.shared.tx));
        if !sending.is_empty() {
            wire.send(&sending);
        }
        // Only news: a step for nothing would run the service's host work at
        // every automation tick, 120 times a second while the pointer moves.
        if drained > 0 || !sending.is_empty() || dns_switched || desktop_requested {
            self.unpark();
        }
    }

    /// The harness's `desktop_ready` as of its last step.
    pub fn desktop_ready(&self) -> bool {
        self.shared.desktop_ready.load(Ordering::Acquire)
    }

    /// Windows' clock is behind by `paused` (time_resync.rs).
    pub fn note_vm_pause(&self, paused: Duration) {
        let paused_ms = u64::try_from(paused.as_millis()).unwrap_or(u64::MAX);
        self.shared.paused_ms.fetch_add(paused_ms, Ordering::Relaxed);
        self.unpark();
    }

    fn unpark(&self) {
        if let Some(thread) = &self.thread {
            thread.thread().unpark();
        }
    }
}

impl Drop for ServiceThread {
    /// Returns once the harness is dropped: the next boot generation's harness
    /// starts after its ctl offset, pasteboard and share state are gone.
    fn drop(&mut self) {
        *lock(&self.slot.0) = None;
        self.shared.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            thread.thread().unpark();
            let _ = thread.join();
        }
    }
}

fn serve(mut harness: AgentConsoleHarness, shared: &Shared, wake: &dyn Fn(), tick: Duration) {
    raise_qos();
    loop {
        // Nothing else on this thread parks (channels are polled with try_recv),
        // so an unpark is never used up elsewhere.
        std::thread::park_timeout(tick);
        if shared.stop.load(Ordering::Acquire) {
            return;
        }
        let taken = harness.take_inbound(&mut lock(&shared.rx));
        harness.dns_flush.seen = shared.dns_server_changes.load(Ordering::Relaxed);
        let paused_ms = shared.paused_ms.swap(0, Ordering::Relaxed);
        if paused_ms > 0 {
            harness.note_vm_pause(Duration::from_millis(paused_ms));
        }
        let now = crate::boot_clocks::tick_times(&crate::boot_clocks::Live).agent;
        let mut queued = Vec::new();
        harness.advance(now, &mut |bytes| queued.extend_from_slice(bytes));
        let ready = harness.desktop_ready();
        let became_ready = ready && !shared.desktop_ready.swap(ready, Ordering::AcqRel);
        let sending = !queued.is_empty();
        if sending {
            lock(&shared.tx).append(&mut queued);
        }
        // vCPU0 exchanges on a wake: to send, to time READY (BOOT_TIMER), and
        // to drain the device a full `rx` held back.
        if sending || became_ready || taken >= MAX_HOST_INBOUND_LEN {
            wake();
        }
    }
}

/// User-interactive QoS: each lockstep round trip with the guest waits for this
/// thread to wake, as the HDA tick does (hda_service_tick_pacing.rs).
fn raise_qos() {
    use libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE;
    // SAFETY: sets only the calling thread's own QoS class; no pointers.
    let status = unsafe { libc::pthread_set_qos_class_self_np(QOS_CLASS_USER_INTERACTIVE, 0) };
    if status != 0 {
        eprintln!("agent service: QoS not raised: {status}");
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What `exchange` needs of the platform under its guard; tests model the device.
trait AgentWire {
    fn drain_inbound_into(&mut self, out: &mut Vec<u8>);
    fn poll(&mut self);
    fn send(&mut self, bytes: &[u8]);
    fn dns_server_changes(&self) -> Option<u64>;
}

impl AgentWire for (&mut VirtPlatform, &mut dyn GuestMemoryMut) {
    fn drain_inbound_into(&mut self, out: &mut Vec<u8>) {
        self.0.virtio_console_agent_drain_inbound_into(out);
    }
    fn poll(&mut self) {
        self.0.poll_virtio_console(&mut *self.1);
    }
    fn send(&mut self, bytes: &[u8]) {
        self.0.virtio_console_agent_send(bytes, &mut *self.1);
    }
    fn dns_server_changes(&self) -> Option<u64> {
        self.0
            .virtio_net_nat_stats()
            .map(|stats| stats.dns_server_changes)
    }
}

#[cfg(test)]
#[path = "service_thread_tests.rs"]
mod tests;
