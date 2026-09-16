//! Keep every spawned child owned until an actual wait result observes its exit.

use crate::{ChildRole, CleanupUnconfirmed, RuntimeControl, RuntimeLifecycleEvent};
use std::io;
use std::process::{Child, ChildStdin, ExitStatus};
#[cfg(test)]
use std::time::{Duration, Instant};
#[path = "owned_child_signals.rs"]
mod signals;

#[path = "owned_child_wait.rs"]
mod wait;

pub(crate) struct OwnedChild {
    child: Child,
    role: ChildRole,
    reaped: Option<ExitStatus>,
    wait_uncertain: bool,
    generation: Option<u64>,
    reap_emitted: bool,
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
            generation: None,
            reap_emitted: false,
        }
    }
    pub(crate) fn adopt(
        child: Child,
        role: ChildRole,
        generation: Option<u64>,
        control: &RuntimeControl<'_>,
    ) -> Self {
        let mut owned = Self::new(child, role);
        owned.generation = generation;
        control.observe(RuntimeLifecycleEvent::ChildStarted {
            role,
            pid: owned.id(),
            generation,
        });
        owned
    }
    pub(crate) fn try_wait_observed(
        &mut self,
        control: &RuntimeControl<'_>,
    ) -> io::Result<Option<ExitStatus>> {
        let result = self.try_wait()?;
        if let Some(status) = result {
            if !self.reap_emitted {
                self.reap_emitted = true;
                control.observe(RuntimeLifecycleEvent::ChildReaped {
                    role: self.role,
                    pid: self.id(),
                    generation: self.generation,
                    status,
                });
            }
        }
        Ok(result)
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
    fn report_once(&self, control: &RuntimeControl<'_>, reported: &mut bool, reason: &'static str) {
        if !*reported {
            *reported = true;
            let value = CleanupUnconfirmed {
                role: self.role,
                pid: self.id(),
                reason,
            };
            control.observe(RuntimeLifecycleEvent::CleanupUnconfirmed {
                value,
                generation: self.generation,
            });
            control.report(value);
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

#[cfg(test)]
#[path = "owned_test_publication.rs"]
mod publication;
