//! Start descriptor-relative destination traversal from a path or verified root.

use super::{directory, name, open_at, Destination};
use std::ffi::CString;
use std::fs::{File, OpenOptions};
use std::io;
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
