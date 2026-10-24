//! The host HDA service tick: guest audio DMA advances without vCPU exits.
//!
//! HDA playback DMA advances by the host time between polls of the controller,
//! and before this thread only a vCPU exit's drain polled it
//! (interrupt_delivery/prepare.rs). Exits come in bursts: T17 r92's stream ran
//! 5807 ms with 54 poll gaps of 20-50 ms, one of 50-100 ms and one of 216 ms
//! against Windows' 20 ms cyclic buffer, so the DMA re-read audio the guest had
//! not rewritten (30 repeat passes), the 100 ms cap dropped 116 ms, and IOCs
//! that one poll crossed shared one MSI (372 MSIs for 568 IOCs). This thread
//! polls the controller every 5 ms while the guest's stream runs.
//!
//! - Parked otherwise: it sleeps on its park token until the controller's
//!   lock-free stream-activity flag rises with SD0CTL.RUN (HdaStreamActivity),
//!   so a guest without a running stream costs no wakeup and no lock attempt.
//! - A tick only `try_lock`s the platform. A busy platform skips the tick: the
//!   holder is a vCPU, whose drain polls HDA itself, or a reset; the next poll
//!   still plays all the elapsed host time, up to the controller's 100 ms cap.
//!   Under the lock it polls with the host time and takes only HDA's MSI
//!   (`VirtPlatform::poll_hda_taking_msi`); it releases the lock and only then
//!   sends that MSI, through the vCPU drains' `deliver_pending_msix`.
//! - It never calls `hv_vcpus_exit` and holds no vCPU handle, so it adds no
//!   EXIT_CANCELED, each of which rolls the A1 swallow race (probe_runtime.rs).
//!   Under BRIDGEVM_USERSPACE_GIC an MSI send is itself a vCPU kick
//!   (usgic_bridge.rs), so there the tick stays off and exits poll as before.
//! - One per probe process, started outside the reboot loop beside
//!   RamfbDisplayThread and declared after the guest RAM backing. The probe
//!   drops it before the final report, so no tick runs during teardown, and
//!   unwinding joins it before RAM is unmapped. The reboot arm holds
//!   `quiesce()` across the GIC and platform resets, so an MSI taken in one
//!   boot generation never lands in the next.
//! - Deadline-paced like the ramfb exporter (a late tick resynchronises, it
//!   does not burst), at user-interactive QoS: this VMM's default-QoS timed
//!   waits woke 4-8 ms late at p50 in T17 r92.
//! - A failed MSI send ends the tick with one line; HDA then advances on vCPU
//!   exits as before, and their own failed delivery stops the run. The thread
//!   prints `hda service tick: stopped ...` with its counts as it ends.

use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, TryLockError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::hda::HdaStreamActivity;
use bridgevm_hvf::machine;
use bridgevm_hvf::msix::MsixMessage;

use crate::{deliver_pending_msix, HvReturn, MappedRam, VirtPlatform};
#[path = "hda_service_tick_counts.rs"]
mod counts;
pub(crate) use counts::TickCounts;

/// Half the 10 ms period Windows programs, so each IOC gets its own MSI.
pub(crate) const TICK_INTERVAL: Duration = Duration::from_millis(5);

/// What one tick found on the platform.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Service {
    /// A vCPU or a reset held the platform: skipped, no host time lost.
    Busy,
    /// Polled, with HDA's MSI when one became due.
    Polled(Option<MsixMessage>),
    /// A thread panicked holding the platform.
    Poisoned,
}

/// `try_lock` the platform, poll HDA at `now` taking only its MSI, unlock.
pub(crate) fn service_once(
    platform: &Mutex<VirtPlatform>,
    mem: &mut dyn GuestMemoryMut,
    now: Instant,
) -> Service {
    let mut platform = match platform.try_lock() {
        Ok(platform) => platform,
        Err(TryLockError::WouldBlock) => return Service::Busy,
        Err(TryLockError::Poisoned(_)) => return Service::Poisoned,
    };
    let message = platform.poll_hda_taking_msi(mem, now);
    drop(platform);
    Service::Polled(message)
}

