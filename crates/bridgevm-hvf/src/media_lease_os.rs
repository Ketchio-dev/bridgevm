use std::fs::{self, File};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::PathBuf;

pub(super) fn uid() -> u32 {
    // SAFETY: geteuid has no pointer arguments or additional preconditions.
    unsafe { libc::geteuid() }
}

pub(super) fn lock(file: &File, operation: libc::c_int) -> io::Result<()> {
    loop {
        // SAFETY: file owns a live descriptor for the duration of flock.
        if unsafe { libc::flock(file.as_raw_fd(), operation) } == 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

pub(super) fn private_root(uid: u32) -> io::Result<PathBuf> {
    // GUI processes and sanitized queue workers share this fixed namespace.
    let root = PathBuf::from(format!("/tmp/bridgevm-media-leases-{uid}"));
    match fs::DirBuilder::new().mode(0o700).create(&root) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    let metadata = fs::symlink_metadata(&root)?;
    if !metadata.file_type().is_dir()
        || metadata.uid() != uid
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(io::Error::other("unsafe media lease directory"));
    }
    Ok(root)
}
