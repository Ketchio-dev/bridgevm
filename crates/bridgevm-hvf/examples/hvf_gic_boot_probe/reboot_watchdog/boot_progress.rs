//! Progress-rate watchdog for guest boot.
//!
//! The existing boot watchdog is a deadline: it fires once `watchdog_ms`
//! elapses, whatever the guest was doing. That makes a stalled boot
//! indistinguishable from a slow one until the whole budget is spent, and it
//! re-arms on every reboot, so its exit total only ever describes the final
//! boot generation.
//!
//! Two live runs show why that is not enough. A healthy boot and a stalled one
//! ended with almost the same exit total (69479 vs 69429) and in the same UEFI
//! idle image, but took 900 s and 2400 s to get there -- 77 vs 29 exits/s. The
//! stalled run spent its entire final generation making a third of the
//! progress. Rate separates them; totals do not.
//!
//! So this samples the exit counter on an interval and reports when the guest
//! falls below a floor for long enough to rule out a transient pause. It is
//! observation-only by design: it records a verdict rather than killing the
//! run, because a boot that is merely slow must not be turned into a failure.

use crate::{BootProgressKill, RebootPlan};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// How often the sampler wakes. Short enough that a stall is noticed well
/// inside a normal boot phase, long enough not to matter next to a vCPU exit.
pub(crate) const PROGRESS_SAMPLE_INTERVAL: Duration = Duration::from_millis(1000);

/// Exits per second below which the guest counts as not progressing.
///
/// Measured, not guessed: the healthy run averaged 77 exits/s over its whole
/// budget and the stalled one 29. A floor of 5 sits an order of magnitude under
/// the stalled figure, so it only trips on a guest that has effectively
/// stopped, never on one that is merely slow.
pub(crate) const DEFAULT_MIN_EXITS_PER_SEC: u64 = 5;

/// How long the guest must stay under the floor before it is called a stall.
/// Long enough to cover a slow phase transition and a reboot.
pub(crate) const DEFAULT_STALL_AFTER: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BootProgressRecord {
    /// Wall time the guest has been under the floor.
    pub(crate) stalled_for: Duration,
    /// Exits observed across that window.
    pub(crate) exits_in_window: u64,
    /// Total exits when the stall was declared.
    pub(crate) total_exits: u64,
    /// Reboots completed before the stall.
    pub(crate) reboots: u64,
}

impl BootProgressRecord {
    /// One line, grep-able, same shape as the fence-poll watchdog.
    ///
    /// `reboots` is load-bearing when reading these after the fact: firstboot
    /// advances by rebooting between stages, so a stall at a low reboot count
    /// is an early-stage failure, not a late one.
    pub(crate) fn format(&self, label: &str) -> String {
        format!(
            "{label}: boot-progress watchdog stalled_for_ms={} exits_in_window={} \
             total_exits={} reboots={} suspect={}",
            self.stalled_for.as_millis(),
            self.exits_in_window,
            self.total_exits,
            self.reboots,
            if self.exits_in_window == 0 {
                "guest-not-running"
            } else if self.reboots == 0 {
                "stalled-before-first-reboot"
            } else {
                "stalled-between-boot-stages"
            }
        )
    }
}

/// Decide whether a sampling window counts as a stall.
///
/// Split out from the thread so the policy is testable without spawning
/// anything or waiting on real time.
pub(crate) fn is_stalled(exits_in_window: u64, window: Duration, min_exits_per_sec: u64) -> bool {
    let secs = window.as_secs_f64();
    if secs <= 0.0 {
        return false;
    }
    (exits_in_window as f64 / secs) < min_exits_per_sec as f64
}

/// Shared state sampled by the watchdog thread.
///
/// Counters are published by the run loop; the watchdog only reads them, so
/// this never blocks the vCPU and needs no lock.
#[derive(Debug)]
pub(crate) struct BootProgressWatchdog {
    exits: AtomicU64,
    reboots: AtomicU64,
    armed: AtomicBool,
    fired: AtomicBool,
}

impl BootProgressWatchdog {
    pub(crate) fn new() -> Self {
        Self {
            exits: AtomicU64::new(0),
            reboots: AtomicU64::new(0),
            armed: AtomicBool::new(true),
            fired: AtomicBool::new(false),
        }
    }

    pub(crate) fn record_exit(&self) {
        self.exits.fetch_add(1, Ordering::Relaxed);
    }

