//! Verified duplicates retain the selected target across later path replacement.

use super::ShareRoot;
use std::fs::File;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

impl ShareRoot {
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
