//! Validate the leaf and pinned descriptor under the writer/reader flock.
//! Parent directories must be trusted; flock does not prevent hostile renames.

use std::fs::{self, File, Metadata, OpenOptions};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;
#[path = "exclusive_lock.rs"]
mod exclusive_lock;
use exclusive_lock::ExclusiveLock;

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
