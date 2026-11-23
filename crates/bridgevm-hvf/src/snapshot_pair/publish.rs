//! Atomic publication of an exclusively owned staged snapshot directory.
//!
//! An absent or empty destination is published with one same-parent rename.
//! Replacing a nonempty generation needs an atomic directory exchange; a volume
//! without one keeps its directory and says why.

use std::fs;
use std::io;
use std::path::Path;

pub(super) fn publish(staged: &Path, destination: &Path) -> io::Result<()> {
    publish_with(staged, destination, exchange)
}

pub(super) fn publish_with(
    staged: &Path,
    destination: &Path,
    swap: impl FnOnce(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    publish_using(
        staged,
        destination,
        swap,
        |a, b| fs::rename(a, b),
        super::sync_dir,
    )
}

fn publish_using(
    staged: &Path,
    destination: &Path,
    swap: impl FnOnce(&Path, &Path) -> io::Result<()>,
    rename: impl FnOnce(&Path, &Path) -> io::Result<()>,
    sync: impl FnOnce(&Path) -> io::Result<()>,
) -> io::Result<()> {
    if staged == destination || staged.parent() != destination.parent() {
        return Err(io::Error::other(
            "snapshot publication needs distinct siblings",
        ));
    }
    if !fs::symlink_metadata(staged)?.file_type().is_dir() {
        return Err(io::Error::other("snapshot staging is not a directory"));
    }
    let exchanged = match fs::symlink_metadata(destination) {
        Ok(metadata) if metadata.file_type().is_dir() => {
            if fs::read_dir(destination)?.next().transpose()?.is_none() {
                // One rename replaces only an empty directory. A late entry
                // makes the syscall refuse; never retry with exchange.
                rename(staged, destination)?;
                false
            } else {
                swap(staged, destination).map_err(|e| refuse_unsupported(e, destination))?;
                true
            }
        }
        Ok(_) => return Err(io::Error::other("snapshot destination is not a directory")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            rename(staged, destination)?;
            false
        }
        Err(error) => return Err(error),
    };
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    sync(parent)?;
    // After exchange, the old complete snapshot occupies the staging name.
    // Keep it if durability was uncertain; a kept copy refuses the next create.
    if exchanged {
        let _ = fs::remove_dir_all(staged);
    }
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

/// The exchange's ENOTSUP names the operation and the volume type; every
/// other error passes through unchanged, raw OS error included.
fn refuse_unsupported(error: io::Error, destination: &Path) -> io::Error {
    if error.raw_os_error() != Some(libc::ENOTSUP) {
        return error;
    }
    let volume = match volume_type(destination) {
        Some(name) => format!("a volume of type {name}"),
        None => "a volume".to_string(),
    };
    io::Error::new(
        io::ErrorKind::Unsupported,
        format!(
            "{} is on {volume} that cannot atomically exchange directories \
             (renamex_np RENAME_SWAP: {error}); it was left unchanged",
            destination.display()
        ),
    )
}

#[cfg(target_os = "macos")]
fn volume_type(path: &Path) -> Option<String> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut info = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: a valid C string and a writable statfs buffer, read only after success.
    let info = unsafe {
        if libc::statfs(path.as_ptr(), info.as_mut_ptr()) != 0 {
            return None;
        }
        info.assume_init()
    };
    let name: Vec<u8> = info
        .f_fstypename
        .iter()
        .take_while(|&&byte| byte != 0)
        .map(|&byte| byte as u8)
        .collect();
    (!name.is_empty()).then(|| String::from_utf8_lossy(&name).into_owned())
}

#[cfg(not(target_os = "macos"))]
fn volume_type(_path: &Path) -> Option<String> {
    None
}

#[cfg(test)]
mod tests;
