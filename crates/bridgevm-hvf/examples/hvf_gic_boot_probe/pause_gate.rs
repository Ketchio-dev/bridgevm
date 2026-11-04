//! Pause and resume of the whole guest (`PAUSE` / `RESUME` on input.ctl).
//!
//! vCPU0 reads the line, asks every secondary to park before its next
//! `hv_vcpu_run` and waits until each one inside its run loop has parked;
//! then it waits in the run loop itself. No guest instruction runs until
//! RESUME. The guest counter is the host counter minus the vTimer offset
//! (hv_vcpu.h; every vCPU starts at offset 0), so resume adds the paused
//! ticks to one VM-wide offset, and each vCPU sets it on its own thread
//! before it runs again, as QEMU's `hvf_vm_state_change` does. The guest's
//! counter, and every timer deadline it programmed, continue where they
//! stopped. A vCPU created later takes the same offset before its first run.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::Duration;

/// A secondary that has not parked by then is stuck; the pause is abandoned.
const PARK_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) static GATE: PauseGate = PauseGate::new();

pub(crate) struct PauseGate {
    /// Checked before every run: set from the pause request until resume.
    requested: AtomicBool,
    /// The VM-wide vTimer offset and its version; a vCPU whose applied
    /// version differs sets the offset before it runs.
    offset: AtomicU64,
    version: AtomicU64,
    state: Mutex<GateState>,
    changed: Condvar,
}

struct GateState {
    /// Secondaries inside their run loop, and how many of those are parked.
    in_loop: usize,
    parked: usize,
    /// Both host counters when the last vCPU stopped; Some while paused.
    paused_at: Option<(u64, u64)>,
    /// The input watcher saw an append since the paused wait last looked.
    input: bool,
}

pub(crate) enum PauseOutcome {
    Paused,
    /// A secondary did not park in time; every vCPU runs on.
    Abandoned,
}

impl PauseGate {
    pub(crate) const fn new() -> Self {
        Self {
            requested: AtomicBool::new(false),
            offset: AtomicU64::new(0),
            version: AtomicU64::new(0),
            state: Mutex::new(GateState {
                in_loop: 0,
                parked: 0,
                paused_at: None,
                input: false,
            }),
            changed: Condvar::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, GateState> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Held by a secondary for as long as it is in its run loop.
    pub(crate) fn enter_run_loop(&self) -> InRunLoop<'_> {
        self.lock().in_loop += 1;
        InRunLoop(self)
    }

    /// Before every `hv_vcpu_run`, on the vCPU's own thread: park while a
    /// pause is requested, then take a changed offset. `set` sets this
    /// vCPU's offset and returns it read back.
    pub(crate) fn before_run(&self, applied: &mut u64, set: impl FnOnce(u64) -> u64) {
        if self.requested.load(Ordering::Acquire) {
            let mut state = self.lock();
            state.parked += 1;
            self.changed.notify_all();
            while self.requested.load(Ordering::Acquire) {
                state = self
                    .changed
                    .wait(state)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
            state.parked -= 1;
        }
        self.take_offset(applied, set);
    }

    /// vCPU0's half of `before_run`: it never parks here (it waits in the run loop).
    pub(crate) fn take_offset(&self, applied: &mut u64, set: impl FnOnce(u64) -> u64) {
        let version = self.version.load(Ordering::Acquire);
        if version == *applied {
            return;
        }
        let offset = self.offset.load(Ordering::Acquire);
        let read_back = set(offset);
        debug_assert_eq!(read_back, offset, "vTimer offset read back");
        *applied = version;
    }

    /// vCPU0, outside `hv_vcpu_run`: stop every secondary. `kick` makes each
    /// running secondary leave `hv_vcpu_run` (or a userspace-GIC WFI sleep);
    /// `now` reads the host counter once all are parked.
    pub(crate) fn pause(&self, kick: impl Fn(), counters: &impl HostCounters) -> PauseOutcome {
        let mut state = self.lock();
        if state.paused_at.is_some() {
            return PauseOutcome::Paused;
        }
        self.requested.store(true, Ordering::Release);
        drop(state);
        kick();
        state = self.lock();
        let deadline = std::time::Instant::now() + PARK_TIMEOUT;
        while state.parked < state.in_loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                self.requested.store(false, Ordering::Release);
                self.changed.notify_all();
                return PauseOutcome::Abandoned;
            }
            state = self
                .changed
                .wait_timeout(state, left)
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .0;
        }
        state.paused_at = Some((counters.mach(), counters.raw()));
        state.input = false;
        PauseOutcome::Paused
    }

