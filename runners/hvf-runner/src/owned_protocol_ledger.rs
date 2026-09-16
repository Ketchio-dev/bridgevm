//! Fold exact runtime child events; no completion is inferred from wrapper exit.
use super::dto::{Child, Event, Summary, Unconfirmed};
use bridgevm_hvf_runtime::{ChildRole, DirectoryDisposition, RuntimeLifecycleEvent};
use std::os::unix::process::ExitStatusExt;
#[derive(Default)]
pub(super) struct Ledger {
    pub helper: Summary,
    pub swtpm: Summary,
    live_helper: Option<Child>,
    live_swtpm: Option<Child>,
    pub directory: Option<DirectoryDisposition>,
    pub invalid: bool,
}
impl Ledger {
    pub fn observe(&mut self, fact: RuntimeLifecycleEvent) -> Option<Event> {
        let (role, pid, generation, status) = match fact {
            RuntimeLifecycleEvent::SwtpmDirectory(value) => {
                self.directory = Some(value);
                return None;
            }
            RuntimeLifecycleEvent::CleanupUnconfirmed { value, generation } => {
                let mut event = Event::new("cleanupUnconfirmed");
                event.unconfirmed = Some(Unconfirmed {
                    role: role_name(value.role).into(),
                    pid: value.pid,
                    generation,
                    reason: match value.reason {
                        "child wait failed" => "waitFailed",
                        "TERM failed" => "termFailed",
                        "KILL failed" => "killFailed",
                        _ => "reapDeadline",
                    }
                    .into(),
                    media_lease_disposition: "retained".into(),
                });
                return Some(event);
            }
            RuntimeLifecycleEvent::ChildStarted {
                role,
                pid,
                generation,
            } => (role, pid, generation, None),
            RuntimeLifecycleEvent::ChildReaped {
                role,
                pid,
                generation,
                status,
            } => (role, pid, generation, Some(status)),
        };
        let (summary, live) = match role {
            ChildRole::Helper => (&mut self.helper, &mut self.live_helper),
            ChildRole::Swtpm => (&mut self.swtpm, &mut self.live_swtpm),
        };
        let mut child = Child {
            role: role_name(role).into(),
            pid,
            generation,
            reason: None,
            status: None,
        };
        if pid == 0 || pid > i32::MAX as u32 || (role == ChildRole::Helper) != generation.is_some()
        {
            self.invalid = true;
            return None;
        }
        let mut event = Event::new(if status.is_some() {
            "childReaped"
        } else {
            "childStarted"
        });
        if let Some(status) = status {
            if live.as_ref() != Some(&child) {
                self.invalid = true;
                return None;
            }
            let (reason, code) = match (status.code(), status.signal()) {
                (Some(code), None) => ("exit", code),
                (None, Some(signal)) => ("signal", signal),
                _ => {
                    self.invalid = true;
                    return None;
                }
            };
            child.reason = Some(reason.into());
            child.status = Some(code);
            summary.reaped_count = match summary.reaped_count.checked_add(1) {
                Some(value) => value,
                None => {
                    self.invalid = true;
                    return None;
                }
            };
            summary.last = Some(child.clone());
            *live = None;
        } else {
            if live.is_some()
                || (role == ChildRole::Swtpm && summary.spawned_count != 0)
                || (role == ChildRole::Helper && generation != Some(summary.spawned_count))
            {
                self.invalid = true;
                return None;
            }
            summary.spawned_count = match summary.spawned_count.checked_add(1) {
                Some(value) => value,
                None => {
                    self.invalid = true;
                    return None;
                }
            };
            *live = Some(child.clone());
        }
        event.child = Some(child);
        Some(event)
    }
    pub fn complete(&self) -> bool {
        !self.invalid
            && self.live_helper.is_none()
            && self.live_swtpm.is_none()
            && self.helper.spawned_count == self.helper.reaped_count
            && self.swtpm.spawned_count == self.swtpm.reaped_count
            && self.directory != Some(DirectoryDisposition::Created)
    }
    pub fn directory_name(&self) -> &'static str {
        match self.directory {
            None => "notCreated",
            Some(DirectoryDisposition::Removed) => "removed",
            _ => "removeFailed",
        }
    }
}
fn role_name(role: ChildRole) -> &'static str {
    match role {
        ChildRole::Helper => "helper",
        ChildRole::Swtpm => "swtpm",
    }
}

#[cfg(test)]
#[path = "owned_protocol_ledger_tests.rs"]
mod tests;
