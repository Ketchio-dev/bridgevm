//! Persistent access to the append-only host input channel.

use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::path::PathBuf;

pub(crate) struct InputControlFile {
    path: PathBuf,
    file: Option<File>,
}

impl InputControlFile {
    pub(crate) fn from_env() -> Option<Self> {
        std::env::var_os("BRIDGEVM_INPUT_CONTROL")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .map(Self::from_path)
    }

    pub(crate) fn from_path(path: PathBuf) -> Self {
        Self { path, file: None }
    }

    pub(crate) fn length(&mut self) -> Option<u64> {
        self.file()?.metadata().ok().map(|metadata| metadata.len())
    }

    pub(crate) fn with_exclusive<T>(
        &mut self,
        action: impl FnOnce(&mut File) -> T,
    ) -> Option<T> {
        let file = self.file()?;
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
            return None;
        }
        let result = action(file);
        unsafe {
            libc::flock(file.as_raw_fd(), libc::LOCK_UN);
        }
        Some(result)
    }

    fn file(&mut self) -> Option<&mut File> {
        if self.file.is_none() {
            self.file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&self.path)
                .ok();
        }
        self.file.as_mut()
    }
}
