//! Signals are sent only while the caller retains an unreaped direct Child.

use std::io;

pub(super) fn terminate(pid: u32) -> io::Result<()> {
    // SAFETY: the private caller owns the unreaped Child and has not delegated wait.
    let result = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
