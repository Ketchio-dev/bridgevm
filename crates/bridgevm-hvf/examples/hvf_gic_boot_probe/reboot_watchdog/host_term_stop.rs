//! SIGTERM ends the run through the normal final report.
//!
//! The supervisor sends TERM when its owner goes away (the app quit or
//! crashed) or a stop is forced. The handler only sets a flag. A watcher
//! thread then asks vCPU0 to leave `hv_vcpu_run`; the run loop sees the flag
//! on that canceled exit and stops like any terminal stop: secondary vCPUs
//! are joined, then the UEFI vars and every NVMe namespace are persisted
//! (write-back ends with `F_FULLFSYNC`) before the report. The supervisor's
//! KILL stays the backstop for a run that cannot finish.

use super::boot_progress::BootProgressWatchdog;
use crate::{hv_vcpus_exit, HvVcpuT};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Set by the SIGTERM handler and never cleared.
pub(crate) static HOST_TERM_REQUESTED: AtomicBool = AtomicBool::new(false);

/// The stop record of a run ended by SIGTERM.
pub(crate) const HOST_TERM_STOP_REASON: &str = "host stop requested (SIGTERM)";

const WATCH_INTERVAL: Duration = Duration::from_millis(10);

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
    if status != 0 {
        println!(
            "HOST-TERM: handler unavailable ({}); TERM ends the run without persistence",
            std::io::Error::last_os_error()
        );
    }
}

/// Wakes vCPU0 every few milliseconds once TERM arrives, until the run is
/// disarmed at its terminal stop, so a wake that lands between two
/// `hv_vcpu_run` calls, or during a guest reset, cannot be lost.
pub(crate) fn start_host_term_watcher(vcpu: HvVcpuT, run: Arc<BootProgressWatchdog>) {
    let spawned = std::thread::Builder::new()
        .name("host-term-stop".into())
        .spawn(move || {
            let mut reported = false;
            while run.is_armed() {
                if HOST_TERM_REQUESTED.load(Ordering::SeqCst) {
                    if !reported {
                        reported = true;
                        println!("HOST-TERM: SIGTERM received; ending run through final report");
                    }
                    // SAFETY: Category 8 - `vcpu` remains a live probe-owned
                    // HVF handle while the run is armed; this only asks its
                    // owning thread to leave `hv_vcpu_run`.
                    unsafe { hv_vcpus_exit(&vcpu, 1) };
                }
                std::thread::sleep(WATCH_INTERVAL);
            }
        });
    if let Err(error) = spawned {
        println!("HOST-TERM: watcher unavailable ({error}); TERM waits for the next exit");
    }
}
