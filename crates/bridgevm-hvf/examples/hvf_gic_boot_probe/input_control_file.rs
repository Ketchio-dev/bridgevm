//! Persistent access to the append-only host input channel.

use std::fs::File;
use std::path::PathBuf;

#[path = "input_control_file/checked_access.rs"]
mod checked_access;

pub(crate) struct InputControlFile {
    path: PathBuf,
    // Once opened, keep this inode even when validation fails; never reopen a replacement.
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
        self.with_exclusive(|file| file.metadata().ok().map(|metadata| metadata.len()))?
    }

    pub(crate) fn with_exclusive<T>(&mut self, action: impl FnOnce(&mut File) -> T) -> Option<T> {
        if self.file.is_none() {
            self.file = checked_access::open(&self.path);
        }
        checked_access::with_exclusive(&self.path, self.file.as_mut()?, action)
    }

    pub(crate) fn file(&mut self) -> Option<&mut File> {
        self.with_exclusive(|_| ())?;
        self.file.as_mut()
    }
}
