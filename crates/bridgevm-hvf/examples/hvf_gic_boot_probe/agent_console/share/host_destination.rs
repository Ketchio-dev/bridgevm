//! Descriptor-relative guest writes: no symlink traversal below the chosen root.

use std::ffi::{CStr, CString};
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

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
    pub(super) fn open(root: &Path, key: &str, create: bool) -> io::Result<Self> {
        let mut names = Vec::new();
        for component in Path::new(key).components() {
            match component {
                Component::Normal(part) => names.push(name(part)?),
                _ => return Err(io::ErrorKind::InvalidInput.into()),
            }
        }
        let leaf = names.pop().ok_or(io::ErrorKind::InvalidInput)?;
        // The explicitly selected root may itself be a symlink; pin its target.
        let mut parent = OpenOptions::new().read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC).open(root)?;
        for part in names {
            let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW;
            parent = match open_at(&parent, &part, flags, 0) {
                Ok(file) => file,
                Err(e) if create && e.kind() == io::ErrorKind::NotFound => {
                    // SAFETY: live directory descriptor and NUL-terminated child name.
                    let status = unsafe { libc::mkdirat(parent.as_raw_fd(), part.as_ptr(), 0o755) };
                    if status < 0 && io::Error::last_os_error().kind() != io::ErrorKind::AlreadyExists {
                        return Err(io::Error::last_os_error());
                    }
                    open_at(&parent, &part, flags, 0)?
                }
                Err(e) => return Err(e),
            };
        }
        Ok(Self { parent, leaf })
    }

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
        self.write_with(|file| file.write_all(bytes))
    }

    fn write_with(&self, write: impl FnOnce(&mut File) -> io::Result<()>) -> io::Result<Option<u128>> {
        // Require write access to an existing destination before replacing it.
        let existing = self.existing(libc::O_WRONLY)?;
        let (temporary, mut file) = self.temporary()?;
        let result = (|| {
            write(&mut file)?;
            if let Some(existing) = existing {
                let mode = existing.metadata()?.permissions().mode() & 0o777;
                file.set_permissions(std::fs::Permissions::from_mode(mode))?;
                #[cfg(target_os = "macos")]
                {
                    // SAFETY: both descriptors are live; no data/stat copy so the
                    // replacement keeps its new content and modification time.
                    let status = unsafe { libc::fcopyfile(existing.as_raw_fd(), file.as_raw_fd(),
                        std::ptr::null_mut(), libc::COPYFILE_ACL | libc::COPYFILE_XATTR) };
                    if status != 0 { return Err(io::Error::last_os_error()); }
                }
            }
            file.sync_all()?;
            let mtime = file.metadata()?.modified().ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|t| t.as_millis());
            // SAFETY: both names and the owning directory remain live. renameat
            // replaces a concurrently inserted leaf symlink instead of following it.
            let status = unsafe { libc::renameat(self.parent.as_raw_fd(), temporary.as_ptr(),
                                                self.parent.as_raw_fd(), self.leaf.as_ptr()) };
            if status < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(mtime)
        })();
        if result.is_err() {
            // SAFETY: remove the unpublished exclusive temporary name.
            unsafe { libc::unlinkat(self.parent.as_raw_fd(), temporary.as_ptr(), 0) };
        }
        result
    }

    fn temporary(&self) -> io::Result<(CString, File)> {
        for _ in 0..32 {
            // Uniqueness only: O_EXCL, not unpredictability, protects existing files.
            let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let temporary = CString::new(format!(".bridgevm-sync-{}-{id}", std::process::id())).unwrap();
            if temporary == self.leaf { continue; }
            match open_at(&self.parent, &temporary,
                          libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW, 0o600) {
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
