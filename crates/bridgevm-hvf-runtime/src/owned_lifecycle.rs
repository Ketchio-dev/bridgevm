//! Facts observed by the actual retained child/owner, independent of wire I/O.
use std::process::ExitStatus;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChildRole {
    Helper,
    Swtpm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CleanupUnconfirmed {
    pub role: ChildRole,
    pub pid: u32,
    pub reason: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub enum RuntimeLifecycleEvent {
    ChildStarted {
        role: ChildRole,
        pid: u32,
        generation: Option<u64>,
    },
    ChildReaped {
        role: ChildRole,
        pid: u32,
        generation: Option<u64>,
        status: ExitStatus,
    },
    CleanupUnconfirmed {
        value: CleanupUnconfirmed,
        generation: Option<u64>,
    },
    SwtpmDirectory(DirectoryDisposition),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DirectoryDisposition {
    Created,
    Removed,
    RemoveFailed,
}

#[cfg(test)]
#[path = "owned_lifecycle_tests.rs"]
mod tests;

use super::RuntimeControl;
impl Default for RuntimeControl<'static> {
    fn default() -> Self {
        Self::new(&|| false, &|value| {
            eprintln!(
                "owned cleanup unconfirmed: {:?} pid={} {}; retaining ownership",
                value.role, value.pid, value.reason
            );
        })
    }
}
