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

/// Clone `src` to `dst` with APFS copy-on-write, or None if that is not
/// possible here.
///
/// Measured: cloning the 64 GiB Windows image takes 2ms and adds no used
/// bytes, against minutes and a second full copy for a byte-by-byte read and
/// write. None means "fall back to copying" -- a non-APFS volume, a
/// cross-volume destination, or any other refusal from the kernel.
#[cfg(target_os = "macos")]
pub(super) fn clone_file(src: &Path, dst: &Path) -> Option<()> {
    use std::os::unix::ffi::OsStrExt;
    let c_src = std::ffi::CString::new(src.as_os_str().as_bytes()).ok()?;
    let c_dst = std::ffi::CString::new(dst.as_os_str().as_bytes()).ok()?;
    // SAFETY: both strings are valid and NUL-terminated, and clonefile only
    // reads them.
    let rc = unsafe { libc::clonefile(c_src.as_ptr(), c_dst.as_ptr(), 0) };
    (rc == 0).then_some(())
}

/// No copy-on-write clone off macOS; callers fall back to copying.
#[cfg(not(target_os = "macos"))]
pub(super) fn clone_file(_src: &Path, _dst: &Path) -> Option<()> {
    None
}
