//! SIGTERM ends the run through the normal final report.
//!
//! The supervisor sends TERM when its owner goes away (the app quit or
//! crashed) or a stop is forced. The handler only sets a flag. A watcher
//! thread, asleep on a kqueue until TERM, then asks vCPU0 to leave
//! `hv_vcpu_run`; the run loop sees the flag on that canceled exit and stops
//! like any terminal stop: secondary vCPUs are joined, then the UEFI vars and
//! every NVMe namespace are persisted (write-back ends with `F_FULLFSYNC`)
//! before the report. The supervisor's KILL stays the backstop for a run that
//! cannot finish.

use super::boot_progress::BootProgressWatchdog;
use crate::{hv_vcpus_exit, HvVcpuT};
use std::io::ErrorKind;
use std::mem::MaybeUninit;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Set by the SIGTERM handler and never cleared.
pub(crate) static HOST_TERM_REQUESTED: AtomicBool = AtomicBool::new(false);

/// The stop record of a run ended by SIGTERM.
pub(crate) const HOST_TERM_STOP_REASON: &str = "host stop requested (SIGTERM)";

/// The kick period after TERM, and the poll period without a kqueue.
const WATCH_INTERVAL: Duration = Duration::from_millis(10);

/// The longest one wait for TERM lasts before the watcher checks the run is
/// still armed. An idle guest otherwise costs this thread no wake-ups.
const ARMED_CHECK: Duration = Duration::from_secs(1);

extern "C" fn record_host_term(_: libc::c_int) {
    // An atomic store is async-signal-safe; nothing else happens here.
    HOST_TERM_REQUESTED.store(true, Ordering::SeqCst);
}

/// Replaces TERM's default action. If that fails, TERM still ends the
/// process immediately, as before.
pub(crate) fn install_host_term_handler() {
    // SAFETY: Category 8 - FFI boundary. The action is fully initialised
    // before sigaction reads it, and the handler only stores to an atomic.
    let status = unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = record_host_term as extern "C" fn(libc::c_int) as usize;
        action.sa_flags = libc::SA_RESTART;
        libc::sigemptyset(&mut action.sa_mask);
        libc::sigaction(libc::SIGTERM, &action, std::ptr::null_mut())
    };
    if std::env::var_os("BRIDGEVM_RESET_GENERATION").is_some() {
        std::thread::Builder::new().name("runner-exit-stop".into()).spawn(|| {
            if crate::parent_watch::wait_for_runner_exit() {
                // SAFETY: signal only this process; the installed handler routes it through persistence.
                unsafe { libc::kill(libc::getpid(), libc::SIGTERM) };
            }
        }).expect("runner lifetime watcher");
    }
    if status != 0 {
        println!(
            "HOST-TERM: handler unavailable ({}); TERM ends the run without persistence",
            std::io::Error::last_os_error()
        );
    }
}

pub(crate) fn start_host_term_watcher(vcpu: HvVcpuT, run: Arc<BootProgressWatchdog>) {
    let spawned = std::thread::Builder::new()
        .name("host-term-stop".into())
        .spawn(move || {
            let term = TermEvent::new();
            let kick = || {
                // SAFETY: Category 8 - `vcpu` remains a live probe-owned HVF
                // handle while the run is armed; this only asks its owning
                // thread to leave `hv_vcpu_run`.
                unsafe { hv_vcpus_exit(&vcpu, 1) };
            };
            watch(&HOST_TERM_REQUESTED, &run, || term.wait(), kick);
        });
    if let Err(error) = spawned {
        println!("HOST-TERM: watcher unavailable ({error}); TERM waits for the next exit");
    }
}

/// Blocks in `wait` until TERM, then kicks vCPU0 every few milliseconds until
/// the run is disarmed at its terminal stop, so a kick that lands between two
/// `hv_vcpu_run` calls, or during a guest reset, cannot be lost.
fn watch(
    requested: &AtomicBool,
    run: &BootProgressWatchdog,
    mut wait: impl FnMut() -> bool,
    mut kick: impl FnMut(),
) {
    // The kqueue can report TERM before the handler's store is visible here.
    let mut seen = false;
    while run.is_armed() && !seen && !requested.load(Ordering::SeqCst) {
        seen = wait();
    }
    if run.is_armed() {
        println!("HOST-TERM: SIGTERM received; ending run through final report");
    }
    while run.is_armed() {
        kick();
        std::thread::sleep(WATCH_INTERVAL);
    }
}

/// SIGTERM as a kqueue event. `EVFILT_SIGNAL` records the signal next to the
/// installed handler, so the watcher sleeps until TERM instead of polling.
struct TermEvent(Option<libc::c_int>);

impl TermEvent {
    fn new() -> Self {
        let change = libc::kevent {
            ident: libc::SIGTERM as usize,
            filter: libc::EVFILT_SIGNAL,
            flags: libc::EV_ADD,
            fflags: 0,
            data: 0,
            udata: null_mut(),
        };
        // SAFETY: Category 8 - plain kqueue/kevent calls; `change` outlives them.
        let queue = unsafe { libc::kqueue() };
        if queue >= 0 && unsafe { libc::kevent(queue, &change, 1, null_mut(), 0, null()) } == 0 {
            return Self(Some(queue));
        }
        println!(
            "HOST-TERM: no kqueue for TERM ({}); checking for it every 10 ms",
            std::io::Error::last_os_error()
        );
        if queue >= 0 {
            // SAFETY: Category 8 - the queue is owned here and not used again.
            unsafe { libc::close(queue) };
        }
        Self(None)
    }

    /// True when TERM was sent; false after `ARMED_CHECK` (`WATCH_INTERVAL`
    /// without a kqueue).
    fn wait(&self) -> bool {
        let Some(queue) = self.0 else {
            std::thread::sleep(WATCH_INTERVAL);
            return false;
        };
        let timeout = libc::timespec {
            tv_sec: ARMED_CHECK.as_secs() as libc::time_t,
            tv_nsec: 0,
        };
        let mut event = MaybeUninit::<libc::kevent>::uninit();
        // SAFETY: Category 8 - `event` has room for the one event asked for.
        let got = unsafe { libc::kevent(queue, null(), 0, event.as_mut_ptr(), 1, &timeout) };
        if got < 0 && std::io::Error::last_os_error().kind() != ErrorKind::Interrupted {
            std::thread::sleep(WATCH_INTERVAL); // never spin on a failing queue
        }
        got > 0
    }
}

impl Drop for TermEvent {
    fn drop(&mut self) {
        if let Some(queue) = self.0 {
            // SAFETY: Category 8 - the queue is owned here.
            unsafe { libc::close(queue) };
        }
    }
}

#[cfg(test)]
#[path = "host_term_stop_tests.rs"]
mod tests;
