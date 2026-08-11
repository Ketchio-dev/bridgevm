//! Exclusive writer lease for guest disk and UEFI variable images.
//!
//! Two VMM processes writing one disk image corrupts it, and the damage is
//! usually discovered much later as an unbootable guest. Nothing prevented
//! that: a stale runner, a second app window, or a live gate started while a
//! VM was already running would all just open the file and write.
//!
//! The lease is `flock(LOCK_EX | LOCK_NB)` on a sidecar file. Non-blocking is
//! deliberate: a writer that waits would hang the UI on a lock it can never
//! fairly obtain, whereas a writer that fails immediately can say which
//! process holds the image. The kernel releases the lock when the holder's
//! descriptor closes, so a crashed VMM does not strand the image -- which a
//! pidfile scheme could not promise.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
}

const LOCK_EX: i32 = 2;
const LOCK_NB: i32 = 4;
const LOCK_UN: i32 = 8;

/// Why a lease could not be taken.
#[derive(Debug)]
pub enum MediaLockError {
    /// Another process holds the image. Carries whatever it recorded about
    /// itself, so the message can name the holder rather than just failing.
    Held { path: PathBuf, holder: String },
    /// The lock file itself could not be created or opened.
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl std::fmt::Display for MediaLockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MediaLockError::Held { path, holder } => write!(
                f,
                "{} is already open for writing by {holder}",
                path.display()
            ),
            MediaLockError::Io { path, source } => {
                write!(f, "cannot lock {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for MediaLockError {}

/// An exclusive writer lease. Dropping it releases the lock.
#[derive(Debug)]
pub struct MediaLease {
    file: File,
    lock_path: PathBuf,
    image_path: PathBuf,
}

impl MediaLease {
    /// Take the lease for `image_path`, or fail immediately if held.
    ///
    /// `holder` describes this process for the benefit of whoever is refused;
    /// it is advisory text, never trusted for a decision.
    pub fn acquire(image_path: &Path, holder: &str) -> Result<Self, MediaLockError> {
        let lock_path = lock_path_for(image_path);
        if let Some(parent) = lock_path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| MediaLockError::Io {
                path: lock_path.clone(),
                source,
            })?;
        }
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|source| MediaLockError::Io {
                path: lock_path.clone(),
                source,
            })?;

        if !Self::flock(&file, LOCK_EX | LOCK_NB) {
            let mut existing = String::new();
            let _ = file.read_to_string(&mut existing);
            let holder = existing.trim();
            return Err(MediaLockError::Held {
                path: image_path.to_path_buf(),
                holder: if holder.is_empty() {
                    "another process".to_string()
                } else {
                    holder.to_string()
                },
            });
        }

        // Record who holds it, for the error message the next caller sees.
        let _ = file.set_len(0);
        let _ = file.seek(SeekFrom::Start(0));
        let _ = writeln!(file, "{holder} (pid {})", std::process::id());
        let _ = file.flush();

        Ok(Self {
            file,
            lock_path,
            image_path: image_path.to_path_buf(),
        })
    }

    /// SAFETY: `file` owns a valid descriptor for the duration of the call.
    fn flock(file: &File, op: i32) -> bool {
        (unsafe { flock(file.as_raw_fd(), op) }) == 0
    }
    pub fn image_path(&self) -> &Path {
        &self.image_path
    }

    pub fn lock_path(&self) -> &Path {
        &self.lock_path
    }
}
impl Drop for MediaLease {
    fn drop(&mut self) {
        let _ = self.file.set_len(0); // Clear stale holder attribution.
        let _ = Self::flock(&self.file, LOCK_UN); // Do not leak through fork.
    }
}
/// Sidecar path for an image. Kept beside the image so a lease travels with
/// the volume rather than living in a temp dir that a reboot clears.
pub fn lock_path_for(image_path: &Path) -> PathBuf {
    let mut name = image_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "image".to_string());
    name.push_str(".bridgevm-writer.lock");
    image_path.with_file_name(name)
}

#[cfg(test)]
#[path = "media_lock_tests.rs"]
mod tests;