/// The platform and GIC side of a tick, injectable for tests.
pub(crate) trait TickPort {
    fn service(&mut self, now: Instant) -> Service;
    /// Send one MSI; 0 is success.
    fn send(&mut self, message: MsixMessage) -> HvReturn;
}

/// The probe's port: its platform, its guest RAM mapping, and the GIC.
struct ProbePort<M> {
    platform: Arc<Mutex<VirtPlatform>>,
    memory: M,
}

impl<M: GuestMemoryMut> TickPort for ProbePort<M> {
    fn service(&mut self, now: Instant) -> Service {
        service_once(&self.platform, &mut self.memory, now)
    }

    fn send(&mut self, message: MsixMessage) -> HvReturn {
        match deliver_pending_msix(&[message], false) {
            Ok(_) => 0,
            Err(failure) => failure.status,
        }
    }
}

/// Why a tick ended the thread.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Halt {
    Poisoned,
    SendFailed(HvReturn),
}

/// One tick at `now`. The gate, held through the send, is what `quiesce()`
/// takes; the platform lock is held only inside `service`.
pub(crate) fn tick_once(
    port: &mut impl TickPort,
    gate: &Mutex<()>,
    now: Instant,
    counts: &mut TickCounts,
) -> Result<(), Halt> {
    let _gate = match gate.try_lock() {
        Ok(gate) => gate,
        Err(TryLockError::WouldBlock) => {
            counts.note_quiesced();
            return Ok(());
        }
        Err(TryLockError::Poisoned(gate)) => gate.into_inner(),
    };
    let service = port.service(now);
    counts.note_try(service == Service::Busy);
    match service {
        Service::Busy | Service::Polled(None) => {}
        Service::Poisoned => return Err(Halt::Poisoned),
        Service::Polled(Some(message)) => match port.send(message) {
            0 => counts.msis += 1,
            status => return Err(Halt::SendFailed(status)),
        },
    }
    Ok(())
}

#[derive(Default)]
struct Shared {
    stop: AtomicBool,
    gate: Mutex<()>,
}

/// The probe's HDA service tick. Start it once per process, outside the reboot
/// loop, after guest RAM is mapped and before any vCPU runs; dropping it stops
/// and joins the thread, so declare it after the RAM backing.
pub struct HdaServiceTick {
    shared: Arc<Shared>,
    handle: Option<JoinHandle<()>>,
}

impl HdaServiceTick {
    /// `ram`/`len` are the live guest-RAM mapping at `machine::RAM_BASE`.
    pub fn start(platform: &Arc<Mutex<VirtPlatform>>, ram: *mut u8, len: usize) -> Self {
        let (ram, owner) = (ram as usize, Arc::clone(platform));
        let userspace_gic = crate::usgic_bridge::usgic().is_some();
        Self::start_with(platform, userspace_gic, TICK_INTERVAL, move || ProbePort {
            platform: owner,
            memory: MappedRam {
                base: machine::RAM_BASE,
                ptr: ram as *mut u8,
                len,
            },
        })
    }

    pub(crate) fn start_with<P: TickPort>(
        platform: &Mutex<VirtPlatform>,
        userspace_gic: bool,
        interval: Duration,
        port: impl FnOnce() -> P + Send + 'static,
    ) -> Self {
        let activity = match platform.lock() {
            Ok(platform) => platform.hda_stream_activity(),
            Err(_) => {
                eprintln!("hda service tick: off, platform mutex poisoned");
                return Self::inert();
            }
        };
        let Some(activity) = activity else {
            return Self::inert();
        };
        if userspace_gic {
            println!("hda service tick: off, the userspace GIC sends MSIs as vCPU kicks");
            return Self::inert();
        }
        Self::spawn(activity, interval, port)
    }

