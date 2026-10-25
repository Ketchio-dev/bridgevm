//! The `platform lock holds:` record: who held the platform lock each time
//! the HDA service tick found it held.
//!
//! The tick only `try_lock`s the platform and skips a tick that finds it held
//! (hda_service_tick.rs). T17 r94's first-ready session skipped 835 of 1199
//! ticks so and still had five HDA poll gaps over 100 ms (the longest 394 ms),
//! and nothing said which thread held the lock. Every platform-lock site that
//! can run while the tick does now holds the lock as a [`HeldPlatform`]: the
//! vCPUs' pre-run drains and data-abort exits, vCPU0's automation block and
//! UART preload, and the ramfb display exporter's config copy. The tick counts
//! each attempt that finds the lock held ([`PlatformHolds::note_busy`]); a hold
//! that such an attempt fell inside is charged to its holder as it ends, still
//! under the lock: one hold, the attempts it blocked, whether it lasted more
//! than 20 ms (Windows' whole 20 ms cyclic buffer in T17 r92) and its length.
//! A data-abort exit is charged by its target: NVMe, whose reads, writes and
//! flushes run inside the exit; the TPM, whose commands are swtpm round trips;
//! or any other device.
//!
//! - Cost: a hold reads one atomic and the clock as it starts and the atomic
//!   again as it ends. Only a hold that a tick found writes counters, so the
//!   5 ms tick bounds those writes to 200 a second. The storage is fixed, and
//!   nothing on the hold path allocates, locks, prints or does I/O.
//! - Attribution: holds never overlap, so neither do the windows in which they
//!   read the tick's count, and no attempt is charged twice. An attempt whose
//!   count lands outside every window (between an acquisition and its first
//!   read, a few instructions) stays `unattributed`.
//! - Output: the tick prints the record once, as its thread stops, right after
//!   its `hda service tick: stopped` line and before the final report. Like
//!   that line it is outside the bound post-footer host tail: no reader parses
//!   it, nothing gates on it, and a guest can print a look-alike line elsewhere
//!   in the run log. Nothing here is visible to the guest.
//! - Scope: a probe process runs one VM, so [`PLATFORM_HOLDS`] is its platform
//!   lock's; tests use their own [`PlatformHolds`].

use std::fmt::Write as _;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed, Ordering::SeqCst};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};
use std::time::{Duration, Instant};

use bridgevm_hvf::machine;
use bridgevm_hvf::pcie::NVME_BDF;

use crate::{lock_with_optional_trace, SmpTrace, VirtPlatform};

/// A charged hold longer than this is counted as long: a whole 20 ms cyclic
/// buffer without an HDA poll.
pub(crate) const LONG_HOLD_US: u64 = 20_000;

/// The probe's platform-lock holds.
pub(crate) static PLATFORM_HOLDS: PlatformHolds = PlatformHolds::new();

/// Lock the probe's platform at `site`, with wait tracing under SMP tracing.
pub(crate) fn lock_platform<'a>(
    platform: &'a Arc<Mutex<VirtPlatform>>,
    smp_trace: Option<&SmpTrace>,
    cpu: u64,
    site: HoldSite,
) -> HeldPlatform<'a> {
    PLATFORM_HOLDS.lock(platform, smp_trace, cpu, site)
}

/// `try_lock` the probe's platform at `site`.
pub(crate) fn try_lock_platform(platform: &Mutex<VirtPlatform>, site: HoldSite) -> TryHeld<'_> {
    PLATFORM_HOLDS.try_lock(platform, site)
}

/// What `try_lock_platform` returns.
pub(crate) type TryHeld<'a> = Result<HeldPlatform<'a>, TryLockError<MutexGuard<'a, VirtPlatform>>>;

/// Where the platform lock was taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HoldSite {
    /// vCPU0's UART preload, once per boot generation.
    UartPreload,
    /// A vCPU's pre-run drain.
    PreRun,
    /// A vCPU's data-abort exit to this guest-physical address.
    Mmio(u64),
    /// vCPU0's automation block: live input, the agent console, checkpoints.
    Automation,
    /// The ramfb display exporter's config copy.
    Display,
}

impl HoldSite {
    /// What a poisoned lock's panic names, as each site always has.
    pub(crate) fn context(self, cpu: u64) -> &'static str {
        match (self, cpu) {
            (Self::UartPreload, _) => "cpu0 UART preload platform mutex",
            (Self::PreRun, 0) => "cpu0 pre-run platform mutex",
            (Self::PreRun, _) => "secondary pre-run platform mutex",
            (Self::Mmio(_), 0) => "cpu0 data-abort platform mutex",
            (Self::Mmio(_), _) => "secondary data-abort platform mutex",
            (Self::Automation, _) => "cpu0 automation platform mutex",
            (Self::Display, _) => "ramfb display platform mutex",
        }
    }
}

/// The record's holders, in print order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Holder {
    PreRun,
    Nvme,
    Tpm,
    Mmio,
    Automation,
    /// The UART preload and the display exporter.
    Host,
}

impl Holder {
    pub(crate) const ALL: [Self; 6] = [
        Self::PreRun,
        Self::Nvme,
        Self::Tpm,
        Self::Mmio,
        Self::Automation,
        Self::Host,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::PreRun => "prerun",
            Self::Nvme => "nvme",
            Self::Tpm => "tpm",
            Self::Mmio => "mmio",
            Self::Automation => "automation",
            Self::Host => "host",
        }
    }

    /// Who held the lock at `site`; an MMIO exit by its target on `platform`.
    pub(crate) fn of(site: HoldSite, platform: &VirtPlatform) -> Self {
        match site {
            HoldSite::PreRun => Self::PreRun,
            HoldSite::Mmio(ipa) => mmio_holder(ipa, platform),
            HoldSite::Automation => Self::Automation,
            HoldSite::UartPreload | HoldSite::Display => Self::Host,
        }
    }
}

