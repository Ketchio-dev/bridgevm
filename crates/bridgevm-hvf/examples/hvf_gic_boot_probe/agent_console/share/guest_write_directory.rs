//! Directories a guest path causes to be created on the Mac (follow-up of
//! 2026-10-23, conservative choice). They are created owner-only 0700 before
//! the umask, and they get a fresh com.apple.quarantine mark like guest files,
//! set through the new directory's descriptor before anything is published in
//! it: an app bundle is a directory, and LaunchServices reads a bundle's own
//! mark. A directory whose mark cannot be set is removed again and the write
//! is refused, for good when the cause stays (guest_write_mark.rs). A directory
//! that already exists, someone else's included, is used as it is.

use super::mark::set_mark;
use super::open_at;
use super::seal_host::NativeSealHost;
use bridgevm_hvf::host_entropy::HostEntropy;
use std::ffi::CStr;
use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;

/// Mode a directory created for a guest path gets, before the process umask.
pub(super) const NEW_DIR_MODE: libc::mode_t = 0o700;

/// The directory `name` in `parent`, opened with `flags`: created owner-only
/// and marked when it is missing.
pub(super) fn open_or_create(parent: &File, name: &CStr, flags: i32) -> io::Result<File> {
    // SAFETY: live directory descriptor and NUL-terminated child name.
    if unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), NEW_DIR_MODE) } < 0 {
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::AlreadyExists {
            return Err(error);
        }
        return open_at(parent, name, flags, 0);
    }
    let host = &mut NativeSealHost { directory: parent };
    let opened = open_at(parent, name, flags, 0).and_then(|directory| {
        set_mark(&directory, &mut HostEntropy::new(), host)?;
        Ok(directory)
    });
    if opened.is_err() {
        // SAFETY: removes only an empty directory under this name, never recursively.
        unsafe { libc::unlinkat(parent.as_raw_fd(), name.as_ptr(), libc::AT_REMOVEDIR) };
    }
    opened
}
