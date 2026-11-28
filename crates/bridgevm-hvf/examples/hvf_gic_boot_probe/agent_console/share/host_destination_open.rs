//! Start descriptor-relative destination traversal from a path or verified root.

use super::{directory, name, open_at, Destination};
use std::ffi::CString;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Component, Path};

fn components(key: &str) -> io::Result<(Vec<CString>, CString)> {
    let mut names = Vec::new();
    for component in Path::new(key).components() {
        match component {
            Component::Normal(part) => names.push(name(part)?),
            _ => return Err(io::ErrorKind::InvalidInput.into()),
        }
    }
    let leaf = names.pop().ok_or(io::ErrorKind::InvalidInput)?;
    Ok((names, leaf))
}

impl Destination {
    pub(in crate::agent_console::share) fn open(
        root: &Path,
        key: &str,
        create: bool,
    ) -> io::Result<Self> {
        let (names, leaf) = components(key)?;
        // The explicitly selected root may itself be a symlink; pin its target.
        let parent = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC)
            .open(root)?;
        Self::walk(parent, names, leaf, create)
    }

    pub(in crate::agent_console::share) fn from_root(
        root: File,
        key: &str,
        create: bool,
    ) -> io::Result<Self> {
        let (names, leaf) = components(key)?;
        Self::walk(root, names, leaf, create)
    }

    /// After a synced delete of `key`: the folders above it that are now empty, deepest first,
    /// up to but never the root. rmdir removes only an empty folder and never follows a symlink.
    /// Finder's `.DS_Store` (view settings, never synced) is in every folder it opened, so a
    /// folder that holds more is retried without it; one that still holds other files keeps them
    /// and loses only its view settings, which Finder writes again.
    pub(in crate::agent_console::share) fn remove_empty_folders(root: File, key: &str) {
        let Ok((names, _)) = components(key) else { return };
        let mut folders = vec![root];
        for part in &names {
            let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW;
            match open_at(folders.last().expect("the root"), part, flags, 0) {
                Ok(folder) => folders.push(folder),
                Err(_) => break,
            }
        }
        // folders[i] holds names[i], so folders[i + 1] is the folder names[i] names.
        let rmdir = |parent: &File, part: &CString| {
            // SAFETY: a live directory descriptor and one NUL-terminated name, never recursive.
            unsafe { libc::unlinkat(parent.as_raw_fd(), part.as_ptr(), libc::AT_REMOVEDIR) == 0 }
        };
        for (i, (parent, part)) in folders.iter().zip(&names).enumerate().rev() {
            if rmdir(parent, part) {
                continue;
            }
            let not_empty = io::Error::last_os_error().raw_os_error() == Some(libc::ENOTEMPTY);
            let Some(folder) = folders.get(i + 1).filter(|_| not_empty) else { break };
            // SAFETY: as above, a file name in that folder.
            unsafe { libc::unlinkat(folder.as_raw_fd(), c".DS_Store".as_ptr(), 0) };
            if !rmdir(parent, part) {
                break;
            }
        }
    }

    fn walk(
        mut parent: File,
        names: Vec<CString>,
        leaf: CString,
        create: bool,
    ) -> io::Result<Self> {
        for part in names {
            let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW;
            parent = match open_at(&parent, &part, flags, 0) {
                Ok(file) => file,
                Err(e) if create && e.kind() == io::ErrorKind::NotFound => {
                    // Owner-only and marked before anything is published in it.
                    directory::open_or_create(&parent, &part, flags)?
                }
                Err(e) => return Err(e),
            };
        }
        Ok(Self { parent, leaf })
    }
}
