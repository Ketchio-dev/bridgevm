//! Scoped flock ownership; only periodic polling requests nonblocking acquisition.

use std::fs::File;
use std::os::fd::{AsRawFd, RawFd};

pub(super) struct ExclusiveLock(RawFd);

impl ExclusiveLock {
    pub(super) fn new(file: &File, nonblocking: bool) -> Option<Self> {
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