    /// vCPU0: add the paused mach ticks to the VM-wide offset and release every
    /// secondary, which takes the offset before it runs. None when not paused.
    pub(crate) fn resume(&self, counters: &impl HostCounters) -> Option<Resumed> {
        let mut state = self.lock();
        let (mach_at, raw_at) = state.paused_at.take()?;
        let paused = counters.mach().saturating_sub(mach_at);
        let slept = counters.raw().saturating_sub(raw_at).saturating_sub(paused);
        let offset = self.offset.load(Ordering::Acquire).wrapping_add(paused);
        self.offset.store(offset, Ordering::Release);
        self.version.fetch_add(1, Ordering::AcqRel);
        self.requested.store(false, Ordering::Release);
        self.changed.notify_all();
        Some(Resumed { paused, offset, slept })
    }

    /// The VM-wide offset now in effect.
    pub(crate) fn offset(&self) -> u64 {
        self.offset.load(Ordering::Acquire)
    }

    /// The guest's count for host mach time `mach`: what the emulated
    /// CNTPCT_EL0 returns, so it stays with CNTVCT_EL0 across a pause.
    pub(crate) fn guest_count(&self, mach: u64) -> u64 {
        mach.wrapping_sub(self.offset())
    }

    /// A restored checkpoint set every vCPU's offset to the snapshot's; the
    /// next pause adds to that, not to zero. The vCPUs already have it.
    pub(crate) fn seed_offset(&self, offset: u64) {
        self.offset.store(offset, Ordering::Release);
    }

    /// True from a pause request until resume; device threads idle on it.
    pub(crate) fn paused(&self) -> bool {
        self.requested.load(Ordering::Relaxed)
    }

    #[cfg(test)]
    pub(crate) fn is_paused(&self) -> bool {
        self.lock().paused_at.is_some()
    }

    /// The input watcher: something was appended to input.ctl.
    pub(crate) fn notify_input(&self) {
        let mut state = self.lock();
        if state.paused_at.is_some() {
            state.input = true;
            self.changed.notify_all();
        }
    }

    /// vCPU0's paused wait: until input arrives or `timeout` passes.
    pub(crate) fn wait_for_input(&self, timeout: Duration) {
        let mut state = self.lock();
        if !state.input {
            state = self
                .changed
                .wait_timeout_while(state, timeout, |state| !state.input)
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .0;
        }
        state.input = false;
    }
}

/// Sets one vCPU's vTimer offset on its owning thread and returns it read back.
///
/// # Safety
/// `vcpu` is the calling thread's own live vCPU, outside `hv_vcpu_run`.
pub(crate) unsafe fn set_vtimer_offset(vcpu: crate::HvVcpuT, offset: u64) -> u64 {
    let set = crate::hv_vcpu_set_vtimer_offset(vcpu, offset);
    let mut read = 0u64;
    let get = crate::hv_vcpu_get_vtimer_offset(vcpu, &mut read);
    if set != 0 || get != 0 || read != offset {
        println!("BVPAUSE vtimer offset: set={set:#x} get={get:#x} read={read:#x} wanted={offset:#x}");
    }
    read
}

/// How vCPU0's paused wait ended.
pub(crate) enum PausedExit {
    /// Running again after this long paused (zero when the pause was abandoned).
    Resumed(Duration),
    /// RESET arrived while paused: resumed, and the boot ends as for a reset.
    Reset,
}

/// The host's two counters. HVF defines the guest counter as
/// `CNTVCT_EL0 = mach_absolute_time() - vtimer_offset`, so the pause is measured
/// on mach time. The host's raw `cntvct_el0` also counts while the Mac sleeps:
/// measured on it, a pause across a sleep would shift the offset too far and the
/// guest counter would go backwards. It only says how long the Mac slept.
pub(crate) trait HostCounters {
    fn mach(&self) -> u64;
    fn raw(&self) -> u64;
}

pub(crate) struct Host;

impl HostCounters for Host {
    fn mach(&self) -> u64 {
        crate::usgic_bridge::host_time_ticks()
    }

    fn raw(&self) -> u64 {
        crate::host_cntvct()
    }
}

