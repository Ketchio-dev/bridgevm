//! Guest-originated destinations begin at a verified, held root descriptor.

use super::ShareState;
use std::fs::File;
use std::io;

impl ShareState {
    pub(in crate::agent_console) fn checked_root(&mut self) -> io::Result<File> {
        self.root.checked_clone(&self.host_dir)
    }
}
