//! The guest-agent host service runs on its own thread, off vCPU0.
//!
//! vCPU0 keeps only what needs guest RAM. Under the platform guard, `exchange`
//! moves the device's guest bytes into `rx` and sends what the service queued
//! in `tx`, in one call, and polls the ring a full device stopped taking. The
//! rest is `AgentConsoleHarness::advance`, which touches neither the platform
//! nor guest RAM, and runs here: share walks, PUT reads and hashing, GET
//! publishes with their F_FULLFSYNC, clipboard and drops. The thread is the
//! harness's only owner, so the pasteboard's `Cell` never has two (`spawn`
//! needs `AgentConsoleHarness: Send`). Readiness, the NAT's DNS switches and VM
//! pauses cross as atomics.
//!
//! A guest reply needs no vCPU0 exit: its notify exit already copied the bytes
//! into the device, so the reply hook unparks the service (ctl_wake.rs), and
//! each step first drains the device itself, briefly under the platform lock.
//! Both drains append to `rx` only under that lock, so the bytes keep the
//! device's order. Lock order: platform, then `rx`.
//!
//! The thread steps when there is news for it, never on every vCPU0 tick: a
//! guest reply, an `exchange` that moved bytes or saw a DNS switch or desktop
//! request, a VM pause, a ctl append (both hooks reach it through
//! `ServiceSlot`), and otherwise every `SERVICE_TICK`. That tick replaces the
//! vCPU0 wake the agent used to need (service_wake_interval.rs).

use super::*;
use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::virtio_console::MAX_HOST_INBOUND_LEN;
use std::sync::atomic::AtomicU64;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread::{JoinHandle, Thread};

/// The service's own cadence: its host-driven work (clipboard and share polls,
/// heartbeats, timeouts) runs at least this often.
const SERVICE_TICK: Duration = Duration::from_millis(250);

/// The current boot's service thread, for the probe-lifetime reply hook and ctl watcher.
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
    /// Guest bytes drained from the device (`drain`), for the service.
    rx: Mutex<Vec<u8>>,
    /// The service's drain found the device full: chains may wait in the ring
    /// for vCPU0's poll.
    ring_waiting: AtomicBool,
    /// Wire bytes the service queued, for vCPU0 to send.
    tx: Mutex<Vec<u8>>,
    desktop_ready: AtomicBool,
    dns_server_changes: AtomicU64,
    paused_ms: AtomicU64,
    stop: AtomicBool,
    guest_powered_off: AtomicBool,
}

pub struct ServiceThread {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    slot: Arc<ServiceSlot>,
    /// The desktop mode and scale requests as `exchange` last saw them.
    desktop_seen: (u64, u32),
}

impl ServiceThread {
    /// Move `harness` onto its own thread, which drains `platform`'s device and
    /// is found through `slot` until dropped; `wake` asks vCPU0 to exchange soon.
    pub(super) fn spawn(
        harness: AgentConsoleHarness,
        platform: Arc<Mutex<VirtPlatform>>,
        slot: &Arc<ServiceSlot>,
        wake: impl Fn() + Send + 'static,
    ) -> Self {
        Self::start(harness, platform, slot, wake, SERVICE_TICK)
    }

    fn start(
        harness: AgentConsoleHarness,
        device: impl AgentDevice,
        slot: &Arc<ServiceSlot>,
        wake: impl Fn() + Send + 'static,
        tick: Duration,
    ) -> Self {
        let shared = Arc::<Shared>::default();
        let thread = std::thread::Builder::new()
            .name("bv-agent-service".into())
            .spawn({
                let shared = Arc::clone(&shared);
                move || serve(harness, &shared, &device, &wake, tick)
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

    /// Under the platform guard: hand the guest's bytes to the service, poll
    /// the ring a full device left, and send what the service queued.
    pub fn exchange(&mut self, platform: &mut VirtPlatform, mem: &mut dyn GuestMemoryMut) {
        self.exchange_with(&mut (platform, mem));
    }

    fn exchange_with(&mut self, wire: &mut impl AgentWire) {
        let drained = drain(&self.shared.rx, |out| wire.drain_inbound_into(out));
        // A full device stopped taking the guest's chains, found here or by the
        // service's drain; take those left in the ring.
        let ring_waiting = self.shared.ring_waiting.swap(false, Ordering::AcqRel);
        if ring_waiting || drained >= MAX_HOST_INBOUND_LEN {
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

    /// Called only at the observed PSCI SYSTEM_OFF boundary, never reset or host teardown.
    pub fn guest_powered_off(&mut self) {
        self.shared.guest_powered_off.store(true, Ordering::Release);
        self.shared.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            thread.thread().unpark();
            let _ = thread.join();
        }
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

fn serve(
    mut harness: AgentConsoleHarness,
    shared: &Shared,
    device: &dyn AgentDevice,
    wake: &dyn Fn(),
    tick: Duration,
) {
    raise_qos();
    loop {
        // Nothing else on this thread parks (channels are polled with try_recv),
        // so an unpark is never used up elsewhere.
        std::thread::park_timeout(tick);
        if shared.stop.load(Ordering::Acquire) {
            if shared.guest_powered_off.load(Ordering::Acquire) {
                if let Some(share) = harness.share.as_mut() {
                    // A still-running piecewise publisher has not finished the share state.
                    if share.big.is_none() {
                        if let Some(file) = share.sync_file.as_mut() { file.finish_clean(&mut share.engine); }
                    }
                }
            }
            return;
        }
        // A guest reply's bytes are in the device already; no vCPU0 exit needed.
        if device.drain_into(&shared.rx) >= MAX_HOST_INBOUND_LEN {
            // Only vCPU0 polls the ring (guest RAM), and no notify will come.
            shared.ring_waiting.store(true, Ordering::Release);
            wake();
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

/// Every append to `rx`, by vCPU0 or the service, runs under the platform
/// guard, so the guest's bytes keep the device's order. Returns how many.
fn drain(rx: &Mutex<Vec<u8>>, drain_inbound: impl FnOnce(&mut Vec<u8>)) -> usize {
    let mut rx = lock(rx);
    let before = rx.len();
    // While the service holds a full `rx`, the guest's bytes wait in the
    // device, which holds the guest back at the same bound (agent_inbound.rs).
    if before < MAX_HOST_INBOUND_LEN {
        drain_inbound(&mut rx);
    }
    rx.len() - before
}

/// The device as the service reaches it; tests model the device and its lock.
trait AgentDevice: Send + 'static {
    /// Take the platform lock and `drain` into `rx` under it.
    fn drain_into(&self, rx: &Mutex<Vec<u8>>) -> usize;
}

impl AgentDevice for Arc<Mutex<VirtPlatform>> {
    fn drain_into(&self, rx: &Mutex<Vec<u8>>) -> usize {
        let mut platform = crate::lock_platform(self, None, 0, crate::HoldSite::AgentService);
        drain(rx, |out| platform.virtio_console_agent_drain_inbound_into(out))
    }
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
