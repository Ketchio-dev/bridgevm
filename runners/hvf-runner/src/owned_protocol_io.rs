//! Only owned pipe descriptors are nonblocking; no global signal disposition changes.
use super::codec::{invalid, MAX_FRAME};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::time::{Duration, Instant};
pub(super) const IO_BOUND: Duration = Duration::from_secs(2);
pub(super) fn pipes() -> io::Result<(OwnedFd, OwnedFd)> {
    Ok((pipe(0)?, pipe(1)?))
}
fn pipe(fd: i32) -> io::Result<OwnedFd> {
    // SAFETY: fstat/fcntl operate on borrowed standard descriptors; only the duplicate is owned.
    unsafe {
        let mut stat: libc::stat = std::mem::zeroed();
        if libc::fstat(fd, &mut stat) != 0 {
            return Err(io::Error::last_os_error());
        }
        if stat.st_mode & libc::S_IFMT != libc::S_IFIFO {
            return Err(invalid());
        }
        let copy = libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3);
        if copy < 0 {
            return Err(io::Error::last_os_error());
        }
        let copy = OwnedFd::from_raw_fd(copy);
        let flags = libc::fcntl(copy.as_raw_fd(), libc::F_GETFL);
        if flags < 0 || libc::fcntl(copy.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(copy)
    }
}
pub(super) fn block_sigpipe_for_thread() -> io::Result<()> {
    // SAFETY: this mask applies only to the dedicated I/O thread. It spawns no children
    // and exits with SIGPIPE blocked; it never unblocks a pending broken-pipe signal.
    let result = unsafe {
        let mut set = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGPIPE);
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut())
    };
    if result != 0 {
        return Err(io::Error::from_raw_os_error(result));
    }
    Ok(())
}
pub(super) fn read(fd: i32, bytes: &mut [u8]) -> io::Result<Option<usize>> {
    // SAFETY: bytes is an exclusive live slice for the exact length supplied.
    let count = unsafe { libc::read(fd, bytes.as_mut_ptr().cast(), bytes.len()) };
    if count >= 0 {
        return Ok(Some(count as usize));
    }
    let error = io::Error::last_os_error();
    if matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
    ) {
        Ok(None)
    } else {
        Err(error)
    }
}
pub(super) fn write(fd: i32, bytes: &[u8]) -> io::Result<Option<usize>> {
    // SAFETY: bytes remains readable for its length throughout this synchronous write.
    let count = unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len()) };
    if count > 0 {
        return Ok(Some(count as usize));
    }
    let error = io::Error::last_os_error();
    if count < 0
        && matches!(
            error.kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
        )
    {
        Ok(None)
    } else {
        Err(error)
    }
}
pub(super) fn tick(input: i32, output: i32, wants_output: bool) -> io::Result<()> {
    // Darwin does not report a closed pipe reader when events=0. Probe
    // POLLOUT without waiting, then omit an idle writable descriptor from
    // the timed poll so an ordinary live owner cannot cause a busy loop.
    let mut output_probe = libc::pollfd {
        fd: output,
        events: libc::POLLOUT,
        revents: 0,
    };
    // SAFETY: one initialized descriptor is borrowed for a nonblocking poll.
    if unsafe { libc::poll(&mut output_probe, 1, 0) } < 0 {
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
    if output_probe.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
        return Err(invalid());
    }
    let mut rows = [
        libc::pollfd {
            fd: input,
            events: libc::POLLIN,
            revents: 0,
        },
        libc::pollfd {
            fd: if wants_output { output } else { -1 },
            events: libc::POLLOUT,
            revents: 0,
        },
    ];
    // SAFETY: exactly two initialized pollfd rows are borrowed during poll.
    if unsafe { libc::poll(rows.as_mut_ptr(), 2, 10) } < 0 {
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
    if rows[1].revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
        return Err(invalid());
    }
    Ok(())
}
pub(super) struct Reader {
    bytes: Vec<u8>,
    expected: Option<usize>,
    started: Option<Instant>,
}
impl Reader {
    pub fn new() -> Self {
        Self {
            bytes: Vec::with_capacity(MAX_FRAME + 4),
            expected: None,
            started: None,
        }
    }
    pub fn step(&mut self, fd: i32) -> io::Result<ReadResult> {
        if self.started.is_some_and(|at| at.elapsed() >= IO_BOUND) {
            return Err(invalid());
        }
        let target = self.expected.unwrap_or(4);
        let mut chunk = [0u8; MAX_FRAME];
        match read(fd, &mut chunk[..target - self.bytes.len()])? {
            None => return Ok(ReadResult::Pending),
            Some(0) if self.bytes.is_empty() => return Ok(ReadResult::Eof),
            Some(0) => return Err(invalid()),
            Some(count) => {
                self.started.get_or_insert_with(Instant::now);
                self.bytes.extend_from_slice(&chunk[..count]);
                chunk.fill(0);
            }
        }
        if self.started.is_some_and(|at| at.elapsed() >= IO_BOUND) {
            return Err(invalid());
        }
        if self.bytes.len() == 4 && self.expected.is_none() {
            let length =
                u32::from_be_bytes(self.bytes[..4].try_into().map_err(|_| invalid())?) as usize;
            if length == 0 || length > MAX_FRAME {
                return Err(invalid());
            }
            self.expected = Some(length + 4);
        }
        if Some(self.bytes.len()) == self.expected {
            let frame = self.bytes[4..].to_vec();
            self.bytes.fill(0);
            self.bytes.clear();
            self.expected = None;
            self.started = None;
            return Ok(ReadResult::Frame(frame));
        }
        Ok(ReadResult::Pending)
    }
}
impl Drop for Reader {
    fn drop(&mut self) {
        self.bytes.fill(0);
    }
}
pub(super) enum ReadResult {
    Pending,
    Eof,
    Frame(Vec<u8>),
}

#[cfg(test)]
#[path = "owned_protocol_transport_tests.rs"]
mod tests;
