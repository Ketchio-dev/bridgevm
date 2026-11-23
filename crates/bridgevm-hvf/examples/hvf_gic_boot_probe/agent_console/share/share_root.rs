//! Session-long root identity; the selected path may initially be a symlink.
//! Root notices describe observed availability transitions, never data authority.

use super::*;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

pub(in crate::agent_console) struct ShareRoot {
    directory: Option<File>,
}

impl ShareRoot {
    pub(super) fn new(path: &Path) -> Self {
        Self {
            directory: Self::open(path),
        }
    }

    pub(super) fn matches(&mut self, path: &Path) -> bool {
        let Some(current) = Self::open(path) else {
            return false;
        };
        let Some(pinned) = self.directory.as_ref() else {
            self.directory = Some(current);
            return true;
        };
        let (Ok(expected), Ok(actual)) = (pinned.metadata(), current.metadata()) else {
            return false;
        };
        expected.dev() == actual.dev() && expected.ino() == actual.ino()
    }

    fn open(path: &Path) -> Option<File> {
        // Follow the explicitly selected root, but hold its target for this session.
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC)
            .open(path)
            .ok()
    }

    /// Verified duplicates retain the selected target across later path replacement.
    pub(in crate::agent_console) fn checked_clone(&mut self, path: &Path) -> io::Result<File> {
        let refused = || io::Error::from(io::ErrorKind::WouldBlock);
        if !self.matches(path) {
            return Err(refused());
        }
        let directory = self.directory.as_ref().ok_or_else(refused)?;
        let metadata = directory.metadata().map_err(|_| refused())?;
        if !metadata.is_dir() || metadata.nlink() == 0 {
            return Err(refused());
        }
        directory.try_clone().map_err(|_| refused())
    }
}

impl ShareState {
    /// Guest-originated destinations begin at a verified, held root descriptor.
    pub(in crate::agent_console) fn checked_root(&mut self) -> io::Result<File> {
        self.root.checked_clone(&self.host_dir)
    }

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
mod tests {
    //! Exact root-state lines: initial readiness is silent and only changes print.

    use super::root_state_transition;

    #[test]
    fn root_state_lines_print_only_changes_across_two_cycles() {
        let mut blocked = false;
        assert_eq!(root_state_transition(&mut blocked, false), None);
        for _ in 0..2 {
            assert_eq!(
                root_state_transition(&mut blocked, true),
                Some("BVAGENT SHARE root-state blocked")
            );
            assert!(blocked);
            assert_eq!(root_state_transition(&mut blocked, true), None);
            assert_eq!(
                root_state_transition(&mut blocked, false),
                Some("BVAGENT SHARE root-state ready")
            );
            assert!(!blocked);
            assert_eq!(root_state_transition(&mut blocked, false), None);
        }
    }
}