    /// Count an exit in both the caller's local tally and the watchdog's,
    /// so the two cannot drift apart.
    pub(crate) fn count_exit(&self, exits: u64) -> u64 {
        self.record_exit();
        exits + 1
    }

    pub(crate) fn record_reboot(&self) {
        self.reboots.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn exits(&self) -> u64 {
        self.exits.load(Ordering::Relaxed)
    }

    pub(crate) fn reboots(&self) -> u64 {
        self.reboots.load(Ordering::Relaxed)
    }

    /// Stop sampling. Called at shutdown so the thread cannot report a stall
    /// against a guest that has legitimately powered off.
    pub(crate) fn disarm(&self) {
        self.armed.store(false, Ordering::SeqCst);
    }

    pub(crate) fn is_armed(&self) -> bool {
        self.armed.load(Ordering::SeqCst)
    }

    /// Latch so a persistent stall reports once rather than every interval.
    pub(crate) fn mark_fired(&self) -> bool {
        !self.fired.swap(true, Ordering::SeqCst)
    }
}

impl Default for BootProgressWatchdog {
    fn default() -> Self {
        Self::new()
    }
}

/// Sample `watchdog` until it is disarmed, reporting the first sustained stall.
pub(crate) fn spawn_boot_progress_watchdog(
    watchdog: Arc<BootProgressWatchdog>,
    min_exits_per_sec: u64,
    stall_after: Duration,
    label: &'static str,
    kill: Option<BootProgressKill>,
) {
    std::thread::spawn(move || {
        let mut under_floor = Duration::ZERO;
        let mut window_start_exits = watchdog.exits();
        while watchdog.is_armed() {
            std::thread::sleep(PROGRESS_SAMPLE_INTERVAL);
            if !watchdog.is_armed() {
                return;
            }
            let now_exits = watchdog.exits();
            let delta = now_exits.saturating_sub(window_start_exits);
            if is_stalled(delta, PROGRESS_SAMPLE_INTERVAL, min_exits_per_sec) {
                under_floor += PROGRESS_SAMPLE_INTERVAL;
            } else {
                under_floor = Duration::ZERO;
            }
            window_start_exits = now_exits;

            if under_floor >= stall_after && watchdog.mark_fired() {
                let record = BootProgressRecord {
                    stalled_for: under_floor,
                    exits_in_window: delta,
                    total_exits: now_exits,
                    reboots: watchdog.reboots(),
                };
                println!("{}", record.format(label));
                if let Some(kill) = kill {
                    println!("{label}: boot-progress watchdog ending run (kill mode)");
                    kill.fire();
                    return;
                }
            }
        }
    });
}

#[cfg(test)]
#[path = "boot_progress_tests.rs"]
mod boot_progress_tests;

/// Reboot policy plus both boot watchdogs, established once per run.
///
/// Grouped because they are one decision: how many reboots to allow, which
/// generation the deadline watchdog belongs to, and the run-wide progress
/// sampler that outlives every generation.
pub(crate) fn setup_boot_supervision(
    watchdog_enabled: bool,
    kill: Option<BootProgressKill>,
) -> (RebootPlan, Arc<AtomicU64>, Arc<BootProgressWatchdog>) {
    let reboot_plan = RebootPlan::from_env();
    println!("PSCI SYSTEM_RESET max reboots: {}", reboot_plan.max_reboots);
    let watchdog_generation = Arc::new(AtomicU64::new(0));
    let boot_progress = start_boot_progress_watchdog(watchdog_enabled, kill);
    (reboot_plan, watchdog_generation, boot_progress)
}

/// Build the run-wide watchdog and start sampling.
///
/// One instance per run, deliberately not per boot generation: the deadline
/// watchdog re-arms on every reboot, which is exactly why its exit total cannot
/// tell a stalled boot from a slow one.
pub(crate) fn start_boot_progress_watchdog(
    enabled: bool,
    kill: Option<BootProgressKill>,
) -> Arc<BootProgressWatchdog> {
    let watchdog = Arc::new(BootProgressWatchdog::new());
    if enabled {
        spawn_boot_progress_watchdog(
            Arc::clone(&watchdog),
            DEFAULT_MIN_EXITS_PER_SEC,
            DEFAULT_STALL_AFTER,
            "probe",
            kill,
        );
    }
    watchdog
}