fn mmio_holder(ipa: u64, platform: &VirtPlatform) -> Holder {
    if matches!(machine::device_at(ipa), Some("tpm-tis" | "tpm-ppi")) {
        return Holder::Tpm;
    }
    match platform.pcie_mmio_target(ipa) {
        Some(target) if (target.bdf, target.bar_index) == (NVME_BDF, 0) => Holder::Nvme,
        _ => Holder::Mmio,
    }
}

/// One holder's charged holds.
struct Counts {
    holds: AtomicU64,
    busy: AtomicU64,
    long: AtomicU64,
    max_us: AtomicU64,
}

impl Counts {
    const fn new() -> Self {
        Self {
            holds: AtomicU64::new(0),
            busy: AtomicU64::new(0),
            long: AtomicU64::new(0),
            max_us: AtomicU64::new(0),
        }
    }
}

/// Holds of the platform lock that the HDA service tick found in its way.
pub(crate) struct PlatformHolds {
    /// Tick attempts that found the lock held.
    busy: AtomicU64,
    holders: [Counts; Holder::ALL.len()],
}

impl PlatformHolds {
    pub(crate) const fn new() -> Self {
        Self {
            busy: AtomicU64::new(0),
            holders: [const { Counts::new() }; Holder::ALL.len()],
        }
    }

    /// The tick's `try_lock` found the lock held.
    pub(crate) fn note_busy(&self) {
        self.busy.fetch_add(1, SeqCst);
    }

    /// `guard`, just taken at `site`, charged here as it drops.
    pub(crate) fn hold<'a>(
        &'a self,
        guard: MutexGuard<'a, VirtPlatform>,
        site: HoldSite,
    ) -> HeldPlatform<'a> {
        let busy = self.busy.load(SeqCst);
        HeldPlatform {
            guard,
            holds: self,
            site,
            busy,
            since: Instant::now(),
        }
    }

    /// Lock `platform` as `lock_platform` does, charged here.
    pub(crate) fn lock<'a>(
        &'a self,
        platform: &'a Mutex<VirtPlatform>,
        smp_trace: Option<&SmpTrace>,
        cpu: u64,
        site: HoldSite,
    ) -> HeldPlatform<'a> {
        let context = site.context(cpu);
        let guard = lock_with_optional_trace(platform, smp_trace, cpu, "platform mutex", context);
        self.hold(guard, site)
    }

    /// `try_lock` `platform`, charged here.
    pub(crate) fn try_lock<'a>(
        &'a self,
        platform: &'a Mutex<VirtPlatform>,
        site: HoldSite,
    ) -> TryHeld<'a> {
        platform.try_lock().map(|guard| self.hold(guard, site))
    }

    fn charge(&self, holder: Holder, busy: u64, held: Duration) {
        let counts = &self.holders[holder as usize];
        let held_us = u64::try_from(held.as_micros()).unwrap_or(u64::MAX);
        counts.holds.fetch_add(1, Relaxed);
        counts.busy.fetch_add(busy, Relaxed);
        if held_us > LONG_HOLD_US {
            counts.long.fetch_add(1, Relaxed);
        }
        counts.max_us.fetch_max(held_us, Relaxed);
    }

    /// The `platform lock holds:` record. Per holder: charged holds, the tick
    /// attempts they blocked, those longer than 20 ms, the longest in µs.
    pub(crate) fn record(&self) -> String {
        let busy = self.busy.load(SeqCst);
        let (mut charged, mut fields) = (0u64, String::new());
        for holder in Holder::ALL {
            let counts = &self.holders[holder as usize];
            let [holds, blocked, long, max_us] =
                [&counts.holds, &counts.busy, &counts.long, &counts.max_us]
                    .map(|n| n.load(Relaxed));
            charged = charged.saturating_add(blocked);
            let name = holder.name();
            let _ = write!(
                fields,
                " {name}_holds={holds} {name}_busy={blocked} {name}_gt_20ms={long} \
                 {name}_max_us={max_us}"
            );
        }
        let unattributed = busy.saturating_sub(charged);
        format!("platform lock holds: busy={busy} unattributed={unattributed}{fields}")
    }
}

/// The platform lock, held at a site. Dropping it charges the hold if a tick
/// found it, still under the lock, and then unlocks.
pub(crate) struct HeldPlatform<'a> {
    guard: MutexGuard<'a, VirtPlatform>,
    holds: &'a PlatformHolds,
    site: HoldSite,
    /// The tick's busy count as the hold began.
    busy: u64,
    since: Instant,
}

impl Deref for HeldPlatform<'_> {
    type Target = VirtPlatform;

    fn deref(&self) -> &VirtPlatform {
        &self.guard
    }
}

impl DerefMut for HeldPlatform<'_> {
    fn deref_mut(&mut self) -> &mut VirtPlatform {
        &mut self.guard
    }
}

impl Drop for HeldPlatform<'_> {
    fn drop(&mut self) {
        let busy = self.holds.busy.load(SeqCst).wrapping_sub(self.busy);
        if busy != 0 {
            let holder = Holder::of(self.site, &self.guard);
            self.holds.charge(holder, busy, self.since.elapsed());
        }
    }
}

#[cfg(test)]
#[path = "platform_lock_holds_tests.rs"]
pub(crate) mod tests;
