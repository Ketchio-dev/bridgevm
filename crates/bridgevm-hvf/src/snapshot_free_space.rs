//! Free space on the volume a restore is about to stage a copy onto.
//!
//! Split out so the one `unsafe` call in the snapshot path sits by itself.

use std::path::Path;

/// Free bytes on the volume holding `path`, or None if it cannot be read.
///
/// None means "do not block the restore": a missing statfs is not evidence
/// that space is short, and the copy will fail loudly if it is.
pub(super) fn available_bytes(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: c_path is a valid NUL-terminated string, and statfs only writes
    // into the buffer we hand it.
    let stat = unsafe {
        let mut stat: libc::statfs = std::mem::zeroed();
        if libc::statfs(c_path.as_ptr(), &mut stat) != 0 {
            return None;
        }
        stat
    };
    // f_bsize is u32 on macOS and i64 on Linux, so neither try_from nor a
    // plain From satisfies both targets; a cast is the portable spelling.
    (stat.f_bavail as u64).checked_mul(stat.f_bsize as u64)
}
