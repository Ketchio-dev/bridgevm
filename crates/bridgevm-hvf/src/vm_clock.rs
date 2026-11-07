//! The VM's host clock, with paused time left out.
//!
//! A paused VM (input.ctl `PAUSE`) runs no guest code, and its guest counter
//! continues where it stopped. Devices that pace guest-visible work by host
//! time read this clock instead of `Instant::now()`, so a pause looks to them
//! as it does to the guest: no time passed. That covers HDA DMA and xHCI report
//! pacing (`VirtPlatform::set_host_now`), the NAT's TCP retransmit and idle
//! timers, and the probe's agent console timeouts. Without it, the first tick
//! after a long pause would see minutes pass at once: HDA would skip ahead, the
//! NAT would retransmit and evict every guest connection, and the agent console
//! would declare in-flight requests overdue.
//!
//! In unit tests each thread has its own clock, so a test that pauses cannot
//! stop time for tests running beside it.

use std::time::{Duration, Instant};

#[derive(Clone, Copy, Default)]
struct State {
    paused_total: Duration,
    paused_since: Option<Instant>,
}

impl State {
    fn active_at(&self, now: Instant) -> Instant {
        let at = self.paused_since.map_or(now, |since| since.min(now));
        at.checked_sub(self.paused_total).unwrap_or(at)
    }
}

#[cfg(not(test))]
fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    static STATE: std::sync::Mutex<State> = std::sync::Mutex::new(State {
        paused_total: Duration::ZERO,
        paused_since: None,
    });
    f(&mut STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()))
}

#[cfg(test)]
fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    thread_local! {
        static STATE: std::cell::RefCell<State> = std::cell::RefCell::new(State::default());
    }
    STATE.with(|state| f(&mut state.borrow_mut()))
}

/// The VM's time now: host time minus every completed pause, and held at the
/// pause's start while paused.
pub fn active_now() -> Instant {
    active_at(Instant::now())
}

/// The VM's time at host time `now`.
pub fn active_at(now: Instant) -> Instant {
    with_state(|state| state.active_at(now))
}

/// The host time at which VM time reaches `at`, for a host wake: `at` plus
/// every completed pause (and the current one, as if it ended now).
pub fn host_at(at: Instant) -> Instant {
    let now = Instant::now();
    with_state(|state| {
        let current = state
            .paused_since
            .map_or(Duration::ZERO, |since| now.saturating_duration_since(since));
        at + state.paused_total + current
    })
}

/// The guest stopped (every vCPU parked).
pub fn pause() {
    let now = Instant::now();
    with_state(|state| {
        state.paused_since.get_or_insert(now);
    });
}

/// The guest runs again; returns how long it was paused.
pub fn resume() -> Duration {
    let now = Instant::now();
    with_state(|state| {
        let Some(since) = state.paused_since.take() else {
            return Duration::ZERO;
        };
        let paused = now.saturating_duration_since(since);
        state.paused_total += paused;
        paused
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pause_is_left_out_and_held_while_paused() {
        let wall = Instant::now();
        let start = active_now();
        std::thread::sleep(Duration::from_millis(20));
        pause();
        let held = active_now();
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(active_now(), held, "VM time stands still while paused");
        let paused = resume();
        assert!(paused >= Duration::from_millis(200));
        let after = active_now();
        assert!(after >= held);
        // Against the measured wall time, so a loaded runner that runs the
        // sleeps late cannot fail it (a fixed 150 ms bound did, on CI).
        assert!(
            after.duration_since(start) <= wall.elapsed() - paused,
            "the 200 ms pause is not VM time"
        );
        assert_eq!(resume(), Duration::ZERO, "not paused");
    }

    /// HDA's WALCLK counts VM time from the controller's reset release.
    #[test]
    fn hda_walclk_leaves_out_a_pause() {
        let mut hda = crate::hda::HdaController::new();
        hda.write_gctl(crate::hda::GCTL_CRST);
        pause();
        std::thread::sleep(Duration::from_millis(200));
        resume();
        let ticks = hda.wallclk();
        assert!(
            ticks < 24_000 * 150,
            "WALCLK counted the 200 ms pause: {ticks} ticks"
        );
    }

    /// A paced report's deadline is in VM time; the host wake that delivers it
    /// sleeps in host time, so after a pause the two differ by the pause.
    #[test]
    fn a_vm_deadline_maps_to_the_host_time_it_falls_due() {
        pause();
        std::thread::sleep(Duration::from_millis(200));
        resume();
        let due = active_now() + Duration::from_millis(50);
        let host = host_at(due);
        let wait = host.saturating_duration_since(Instant::now());
        assert!(
            wait > Duration::from_millis(20) && wait <= Duration::from_millis(50),
            "a deadline 50 ms ahead in VM time is {wait:?} ahead in host time"
        );
    }

    /// HDA DMA and xHCI report pacing read the platform's host time: a pause
    /// must not reach them as elapsed time.
    #[test]
    fn the_platform_host_time_leaves_out_a_pause() {
        let mut platform = crate::platform_virt::VirtPlatform::new(Default::default());
        let before = Instant::now();
        platform.set_host_now(before);
        let first = platform.host_now.unwrap();
        pause();
        std::thread::sleep(Duration::from_millis(200));
        resume();
        platform.set_host_now(Instant::now());
        let gap = platform.host_now.unwrap().duration_since(first);
        assert!(
            gap < Duration::from_millis(150),
            "the 200 ms pause reached the platform: {gap:?}"
        );
    }
}
