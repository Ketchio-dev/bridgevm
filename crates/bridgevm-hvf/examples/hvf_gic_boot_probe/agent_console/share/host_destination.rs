//! Descriptor-relative guest writes: no symlink traversal below the chosen root.

use std::ffi::{CStr, CString};
use std::fs::File;
#[cfg(test)]
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::os::fd::{AsRawFd, FromRawFd};
#[cfg(test)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
#[path = "host_destination_open.rs"]
mod open;
#[path = "host_destination_snapshot.rs"]
pub(super) mod snapshot;
#[path = "guest_write_directory.rs"]
mod directory;
#[path = "guest_write_mark.rs"]
mod mark;
#[path = "guest_write_policy.rs"]
mod policy;
#[path = "guest_write_host.rs"]
mod seal_host;

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

#[cfg(test)]
thread_local! {
    /// The errno a new temporary fails with on this thread, as on a full Mac disk (tests).
    pub(super) static TEMPORARY_FAULT: std::cell::Cell<Option<i32>> = const { std::cell::Cell::new(None) };
}

/// The Mac file a preferred guest copy is to replace, as the session's first scan saw it: size
/// and modification time (ms), as the scan reads them (share_synced.rs).
pub(super) type MacStat = Option<(u64, u128)>;

/// A preferred guest copy was not published: the Mac file changed since the first scan (or is
/// no longer a plain file).
#[derive(Debug)]
pub(super) struct MacChanged;

impl std::fmt::Display for MacChanged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the Mac file changed")
    }
}

impl std::error::Error for MacChanged {}

pub(super) fn is_mac_changed(error: &io::Error) -> bool {
    error.get_ref().is_some_and(|inner| inner.is::<MacChanged>())
}

/// The prefix of the host's unpublished temporaries, `.bridgevm-sync-<pid>-<n>`.
pub(super) const TEMP_PREFIX: &str = ".bridgevm-sync-";

/// Whether the last part of `name` is exactly a temporary one side writes: the host's
/// `.bridgevm-sync-<pid>-<n>` or the agent's `.bridgevm-part-<32 lowercase hex>` (a large PUT).
/// Such a file is never shared either way; any other name, a user's `.bridgevm-part-notes` too,
/// is a file like any other.
pub(super) fn is_share_temporary(name: &str) -> bool {
    let leaf = name.rsplit('/').next().unwrap_or(name);
    match leaf.strip_prefix(".bridgevm-part-") {
        Some(hex) => hex.len() == 32 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')),
        None => host_temporary_pid(leaf).is_some(),
    }
}

/// The writer's pid in a host temporary's exact name.
pub(super) fn host_temporary_pid(leaf: &str) -> Option<libc::pid_t> {
    let (pid, n) = leaf.strip_prefix(TEMP_PREFIX)?.split_once('-')?;
    let digits = |s: &str, most: usize| (1..=most).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(pid, 10) || !digits(n, 20) {
        return None;
    }
    pid.parse().ok().filter(|pid| *pid > 0)
}

/// A temporary written in parts, published or discarded by its Destination.
pub(super) struct Staged {
    temporary: CString,
    pub(super) file: File,
}

/// Bytes free for an unprivileged writer on the volume holding `dir`.
pub(in crate::agent_console) fn free_bytes(dir: &File) -> io::Result<u64> {
    // statfs, not statvfs: macOS statvfs counts blocks in 32 bits.
    let mut stat = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: a live descriptor and writable statfs storage.
    if unsafe { libc::fstatfs(dir.as_raw_fd(), stat.as_mut_ptr()) } < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fstatfs succeeded and filled the storage.
    let stat = unsafe { stat.assume_init() };
    Ok(stat.f_bavail.saturating_mul(u64::from(stat.f_bsize)))
}
pub(super) struct Destination {
    parent: File,
    leaf: CString,
}

fn name(value: &std::ffi::OsStr) -> io::Result<CString> {
    use std::os::unix::ffi::OsStrExt;
    CString::new(value.as_bytes()).map_err(|_| io::ErrorKind::InvalidInput.into())
}

