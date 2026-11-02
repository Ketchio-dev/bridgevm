//! Session-long root identity; the selected path may initially be a symlink.

use std::fs::{File, OpenOptions};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

#[path = "share_root_checked.rs"]
mod checked;
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
}
