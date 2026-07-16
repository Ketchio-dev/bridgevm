//! Host vblank waker for `BRIDGEVM_VBLANK_HZ` pacing.
//!
//! The device parks the viogpu3d vsync NOP and the per-exit drain retires it
//! once its interval elapses — but the drain only runs when a vCPU exits, and
//! with the in-kernel GIC an idle/WFI guest can go seconds-to-minutes without
//! one (the earlier `BRIDGEVM_VBLANK_HZ=120` boot freeze: driver init waits on
//! NOP completions while exits sit at vtimer-only rates). This waker bounds
//! retire latency the way `ServiceWake` bounds service-tick latency: a thread
//! polls the device's lock-free `VblankWakeState` (never the platform mutex —
//! vCPU threads hold that almost continuously under 3D load, which is what
//! sank every lock-taking host pacer variant) and forces ONE vCPU exit only
//! when a parked NOP's deadline has actually passed. Under 3D load the guest
//! exits constantly and the drain retires on time, so this thread fires only
//! while the guest idles — worst case `BRIDGEVM_VBLANK_HZ` exits/sec, far
//! below the exit-storm regimes that crashed earlier attempts.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bridgevm_hvf::virtio_gpu::VblankWakeState;

use super::{hv_vcpus_exit, HvVcpuT, EXIT_CANCELED};

/// Idle-poll granularity while nothing is parked or a fire is outstanding.
const IDLE_POLL: Duration = Duration::from_millis(2);

pub struct VblankWake {
    fired: Arc<AtomicBool>,
    started: bool,
}

impl VblankWake {
    pub fn new() -> Self {
        Self {
            fired: Arc::new(AtomicBool::new(false)),
            started: false,
        }
    }

    /// Idempotently start the waker thread. Like `ServiceWake`, it must be
    /// created OUTSIDE the reboot loop and lives for the probe's lifetime;
    /// the vCPU handle stays valid across reboot-loop resets, and the device
    /// republishes `state` as parked NOPs come and go.
    pub fn ensure_started(&mut self, vcpu: HvVcpuT, state: Arc<VblankWakeState>) {
        if self.started {
            return;
        }
        self.started = true;
        let fired = Arc::clone(&self.fired);
        std::thread::spawn(move || loop {
            // Never fire while a previous fire is unconsumed: each forced exit
            // must map 1:1 onto one `fired` claim in the exit dispatcher, or
            // the surplus EXIT_CANCELED is misattributed to the watchdog and
            // stops the probe (live-observed: re-firing while the vCPU thread
            // was blocked in a ramfb checkpoint dump killed boots at the 30s
            // and 90s checkpoints).
            if fired.load(Ordering::SeqCst) {
                std::thread::sleep(IDLE_POLL);
                continue;
            }
            let Some(remaining) = state.time_to_deadline(Instant::now()) else {
                std::thread::sleep(IDLE_POLL);
                continue;
            };
            if !remaining.is_zero() {
                std::thread::sleep(remaining.min(IDLE_POLL));
                continue;
            }
            fired.store(true, Ordering::SeqCst);
            let v = vcpu;
            // SAFETY: Category 8 - `v` is the live HVF vCPU handle owned by
            // the probe loop, and the pointer is valid for this synchronous
            // call that requests one vCPU to leave `hv_vcpu_run`.
            unsafe { hv_vcpus_exit(&v, 1) };
        });
    }

    pub fn canceled_by_vblank_wake(&self, exit_reason: u32, watchdog_fired: &AtomicBool) -> bool {
        exit_reason == EXIT_CANCELED
            && self.fired.swap(false, Ordering::SeqCst)
            && !watchdog_fired.load(Ordering::SeqCst)
    }
}
