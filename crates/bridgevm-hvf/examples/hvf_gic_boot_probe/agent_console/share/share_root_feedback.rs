//! Root notices describe observed availability transitions, never data authority.

use super::*;

impl ShareState {
    pub(in crate::agent_console) fn root_matches(&mut self) -> bool {
        let matches = self.root.matches(&self.host_dir);
        if !matches {
            self.report_root_state(true);
        }
        matches
    }

    /// Prints only a changed state; repeated checks add no log line.
    pub(in crate::agent_console) fn report_root_state(&mut self, blocked: bool) {
        if let Some(line) = root_state_transition(&mut self.root_blocked, blocked) {
            #[cfg(test)]
            NOTICES.with(|notices| notices.borrow_mut().push(line));
            println!("{line}");
        }
    }

    /// Notices printed on this test thread since the previous call.
    #[cfg(test)]
    pub(in crate::agent_console) fn take_root_notices() -> Vec<&'static str> {
        NOTICES.with(|notices| std::mem::take(&mut *notices.borrow_mut()))
    }
}

#[cfg(test)]
thread_local! {
    static NOTICES: std::cell::RefCell<Vec<&'static str>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn root_state_transition(was_blocked: &mut bool, blocked: bool) -> Option<&'static str> {
    if *was_blocked == blocked {
        return None;
    }
    *was_blocked = blocked;
    Some(if blocked {
        "BVAGENT SHARE root-state blocked"
    } else {
        "BVAGENT SHARE root-state ready"
    })
}

#[cfg(test)]
#[path = "share_root_feedback_tests.rs"]
mod tests;
