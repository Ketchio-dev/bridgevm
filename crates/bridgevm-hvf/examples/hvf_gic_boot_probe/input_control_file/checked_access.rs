//! Validate the leaf and pinned descriptor under the writer/reader flock.
//! Parent directories must be trusted; flock does not prevent hostile renames.

use std::fs::{self, File, Metadata, OpenOptions};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

pub(super) fn open(path: &Path) -> Option<File> {
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

pub(super) fn with_exclusive<T>(
    path: &Path,
    file: &mut File,
    action: impl FnOnce(&mut File) -> T,
) -> Option<T> {
    let _lock = ExclusiveLock::new(file)?;
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

struct ExclusiveLock(RawFd);

impl ExclusiveLock {
    fn new(file: &File) -> Option<Self> {
        let fd = file.as_raw_fd();
        // SAFETY: file keeps this descriptor alive through the scoped lock.
        (unsafe { libc::flock(fd, libc::LOCK_EX) } == 0).then(|| Self(fd))
    }
}

impl Drop for ExclusiveLock {
    fn drop(&mut self) {
        // SAFETY: the caller retains file until this guard is dropped.
        unsafe { libc::flock(self.0, libc::LOCK_UN) };
    }
}
