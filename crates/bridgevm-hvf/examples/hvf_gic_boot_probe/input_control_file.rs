//! Persistent access to the append-only host input channel.
//!
//! Blocking startup access and non-waiting polling share one pinned-file path.
//! The leaf and the pinned descriptor are validated under the writer/reader
//! flock. Parent directories must be trusted; flock does not prevent hostile
//! renames.

use std::fs::{self, File, Metadata, OpenOptions};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

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

    pub(crate) fn file(&mut self) -> Option<&mut File> {
        self.with_exclusive(|_| ())?;
        self.file.as_mut()
    }

    pub(crate) fn with_exclusive<T>(&mut self, action: impl FnOnce(&mut File) -> T) -> Option<T> {
        self.with_lock(false, action)
    }

    /// A busy writer defers polling without invoking the reader's state changes.
    pub(crate) fn try_with_exclusive<T>(
        &mut self,
        action: impl FnOnce(&mut File) -> T,
    ) -> Option<T> {
        self.with_lock(true, action)
    }

    fn with_lock<T>(
        &mut self,
        nonblocking: bool,
        action: impl FnOnce(&mut File) -> T,
    ) -> Option<T> {
        if self.file.is_none() {
            self.file = open_checked(&self.path);
        }
        with_checked_lock(&self.path, self.file.as_mut()?, nonblocking, action)
    }
}

fn open_checked(path: &Path) -> Option<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
        .ok()?;
    acceptable(&file.metadata().ok()?).then_some(file)
}

fn acceptable(metadata: &Metadata) -> bool {
    metadata.is_file()
        && metadata.nlink() == 1
        // SAFETY: geteuid only reads the current process credentials.
        && metadata.uid() == unsafe { libc::geteuid() }
}

fn with_checked_lock<T>(
    path: &Path,
    file: &mut File,
    nonblocking: bool,
    action: impl FnOnce(&mut File) -> T,
) -> Option<T> {
    let _lock = ExclusiveLock::new(file, nonblocking)?;
    let descriptor = file.metadata().ok()?;
    let leaf = fs::symlink_metadata(path).ok()?;
    if !acceptable(&descriptor)
        || !acceptable(&leaf)
        || descriptor.dev() != leaf.dev()
        || descriptor.ino() != leaf.ino()
    {
        return None;
    }
    Some(action(file))
}

/// Scoped flock ownership; only periodic polling requests nonblocking acquisition.
struct ExclusiveLock(RawFd);

impl ExclusiveLock {
    fn new(file: &File, nonblocking: bool) -> Option<Self> {
        let fd = file.as_raw_fd();
        let flags = libc::LOCK_EX | if nonblocking { libc::LOCK_NB } else { 0 };
        // SAFETY: file keeps this descriptor alive through the scoped lock.
        (unsafe { libc::flock(fd, flags) } == 0).then(|| Self(fd))
    }
}

impl Drop for ExclusiveLock {
    fn drop(&mut self) {
        // SAFETY: the caller retains file until this guard is dropped.
        unsafe { libc::flock(self.0, libc::LOCK_UN) };
    }
}
