//! One generation-tagged cancellation source for the probe.
//!
//! Seven independent places used to call `hv_vcpus_exit` directly: the ramfb
//! sampler, the vblank pacer, the agent-console heartbeat, the setup-input
//! injector, two reboot watchdogs and the secondary-vCPU forwarder. None of
//! them knew about the others, so the run loop saw cancellations it could not
//! attribute -- the "surplus canceled" exits that correlate with the A1 stall.
//!
//! This module is the bookkeeping half of the fix. A waker publishes *why* it
//! wants the vCPU back before cancelling; the run loop claims those reasons on
//! every `EXIT_CANCELED`. A cancel that claims nothing is genuinely surplus,
//! and now says so with a number instead of a guess.
//!
//! Generations exist because a reboot recreates the vCPU. A cancellation
//! requested for the previous generation must never be counted as an answer
//! for the new one.

use std::sync::atomic::{AtomicU64, Ordering};

/// Why a wake was requested. One bit each, so a single atomic carries the set
/// of reasons pending at the moment the vCPU came back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u64)]
pub(crate) enum WakeReason {
    RamfbSample = 1 << 0,
    Vblank = 1 << 1,
    AgentConsole = 1 << 2,
    SetupInput = 1 << 3,
    RebootWatchdog = 1 << 4,
    BootProgressWatchdog = 1 << 5,
    SecondaryForward = 1 << 6,
}

impl WakeReason {
    pub(crate) const ALL: [WakeReason; 7] = [
        WakeReason::RamfbSample,
        WakeReason::Vblank,
        WakeReason::AgentConsole,
        WakeReason::SetupInput,
        WakeReason::RebootWatchdog,
        WakeReason::BootProgressWatchdog,
        WakeReason::SecondaryForward,
    ];

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            WakeReason::RamfbSample => "ramfb-sample",
            WakeReason::Vblank => "vblank",
            WakeReason::AgentConsole => "agent-console",
            WakeReason::SetupInput => "setup-input",
            WakeReason::RebootWatchdog => "reboot-watchdog",
            WakeReason::BootProgressWatchdog => "boot-progress-watchdog",
            WakeReason::SecondaryForward => "secondary-forward",
        }
    }
}

/// What a claimed cancellation turned out to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CancelClaim {
    /// At least one waker had published a reason; the bitset is returned.
    Claimed(u64),
    /// Nobody had asked for this wake. This is the exit that used to be
    /// attributed by guesswork.
    Surplus,
    /// The request belonged to an earlier vCPU generation and was discarded.
    StaleGeneration,
}

impl CancelClaim {
    pub(crate) fn reasons(self) -> Vec<WakeReason> {
        let CancelClaim::Claimed(bits) = self else {
            return Vec::new();
        };
        WakeReason::ALL
            .into_iter()
            .filter(|reason| bits & (*reason as u64) != 0)
            .collect()
    }
}

/// Coordinates every cancellation of one vCPU.
#[derive(Debug)]
pub(crate) struct WakeCoordinator {
    pending: AtomicU64,
    generation: AtomicU64,
    /// Generation the pending bits belong to.
    pending_generation: AtomicU64,
    requests: AtomicU64,
    claimed: AtomicU64,
    surplus: AtomicU64,
    stale: AtomicU64,
}

impl Default for WakeCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl WakeCoordinator {
    pub(crate) fn new() -> Self {
        Self {
            pending: AtomicU64::new(0),
            generation: AtomicU64::new(0),
            pending_generation: AtomicU64::new(0),
            requests: AtomicU64::new(0),
            claimed: AtomicU64::new(0),
            surplus: AtomicU64::new(0),
            stale: AtomicU64::new(0),
        }
    }

    /// Current vCPU generation. Bumped by `begin_generation` on reboot.
    pub(crate) fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    /// Start a new vCPU generation, discarding anything the old one requested.
    pub(crate) fn begin_generation(&self) -> u64 {
        let next = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.pending.store(0, Ordering::SeqCst);
        self.pending_generation.store(next, Ordering::SeqCst);
        next
    }

    /// Publish a reason before cancelling. Returns the generation the request
    /// was recorded against, which the caller passes to `hv_vcpus_exit` time.
    pub(crate) fn request(&self, reason: WakeReason) -> u64 {
        let generation = self.generation.load(Ordering::SeqCst);
        // Publishing must happen before the cancellation is issued, otherwise
        // the run loop can wake and find nothing to claim.
        let pending_generation = self.pending_generation.swap(generation, Ordering::SeqCst);
        if pending_generation != generation {
            self.pending.store(0, Ordering::SeqCst);
        }
        self.pending.fetch_or(reason as u64, Ordering::SeqCst);
        self.requests.fetch_add(1, Ordering::SeqCst);
        generation
    }

    /// Called by the run loop on every `EXIT_CANCELED`.
    pub(crate) fn claim(&self) -> CancelClaim {
        let generation = self.generation.load(Ordering::SeqCst);
        let bits = self.pending.swap(0, Ordering::SeqCst);
        if bits == 0 {
            self.surplus.fetch_add(1, Ordering::SeqCst);
            return CancelClaim::Surplus;
        }
        if self.pending_generation.load(Ordering::SeqCst) != generation {
            self.stale.fetch_add(1, Ordering::SeqCst);
            return CancelClaim::StaleGeneration;
        }
        self.claimed.fetch_add(1, Ordering::SeqCst);
        CancelClaim::Claimed(bits)
    }

    /// Counters for the final report: requested, claimed, surplus, stale.
    pub(crate) fn counters(&self) -> (u64, u64, u64, u64) {
        (
            self.requests.load(Ordering::SeqCst),
            self.claimed.load(Ordering::SeqCst),
            self.surplus.load(Ordering::SeqCst),
            self.stale.load(Ordering::SeqCst),
        )
    }
}

#[cfg(test)]
#[path = "wake_coordinator_tests.rs"]
mod tests;

/// Final-report lines attributing every cancellation this generation saw.
///
/// Prints after the run has stopped. `surplus` here is measured, not inferred:
/// it counts cancellations that arrived with no waker having asked for one.
pub(crate) fn report_wake_attribution(coordinator: &WakeCoordinator, claims: &[CancelClaim]) {
    let (requests, claimed, surplus, stale) = coordinator.counters();
    println!(
        "WAKE ATTRIBUTION: generation={} requests={requests} claimed={claimed} \
         surplus={surplus} stale={stale} observed_cancels={}",
        coordinator.generation(),
        claims.len()
    );
    let mut per_reason = [0u64; WakeReason::ALL.len()];
    for claim in claims {
        for reason in claim.reasons() {
            let index = WakeReason::ALL
                .iter()
                .position(|candidate| *candidate == reason)
                .expect("every reason is in ALL");
            per_reason[index] += 1;
        }
    }
    for (reason, count) in WakeReason::ALL.into_iter().zip(per_reason) {
        if count != 0 {
            println!("WAKE ATTRIBUTION: {} claimed {count}", reason.as_str());
        }
    }
}
