//! Atomic publication of an exclusively owned staged snapshot directory.

use std::fs;
use std::io;
use std::path::Path;

pub(super) fn publish(staged: &Path, destination: &Path) -> io::Result<()> {
    publish_with(staged, destination, exchange)
}

fn publish_with(
    staged: &Path,
    destination: &Path,
    swap: impl FnOnce(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    if staged == destination || staged.parent() != destination.parent() {
        return Err(io::Error::other(
            "snapshot publication needs distinct siblings",
        ));
    }
    if !fs::symlink_metadata(staged)?.file_type().is_dir() {
        return Err(io::Error::other("snapshot staging is not a directory"));
    }
    match fs::symlink_metadata(destination) {
        Ok(metadata) if metadata.file_type().is_dir() => swap(staged, destination)?,
        Ok(_) => return Err(io::Error::other("snapshot destination is not a directory")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => fs::rename(staged, destination)?,
        Err(error) => return Err(error),
    }
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    super::sync_dir(parent)?;
    // After exchange, the old complete snapshot occupies the staging name.
    // Keep it if durability was uncertain; deletion failure only leaves debris.
    let _ = fs::remove_dir_all(staged);
    Ok(())
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn exchange(left: &Path, right: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let left = CString::new(left.as_os_str().as_bytes())?;
    let right = CString::new(right.as_os_str().as_bytes())?;
    #[cfg(target_os = "macos")]
    // SAFETY: both owned C strings remain valid throughout the syscall.
    let result = unsafe { libc::renamex_np(left.as_ptr(), right.as_ptr(), libc::RENAME_SWAP) };
    #[cfg(target_os = "linux")]
    // SAFETY: renameat2 receives valid C strings and the documented exchange flag.
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            left.as_ptr(),
            libc::AT_FDCWD,
            right.as_ptr(),
            libc::RENAME_EXCHANGE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn exchange(_left: &Path, _right: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "atomic directory exchange unavailable",
    ))
}

#[cfg(test)]
#[path = "snapshot_publish_tests.rs"]
mod tests;
