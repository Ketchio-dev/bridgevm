//! A volume without atomic directory exchange keeps its directory and says why.

use std::io;
use std::path::Path;

/// The exchange's ENOTSUP names the operation and the volume type; every
/// other error passes through unchanged, raw OS error included.
pub(super) fn refuse(error: io::Error, destination: &Path) -> io::Error {
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
#[path = "snapshot_publish_unsupported_tests.rs"]
mod tests;
