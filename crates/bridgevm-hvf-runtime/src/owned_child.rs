//! Keep every spawned child owned until an actual wait result observes its exit.

use crate::{ChildRole, CleanupUnconfirmed, RuntimeControl};
use std::io;
use std::process::{Child, ChildStdin, ExitStatus};
use std::time::{Duration, Instant};
#[path = "owned_child_signals.rs"]
mod signals;

const TERM_GRACE: Duration = Duration::from_secs(2);
const KILL_REAP: Duration = Duration::from_secs(2);
const POLL_INTERVAL: Duration = Duration::from_millis(10);

pub(crate) struct OwnedChild {
    child: Child,
    role: ChildRole,
    reaped: Option<ExitStatus>,
    wait_uncertain: bool,
}

pub(crate) struct ChildExit {
    pub status: ExitStatus,
    pub cancelled: bool,
}

impl OwnedChild {
    pub(crate) fn new(child: Child, role: ChildRole) -> Self {
        Self {
            child,
            role,
            reaped: None,
            wait_uncertain: false,
        }
    }
    pub(crate) fn id(&self) -> u32 {
        self.child.id()
    }
    pub(crate) fn take_stdin(&mut self) -> Option<ChildStdin> {
        self.child.stdin.take()
    }
    pub(crate) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if let Some(status) = self.reaped {
            return Ok(Some(status));
        }
        match self.child.try_wait() {
            Ok(value) => {
                self.reaped = value;
                Ok(value)
            }
            Err(error) => {
                self.wait_uncertain = true;
                Err(error)
            }
        }
    }
    pub(crate) fn wait(&mut self, control: &RuntimeControl<'_>) -> io::Result<ChildExit> {
        self.observe(control, false)
    }
    pub(crate) fn shutdown(&mut self, control: &RuntimeControl<'_>) -> ExitStatus {
        // A wait error never relinquishes ownership. observe returns an error
        // only after a subsequent wait has actually reaped this same child.
        loop {
            if let Ok(value) = self.observe(control, true) {
                return value.status;
            }
        }
    }
    fn observe(&mut self, control: &RuntimeControl<'_>, force_stop: bool) -> io::Result<ChildExit> {
        let mut stop_started = None;
        let mut kill_sent = false;
        let mut reported = false;
        let mut wait_error = None;
        loop {
            match self.try_wait() {
                Ok(Some(status)) => {
                    return match wait_error {
                        Some(error) => Err(error),
                        None => Ok(ChildExit {
                            status,
                            cancelled: stop_started.is_some() || control.is_cancelled(),
                        }),
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    if wait_error.is_none() {
                        wait_error = Some(error);
                    }
                    self.report_once(control, &mut reported, "child wait failed");
                }
            }
            let now = Instant::now();
            if stop_started.is_none()
                && (force_stop || control.is_cancelled() || self.wait_uncertain)
            {
                stop_started = Some(now);
                if !self.wait_uncertain {
                    if let Err(error) = signals::terminate(self.id()) {
                        if error.raw_os_error() != Some(libc::ESRCH) {
                            self.report_once(control, &mut reported, "TERM failed");
                        }
                    }
                }
            }
            if let Some(started) = stop_started {
                if !kill_sent && now.duration_since(started) >= TERM_GRACE {
                    kill_sent = true;
                    if !self.wait_uncertain && self.child.kill().is_err() {
                        self.report_once(control, &mut reported, "KILL failed");
                    }
                }
                if now.duration_since(started) >= TERM_GRACE + KILL_REAP {
                    self.report_once(control, &mut reported, "reap deadline expired");
                }
            }
            // Deadline exhaustion is an unconfirmed observation, not permission
            // to drop the child or the caller's leases. Continue owning/reaping.
            std::thread::sleep(POLL_INTERVAL);
        }
    }
    fn report_once(&self, control: &RuntimeControl<'_>, reported: &mut bool, reason: &'static str) {
        if !*reported {
            *reported = true;
            control.report(CleanupUnconfirmed {
                role: self.role,
                pid: self.id(),
                reason,
            });
        }
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.reaped.is_none() {
            self.shutdown(&RuntimeControl::default());
        }
    }
}

#[cfg(test)]
#[path = "owned_child_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "owned_wait_uncertain_tests.rs"]
mod uncertain_tests;
