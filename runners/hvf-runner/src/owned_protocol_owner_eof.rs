//! Owner EOF (the app crashed or was force-quit) first lets Windows shut itself down.
//!
//! The app's Stop presses the guest's ACPI power button through `input.ctl` when the guest agent is
//! not ready (after a RESUME when the guest is paused, `HvfGuestPause.resumeBeforeStop`); Windows then
//! closes its apps and turns off as for `shutdown.exe /s`. Without its owner the runner does the same,
//! waits up to the app's quit deadline, and only then cancels the run
//! (helper SIGTERM: vars and disk written back, ending with F_FULLFSYNC). A guest power-off takes
//! the helper's same terminal stop. A TERM or INT to the runner ends the wait at once.
use std::cell::Cell;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::{Duration, Instant};

/// The app's quit waits as long for Windows (`ControlAppTermination.deadline`).
pub(super) const BUDGET: Duration = Duration::from_secs(60);

pub(super) struct OwnerEof<P> {
    press: P,
    budget: Duration,
    pressed_at: Cell<Option<Instant>>,
}

impl<P: Fn() -> bool> OwnerEof<P> {
    pub fn new(press: P, budget: Duration) -> Self {
        Self {
            press,
            budget,
            pressed_at: Cell::new(None),
        }
    }

    /// Whether the run is cancelled now, given the latched cause, a TERM/INT to the runner and
    /// whether a helper (a running guest) exists. Pressing happens at most once.
    pub fn cancel(
        &self,
        cause: Option<&str>,
        signalled: bool,
        helper_running: bool,
        now: Instant,
    ) -> bool {
        // A dead owner closes both pipes; whichever the runner notices first latches the cause: its
        // input's EOF (ownerEOF) or a failed write or read on the channel (protocolFailed).
        if !matches!(cause, Some("ownerEOF" | "protocolFailed")) || signalled {
            return cause.is_some() || signalled;
        }
        match self.pressed_at.get() {
            // The guest powered off (or reset): the run ends without a TERM.
            Some(at) => !helper_running || now.duration_since(at) >= self.budget,
            None if helper_running && (self.press)() => {
                self.pressed_at.set(Some(now));
                let _ = writeln!(
                    std::io::stderr(),
                    "owner input closed; pressed the guest power button, waiting up to {} s for Windows to shut down",
                    self.budget.as_secs()
                );
                false
            }
            None => true,
        }
    }
}

/// Appends RESUME (a paused guest's vCPUs could not take the press; a no-op otherwise) and the
/// power-button line under the helper's own flock, so its compaction cannot drop them.
/// The app creates the file; a missing, replaced or busy file means no press.
pub(super) fn press_power_button(path: &Path) -> bool {
    let Ok(mut file) = OpenOptions::new()
        .append(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
    else {
        return false;
    };
    if !file.metadata().is_ok_and(|value| value.is_file()) {
        return false;
    }
    let deadline = Instant::now() + Duration::from_millis(200);
    // SAFETY: flock on a descriptor `file` keeps open; closing it releases the lock.
    while unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    file.write_all(b"RESUME\nPOWERBUTTON\n").is_ok()
}

#[cfg(test)]
#[path = "owned_protocol_owner_eof_tests.rs"]
mod tests;