    fn spawn<P: TickPort>(
        activity: Arc<HdaStreamActivity>,
        interval: Duration,
        port: impl FnOnce() -> P + Send + 'static,
    ) -> Self {
        let shared = Arc::new(Shared::default());
        let thread_shared = Arc::clone(&shared);
        let spawned = thread::Builder::new()
            .name("hda-service-tick".into())
            .spawn(move || {
                if !activity.register_current_thread() {
                    eprintln!("hda service tick: not started, another thread waits on HDA");
                    return;
                }
                raise_qos();
                println!(
                    "hda service tick: started interval_ms={}",
                    interval.as_millis()
                );
                let c = run(port(), &activity, &thread_shared, interval);
                println!("{}", c.stopped_record(interval));
            });
        match spawned {
            Ok(handle) => Self {
                shared,
                handle: Some(handle),
            },
            Err(error) => {
                eprintln!("hda service tick: thread not started: {error}");
                Self::inert()
            }
        }
    }

    fn inert() -> Self {
        Self {
            shared: Arc::default(),
            handle: None,
        }
    }

    /// Hold across a guest reset: no tick polls HDA or sends an MSI until the
    /// guard drops, and a tick in progress finishes first.
    pub fn quiesce(&self) -> MutexGuard<'_, ()> {
        self.shared
            .gate
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl Drop for HdaServiceTick {
    fn drop(&mut self) {
        self.shared.stop.store(true, SeqCst);
        if let Some(handle) = self.handle.take() {
            handle.thread().unpark();
            if handle.join().is_err() {
                eprintln!("hda service tick: thread panicked; HDA polls fall back to vCPU exits");
            }
        }
    }
}

/// Park while the stream is stopped; tick every `interval` while it runs.
fn run(
    mut port: impl TickPort,
    activity: &HdaStreamActivity,
    shared: &Shared,
    interval: Duration,
) -> TickCounts {
    let mut counts = TickCounts::default();
    loop {
        while !activity.running() {
            if shared.stop.load(SeqCst) {
                return counts;
            }
            thread::park();
        }
        counts.note_stream();
        let mut next = Instant::now() + interval;
        loop {
            if !sleep_until(next, &shared.stop) {
                return counts;
            }
            if !activity.running() {
                break;
            }
            let tick = Instant::now();
            let late = tick.saturating_duration_since(next);
            counts.max_late = counts.max_late.max(late);
            if let Err(halt) = tick_once(&mut port, &shared.gate, tick, &mut counts) {
                eprintln!("hda service tick: ended by {halt:?}; HDA polls fall back to vCPU exits");
                return counts;
            }
            next = (next + interval).max(Instant::now());
        }
    }
}

/// Park until `deadline`; false as soon as a stop is requested.
fn sleep_until(deadline: Instant, stop: &AtomicBool) -> bool {
    loop {
        if stop.load(SeqCst) {
            return false;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return true;
        }
        thread::park_timeout(left);
    }
}

/// User-interactive QoS for the calling thread, so its 5 ms wakes are prompt.
fn raise_qos() {
    use libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE;
    // SAFETY: sets only the calling thread's own QoS class; no pointers.
    let status = unsafe { libc::pthread_set_qos_class_self_np(QOS_CLASS_USER_INTERACTIVE, 0) };
    if status != 0 {
        eprintln!("hda service tick: QoS not raised: {status}");
    }
}

#[cfg(test)]
#[path = "hda_service_tick_busy_run_tests.rs"]
mod busy_run_tests;
#[cfg(test)]
#[path = "hda_service_tick_fixture.rs"]
mod fixture;
#[cfg(test)]
#[path = "hda_service_tick_replay_tests.rs"]
mod replay_tests;
#[cfg(test)]
#[path = "hda_service_tick_tests.rs"]
mod tests;
