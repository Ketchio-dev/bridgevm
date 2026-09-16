//! Nonblocking local startup I/O, sharing one monotonic readiness deadline.

use crate::RuntimeControl;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::ChildStdin;
use std::time::Instant;

pub(in crate::vtpm) fn check(deadline: Instant, control: &RuntimeControl<'_>) -> io::Result<()> {
    if control.is_cancelled() {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "runtime cancelled",
        ));
    }
    if Instant::now() >= deadline {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "5s startup deadline",
        ));
    }
    Ok(())
}

fn configure(fd: RawFd) -> io::Result<()> {
    // SAFETY: caller holds the sole live descriptor, and fcntl accesses no pointers.
    let status = unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        if flags < 0 || libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
            -1
        } else {
            libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC)
        }
    };
    if status < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn wait(fd: RawFd, event: i16, deadline: Instant, control: &RuntimeControl<'_>) -> io::Result<()> {
    loop {
        check(deadline, control)?;
        let mut descriptor = libc::pollfd {
            fd,
            events: event,
            revents: 0,
        };
        // SAFETY: poll receives one initialized pollfd valid for this call.
        let status = unsafe { libc::poll(&mut descriptor, 1, 10) };
        check(deadline, control)?;
        if status > 0 {
            return Ok(());
        }
        if status < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
            return Err(io::Error::last_os_error());
        }
    }
}

fn write_all(
    writer: &mut (impl Write + AsRawFd),
    mut data: &[u8],
    deadline: Instant,
    control: &RuntimeControl<'_>,
) -> io::Result<()> {
    while !data.is_empty() {
        check(deadline, control)?;
        match writer.write(data) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(count) => data = &data[count..],
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                wait(writer.as_raw_fd(), libc::POLLOUT, deadline, control)?
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    check(deadline, control)
}

pub(super) fn write_key(
    stdin: &mut ChildStdin,
    key: &[u8],
    deadline: Instant,
    control: &RuntimeControl<'_>,
) -> io::Result<()> {
    configure(stdin.as_raw_fd())?;
    write_all(stdin, key, deadline, control)
}

pub(in crate::vtpm) fn probe(
    path: &Path,
    deadline: Instant,
    control: &RuntimeControl<'_>,
) -> io::Result<bool> {
    check(deadline, control)?;
    let mut stream = match connect(path, deadline, control) {
        Ok(value) => value,
        Err(_) => {
            check(deadline, control)?;
            return Ok(false);
        }
    };
    if write_all(&mut stream, &1u32.to_be_bytes(), deadline, control).is_err() {
        check(deadline, control)?;
        return Ok(false);
    }
    let mut response = [0u8; 8];
    let mut offset = 0;
    while offset < response.len() {
        check(deadline, control)?;
        match stream.read(&mut response[offset..]) {
            Ok(0) => return Ok(false),
            Ok(count) => offset += count,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                wait(stream.as_raw_fd(), libc::POLLIN, deadline, control)?
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => return Ok(false),
        }
    }
    check(deadline, control)?;
    Ok(response[..4] == [0; 4] && response[4..] != [0; 4])
}

fn connect(path: &Path, deadline: Instant, control: &RuntimeControl<'_>) -> io::Result<UnixStream> {
    // SAFETY: sockaddr_un is a plain C address initialized to all-zero bytes.
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    let bytes = path.as_os_str().as_bytes();
    if bytes.len() >= address.sun_path.len() || bytes.contains(&0) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (slot, byte) in address.sun_path.iter_mut().zip(bytes) {
        *slot = *byte as libc::c_char;
    }
    let length = std::mem::size_of_val(&address) as libc::socklen_t;
    #[cfg(target_os = "macos")]
    {
        address.sun_len = length as u8;
    }
    // SAFETY: socket has no pointer inputs and its returned fd is immediately owned.
    let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fd is newly created and transferred to this single UnixStream owner.
    let stream = unsafe { UnixStream::from_raw_fd(fd) };
    configure(fd)?;
    // SAFETY: address and its byte length remain valid for this connect call.
    let status =
        unsafe { libc::connect(fd, (&address as *const libc::sockaddr_un).cast(), length) };
    if status < 0 {
        let error = io::Error::last_os_error();
        if !matches!(
            error.raw_os_error(),
            Some(libc::EINPROGRESS) | Some(libc::EAGAIN)
        ) {
            return Err(error);
        }
        wait(fd, libc::POLLOUT, deadline, control)?;
        let mut error = 0i32;
        let mut size = std::mem::size_of_val(&error) as libc::socklen_t;
        // SAFETY: error/size are live correctly sized outputs and fd is retained.
        if unsafe {
            libc::getsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_ERROR,
                (&mut error as *mut i32).cast(),
                &mut size,
            )
        } < 0
        {
            return Err(io::Error::last_os_error());
        }
        if error != 0 {
            return Err(io::Error::from_raw_os_error(error));
        }
    }
    check(deadline, control)?;
    Ok(stream)
}