fn open_at(parent: &File, leaf: &CStr, flags: i32, mode: libc::mode_t) -> io::Result<File> {
    // SAFETY: parent is live and leaf is NUL-terminated; mode is provided for O_CREAT.
    let fd = unsafe { libc::openat(parent.as_raw_fd(), leaf.as_ptr(), flags | libc::O_CLOEXEC, mode as libc::c_uint) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: openat returned a new owned descriptor, adopted exactly once.
    Ok(unsafe { File::from_raw_fd(fd) })
}

impl Destination {
    fn existing(&self, access: i32) -> io::Result<Option<File>> {
        match open_at(&self.parent, &self.leaf, access | libc::O_NOFOLLOW | libc::O_NONBLOCK, 0) {
            Ok(file) => {
                let meta = file.metadata()?;
                if !meta.is_file() {
                    return Err(io::ErrorKind::InvalidInput.into());
                }
                Ok(Some(file))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub(super) fn write(&self, bytes: &[u8]) -> io::Result<Option<u128>> {
        self.write_at(bytes, None)
    }

    /// `mtime`: the guest file's modification time, which the Mac copy keeps.
    pub(super) fn write_at(&self, bytes: &[u8], mtime: Option<std::time::SystemTime>) -> io::Result<Option<u128>> {
        self.write_at_unless_changed(bytes, mtime, None)
    }

    /// As `write_at`, but with `mac` (a preferred guest copy) not over a Mac file that is no
    /// longer as `mac` says: MacChanged.
    pub(super) fn write_at_unless_changed(&self, bytes: &[u8], mtime: Option<std::time::SystemTime>,
                                          mac: Option<MacStat>) -> io::Result<Option<u128>> {
        self.write_with_at(|file| file.write_all(bytes), mtime, mac)
    }

    #[cfg(test)]
    fn write_with(&self, write: impl FnOnce(&mut File) -> io::Result<()>) -> io::Result<Option<u128>> {
        self.write_with_at(write, None, None)
    }

    fn write_with_at(&self, write: impl FnOnce(&mut File) -> io::Result<()>,
                     mtime: Option<std::time::SystemTime>, mac: Option<MacStat>) -> io::Result<Option<u128>> {
        // Require write access to an existing destination before replacing it.
        let existing = self.existing(libc::O_WRONLY)?;
        let (temporary, mut file) = self.temporary()?;
        let result = write(&mut file).and_then(|()| self.seal_and_rename(existing.as_ref(), &file, &temporary, mtime, mac));
        if result.is_err() {
            // SAFETY: remove the unpublished exclusive temporary name.
            unsafe { libc::unlinkat(self.parent.as_raw_fd(), temporary.as_ptr(), 0) };
        }
        result
    }

    fn seal_and_rename(&self, existing: Option<&File>, file: &File, temporary: &CStr,
                       mtime: Option<std::time::SystemTime>, mac: Option<MacStat>) -> io::Result<Option<u128>> {
        // The guest write policy's mode and quarantine mark, or no write.
        policy::seal(existing, file, &self.parent)?;
        // Only the date shown in Finder: the content is what must land.
        if let Some(mtime) = mtime {
            let _ = file.set_modified(mtime);
        }
        // F_FULLFSYNC on macOS.
        file.sync_all()?;
        let mtime = file.metadata()?.modified().ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|t| t.as_millis());
        // Last, right before the rename: a Mac edit since the first scan wins over a preferred
        // copy. A Mac file deleted since does not: the guest's edit wins over the deletion.
        if let Some(mac) = mac {
            if self.mac_stat().is_some_and(|now| now != mac) {
                // Preserve the fully fetched guest bytes beside the newer Mac copy. Reserve
                // the name exclusively: a conflict never replaces a user's existing file.
                let original = self.leaf.to_string_lossy();
                let path = Path::new(original.as_ref());
                let stem = path.file_stem().unwrap_or(path.as_os_str()).to_string_lossy();
                let extension = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
                for number in 1u64.. {
                    let label = if number == 1 { "Windows copy".to_string() } else { format!("Windows copy {number}") };
                    let conflict = CString::new(format!("{stem} ({label}){extension}")).unwrap();
                    // SAFETY: pinned parent and two live, single-component names. linkat
                    // publishes without replacing any existing conflict (even a racing one).
                    if unsafe { libc::linkat(self.parent.as_raw_fd(), temporary.as_ptr(), self.parent.as_raw_fd(), conflict.as_ptr(), 0) } == 0 {
                        unsafe { libc::unlinkat(self.parent.as_raw_fd(), temporary.as_ptr(), 0) };
                        println!("BVAGENT SHARE conflict kept-mac={} windows-copy={}", original, conflict.to_string_lossy());
                        return Err(io::Error::other(MacChanged));
                    }
                    let error = io::Error::last_os_error();
                    if error.kind() != io::ErrorKind::AlreadyExists { return Err(error); }
                }
            }
        }
        // SAFETY: both names and the owning directory remain live. renameat
        // replaces a concurrently inserted leaf symlink instead of following it.
        let status = unsafe { libc::renameat(self.parent.as_raw_fd(), temporary.as_ptr(),
                                            self.parent.as_raw_fd(), self.leaf.as_ptr()) };
        if status < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(mtime)
    }

    /// An unpublished temporary beside the destination for a file written in parts (a large
    /// guest file, share_stage_writer.rs). Write access to an existing destination is checked
    /// now and again at `publish`.
    pub(super) fn stage(&self) -> io::Result<Staged> {
        self.existing(libc::O_WRONLY)?;
        let (temporary, file) = self.temporary()?;
        Ok(Staged { temporary, file })
    }

    /// The destination's size and modification time (ms) now: None when it is missing,
    /// Some(None) when it is not a plain file (or cannot be read).
    fn mac_stat(&self) -> Option<MacStat> {
        let file = match self.existing(libc::O_RDONLY) {
            Ok(Some(file)) => file,
            Ok(None) => return None,
            Err(_) => return Some(None),
        };
        let stat = file.metadata().ok().and_then(|meta| Some((meta.len(), super::file_mtime_ms_from_meta(&meta)?)));
        Some(stat)
    }

    /// Seal, date, F_FULLFSYNC and rename the filled temporary, not over a Mac file that is no
    /// longer as `mac` says (MacChanged); on failure it is removed.
    pub(super) fn publish(&self, staged: Staged, mtime: Option<std::time::SystemTime>,
                          mac: Option<MacStat>) -> io::Result<Option<u128>> {
        let result = self.existing(libc::O_WRONLY)
            .and_then(|existing| self.seal_and_rename(existing.as_ref(), &staged.file, &staged.temporary, mtime, mac));
        if result.is_err() {
            self.discard(staged);
        }
        result
    }

    pub(super) fn discard(&self, staged: Staged) {
        drop(staged.file);
        // SAFETY: remove the unpublished exclusive temporary name.
        unsafe { libc::unlinkat(self.parent.as_raw_fd(), staged.temporary.as_ptr(), 0) };
    }

    fn temporary(&self) -> io::Result<(CString, File)> {
        #[cfg(test)]
        if let Some(errno) = TEMPORARY_FAULT.with(std::cell::Cell::get) {
            return Err(io::Error::from_raw_os_error(errno));
        }
        for _ in 0..32 {
            // Uniqueness only: O_EXCL, not unpredictability, protects existing files.
            let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let temporary = CString::new(format!("{TEMP_PREFIX}{}-{id}", std::process::id())).unwrap();
            if temporary == self.leaf { continue; }
            match open_at(&self.parent, &temporary,
                          libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW, policy::NEW_FILE_MODE) {
                Ok(file) => return Ok((temporary, file)),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(io::ErrorKind::AlreadyExists.into())
    }

    pub(super) fn absent(root: &Path, key: &str) -> bool {
        let Ok(destination) = Self::open(root, key, false) else { return false; };
        matches!(destination.existing(libc::O_RDONLY), Ok(None))
    }

    pub(super) fn delete_unless_changed(&self, expected: (u64, u128)) -> io::Result<()> {
        if self.mac_stat() != Some(Some(expected)) {
            return Err(io::Error::other(MacChanged));
        }
        self.delete()
    }

    pub(super) fn delete(&self) -> io::Result<()> {
        let mut meta = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: writable stat storage, live parent and NUL-terminated name.
        if unsafe { libc::fstatat(self.parent.as_raw_fd(), self.leaf.as_ptr(), meta.as_mut_ptr(), libc::AT_SYMLINK_NOFOLLOW) } < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: successful fstatat initialized the storage.
        if unsafe { meta.assume_init() }.st_mode & libc::S_IFMT != libc::S_IFREG {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        // SAFETY: pinned parent, one validated name, and no recursive directory deletion.
        if unsafe { libc::unlinkat(self.parent.as_raw_fd(), self.leaf.as_ptr(), 0) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "host_destination_tests.rs"]
mod tests;