/// What a resume did, in host counter ticks.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Resumed {
    pub(crate) paused: u64,
    pub(crate) offset: u64,
    /// Raw-counter ticks beyond the mach ticks: the Mac slept this long.
    pub(crate) slept: u64,
}

/// What vCPU0 does with the last PAUSE (`Some(true)`) or RESUME (`Some(false)`)
/// it read. Every one gets an answer: a RESUME that finds the VM running (a
/// PAUSE and RESUME read together, or a Restart while the pause was under way)
/// and a repeated PAUSE say what the VM already is, so the app never waits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Request {
    Pause,
    Resume,
    AlreadyPaused,
    AlreadyRunning,
    None,
}

pub(crate) fn request(line: Option<bool>, paused: bool) -> Request {
    match (line, paused) {
        (Some(true), false) => Request::Pause,
        (Some(true), true) => Request::AlreadyPaused,
        (Some(false), true) => Request::Resume,
        (Some(false), false) => Request::AlreadyRunning,
        (None, _) => Request::None,
    }
}

pub(crate) fn resumed_line(paused_ms: u128, offset: u64) -> String {
    format!("BVPAUSE resumed paused_ms={paused_ms} vtimer_offset={offset:#x}")
}

/// A RESUME for a running VM: nothing was paused.
pub(crate) fn answer_running() {
    println!("{}", resumed_line(0, GATE.offset()));
    flush_stdout();
}

/// vCPU0, outside `hv_vcpu_run`, after a PAUSE line: stop every vCPU, then
/// read input.ctl until RESUME or RESET. A host stop (SIGTERM when the app
/// went away, a diagnostic stop) resumes too, so the run loop ends the run as
/// it does for a running VM. Every exit resumes before it returns.
///
/// The diagnostic watchdog (`--watchdog-ms`) and the stall kill count wall
/// time, a pause included: a diagnostic run paused longer than its watchdog
/// ends. The app runs with `--no-watchdog`.
pub(crate) fn pause_and_wait(
    live_input: &mut crate::LiveInputController,
    secondaries: Option<&crate::SecondaryVcpuSet>,
    host_stop: impl Fn() -> bool,
) -> PausedExit {
    let kick = || {
        for control in secondaries.map_or(&[][..], |set| set.controls.as_slice()) {
            control.request_exit_if_published();
        }
        crate::usgic_bridge::kick_secondaries();
    };
    let started = std::time::Instant::now();
    if let PauseOutcome::Abandoned = GATE.pause(kick, &Host) {
        println!("BVPAUSE refused: a vCPU did not stop within {PARK_TIMEOUT:?}; the VM runs on");
        return PausedExit::Resumed(Duration::ZERO);
    }
    bridgevm_hvf::vm_clock::pause();
    println!("BVPAUSE paused");
    flush_stdout();
    let exit = loop {
        GATE.wait_for_input(Duration::from_millis(250));
        live_input.prepare_tick(std::time::Instant::now(), true);
        if live_input.take_host_reset() {
            break PausedExit::Reset;
        }
        match request(live_input.take_host_pause(), true) {
            Request::Resume => break PausedExit::Resumed(Duration::ZERO),
            Request::AlreadyPaused => {
                println!("BVPAUSE paused");
                flush_stdout();
            }
            _ => {}
        }
        if host_stop() {
            break PausedExit::Resumed(Duration::ZERO);
        }
    };
    // Device time first, so nothing a vCPU starts sees the pause as elapsed.
    let paused = bridgevm_hvf::vm_clock::resume();
    let resumed = GATE.resume(&Host).unwrap_or_default();
    println!(
        "{} host_slept_ticks={}",
        resumed_line(started.elapsed().as_millis(), resumed.offset),
        resumed.slept
    );
    flush_stdout();
    match exit {
        PausedExit::Resumed(_) => PausedExit::Resumed(paused),
        PausedExit::Reset => PausedExit::Reset,
    }
}

fn flush_stdout() {
    use std::io::Write as _;
    let _ = std::io::stdout().flush();
}

pub(crate) struct InRunLoop<'a>(&'a PauseGate);

impl Drop for InRunLoop<'_> {
    fn drop(&mut self) {
        let mut state = self.0.lock();
        state.in_loop -= 1;
        self.0.changed.notify_all();
    }
}

#[cfg(test)]
#[path = "pause_gate_tests.rs"]
mod tests;
