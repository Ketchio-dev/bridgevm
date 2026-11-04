//! Cancellation and reap loop; never gives up the retained child on uncertainty.
use super::*;
use std::time::{Duration, Instant};
/// swtpm ends at once on TERM.
const TERM_GRACE: Duration = Duration::from_secs(2);
/// The VMM helper answers TERM by stopping its vCPUs and saving the UEFI vars and every NVMe
/// namespace (write-back ends with F_FULLFSYNC) before it exits. 8 s leaves room for a large
/// write-back on a slow disk, while KILL and reap still fit the app's 12 s cleanup observation.
const HELPER_TERM_GRACE: Duration = Duration::from_secs(8);
const KILL_REAP: Duration = Duration::from_secs(2);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
impl OwnedChild {
    pub(super) fn observe(
        &mut self,
        control: &RuntimeControl<'_>,
        force_stop: bool,
    ) -> io::Result<ChildExit> {
        let mut stop_started = None;
        let mut kill_sent = false;
        let mut reported = false;
        let mut wait_error = None;
        let term_grace = match self.role {
            ChildRole::Helper => HELPER_TERM_GRACE,
            ChildRole::Swtpm => TERM_GRACE,
        };
        loop {
            match self.try_wait_observed(control) {
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
                if !kill_sent && now.duration_since(started) >= term_grace {
                    kill_sent = true;
                    if !self.wait_uncertain && self.child.kill().is_err() {
                        self.report_once(control, &mut reported, "KILL failed");
                    }
                }
                if now.duration_since(started) >= term_grace + KILL_REAP {
                    self.report_once(control, &mut reported, "reap deadline expired");
                }
            }
            // Deadline exhaustion is an unconfirmed observation, not permission
            // to drop the child or the caller's leases. Continue owning/reaping.
            std::thread::sleep(POLL_INTERVAL);
        }
    }
}
