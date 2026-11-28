use std::{
    io,
    net::{Ipv4Addr as StdIpv4Addr, TcpStream},
    os::fd::{FromRawFd, RawFd},
};

pub(crate) fn would_block(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
    )
}

pub(crate) use super::tcp_connection_state::tcp_connect_error;

#[cfg(target_os = "macos")]
#[derive(Debug)]
pub(crate) struct RawIcmpSocket {
    pub(crate) fd: RawFd,
}

#[cfg(target_os = "macos")]
impl RawIcmpSocket {
    pub(crate) fn new_nonblocking() -> io::Result<Self> {
        // SAFETY: socket is called with valid constants; errors are returned via errno.
        let fd = unsafe { socket(AF_INET, SOCK_DGRAM, IPPROTO_ICMP) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        if let Err(err) = set_raw_icmp_socket_nonblocking(fd) {
            let _ = raw_close(fd);
            return Err(err);
        }
        Ok(Self { fd })
    }

    pub(crate) fn send_to(&self, message: &[u8], dst: StdIpv4Addr) -> io::Result<usize> {
        let addr = RawSockAddrIn::new(dst, 0);
        // SAFETY: message and addr point to valid memory for the duration of the call.
        let rc = unsafe {
            sendto(
                self.fd,
                message.as_ptr().cast(),
                message.len(),
                0,
                (&addr as *const RawSockAddrIn).cast::<RawSockAddr>(),
                std::mem::size_of::<RawSockAddrIn>() as RawSockLen,
            )
        };
        if rc >= 0 {
            Ok(rc as usize)
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub(crate) fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, StdIpv4Addr)> {
        // SAFETY: zeroed sockaddr_in is a valid writable buffer for recvfrom.
        let mut addr = unsafe { std::mem::zeroed::<RawSockAddrIn>() };
        let mut len = std::mem::size_of::<RawSockAddrIn>() as RawSockLen;
        // SAFETY: buf and addr point to valid writable memory for the duration of the call.
        let rc = unsafe {
            recvfrom(
                self.fd,
                buf.as_mut_ptr().cast(),
                buf.len(),
                0,
                (&mut addr as *mut RawSockAddrIn).cast::<RawSockAddr>(),
                &mut len,
            )
        };
        if rc >= 0 {
            Ok((rc as usize, StdIpv4Addr::from(addr.sin_addr.to_ne_bytes())))
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn raw_icmp_nonblocking_flags(flags: i32) -> i32 {
    flags | O_NONBLOCK
}

#[cfg(target_os = "macos")]
pub(crate) fn set_raw_icmp_socket_nonblocking(fd: RawFd) -> io::Result<()> {
    // SAFETY: fcntl is declared with C's variadic ABI, which is required for
    // F_SETFL on arm64 macOS.
    let flags = unsafe { fcntl(fd, F_GETFL) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fcntl operates on the socket fd created above.
    if unsafe { fcntl(fd, F_SETFL, raw_icmp_nonblocking_flags(flags)) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
impl Drop for RawIcmpSocket {
    fn drop(&mut self) {
        let _ = raw_close(self.fd);
    }
}

#[cfg(not(target_os = "macos"))]
#[derive(Debug)]
pub(crate) struct RawIcmpSocket;

#[cfg(not(target_os = "macos"))]
impl RawIcmpSocket {
    pub(crate) fn new_nonblocking() -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "unprivileged ICMP datagram sockets are only enabled on macOS",
        ))
    }

    pub(crate) fn send_to(&self, _message: &[u8], _dst: StdIpv4Addr) -> io::Result<usize> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "icmp unsupported",
        ))
    }

    pub(crate) fn recv_from(&self, _buf: &mut [u8]) -> io::Result<(usize, StdIpv4Addr)> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "icmp unsupported",
        ))
    }
}

#[cfg(unix)]
pub(crate) fn nonblocking_tcp_connect(dst: StdIpv4Addr, port: u16) -> io::Result<TcpStream> {
    let fd = raw_nonblocking_tcp_socket()?;
    set_host_keepalive(fd);
    let connect_result = raw_connect_ipv4(fd, dst, port);
    if let Err(err) = connect_result {
        if !raw_connect_in_progress(&err) {
            let _ = raw_close(fd);
            return Err(err);
        }
    }
    // SAFETY: fd is a valid TCP socket created above and ownership is moved into TcpStream.
    let stream = unsafe { TcpStream::from_raw_fd(fd) };
    stream.set_nonblocking(true)?;
    Ok(stream)
}

#[cfg(not(unix))]
pub(crate) fn nonblocking_tcp_connect(dst: StdIpv4Addr, port: u16) -> io::Result<TcpStream> {
    let stream = TcpStream::connect(SocketAddrV4::new(dst, port))?;
    stream.set_nonblocking(true)?;
    Ok(stream)
}

/// The NAT answers Windows' own keepalive probes itself (host_socket.rs), so a connection whose
/// far end went away (the Mac changed networks, or slept and woke elsewhere) looked alive to
/// Windows for as long as it stayed idle: mail stopped arriving, an RDP or SSH session froze. With
/// the Mac's keepalive on, an idle connection that stops answering fails after about two minutes,
/// and the guest gets a reset (TcpResetReason::HostRead) and reconnects. Best effort.
#[cfg(target_os = "macos")]
pub(crate) fn set_host_keepalive(fd: RawFd) {
    for (level, name, value) in [
        (libc::SOL_SOCKET, libc::SO_KEEPALIVE, 1),
        (
            libc::IPPROTO_TCP,
            libc::TCP_KEEPALIVE,
            HOST_KEEPALIVE_IDLE_S,
        ),
        (
            libc::IPPROTO_TCP,
            libc::TCP_KEEPINTVL,
            HOST_KEEPALIVE_INTERVAL_S,
        ),
        (libc::IPPROTO_TCP, libc::TCP_KEEPCNT, HOST_KEEPALIVE_PROBES),
    ] {
        let value: libc::c_int = value;
        // SAFETY: fd is a TCP socket; value is a c_int that outlives the call.
        unsafe {
            libc::setsockopt(
                fd,
                level,
                name,
                (&value as *const libc::c_int).cast(),
                std::mem::size_of::<libc::c_int>() as libc::socklen_t,
            )
        };
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn set_host_keepalive(_fd: RawFd) {}

pub(crate) const HOST_KEEPALIVE_IDLE_S: i32 = 60;
pub(crate) const HOST_KEEPALIVE_INTERVAL_S: i32 = 10;
pub(crate) const HOST_KEEPALIVE_PROBES: i32 = 6;

#[cfg(unix)]
pub(crate) fn raw_nonblocking_tcp_socket() -> io::Result<RawFd> {
    // SAFETY: socket is called with valid constants; errors are returned via errno.
    let fd = unsafe { socket(AF_INET, SOCK_STREAM, 0) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fcntl operates on the socket fd created above.
    let flags = unsafe { fcntl(fd, F_GETFL) };
    if flags < 0 || unsafe { fcntl(fd, F_SETFL, flags | O_NONBLOCK) } < 0 {
        let err = io::Error::last_os_error();
        let _ = raw_close(fd);
        return Err(err);
    }
    Ok(fd)
}

#[cfg(unix)]
pub(crate) fn raw_connect_ipv4(fd: RawFd, dst: StdIpv4Addr, port: u16) -> io::Result<()> {
    let addr = RawSockAddrIn::new(dst, port);
    // SAFETY: addr points to a properly initialized IPv4 sockaddr for this platform.
    let rc = unsafe {
        connect(
            fd,
            (&addr as *const RawSockAddrIn).cast::<RawSockAddr>(),
            std::mem::size_of::<RawSockAddrIn>() as RawSockLen,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(unix)]
pub(crate) fn raw_connect_in_progress(err: &io::Error) -> bool {
    matches!(
        err.raw_os_error(),
        Some(ERRNO_EINPROGRESS) | Some(ERRNO_EALREADY) | Some(ERRNO_EWOULDBLOCK)
    )
}

#[cfg(unix)]
pub(crate) fn raw_close(fd: RawFd) -> io::Result<()> {
    // SAFETY: close is called with a raw fd; errors are surfaced through errno.
    let rc = unsafe { close(fd) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "macos")]
pub(crate) type RawSockLen = u32;
#[cfg(target_os = "macos")]
#[repr(C)]
pub(crate) struct RawSockAddr {
    pub(crate) sa_len: u8,
    pub(crate) sa_family: u8,
    pub(crate) sa_data: [u8; 14],
}
#[cfg(target_os = "macos")]
#[repr(C)]
pub(crate) struct RawSockAddrIn {
    pub(crate) sin_len: u8,
    pub(crate) sin_family: u8,
    pub(crate) sin_port: u16,
    pub(crate) sin_addr: u32,
    pub(crate) sin_zero: [u8; 8],
}
#[cfg(target_os = "macos")]
impl RawSockAddrIn {
    pub(crate) fn new(dst: StdIpv4Addr, port: u16) -> Self {
        Self {
            sin_len: std::mem::size_of::<Self>() as u8,
            sin_family: AF_INET as u8,
            sin_port: port.to_be(),
            sin_addr: u32::from_ne_bytes(dst.octets()),
            sin_zero: [0; 8],
        }
    }
}

#[cfg(target_os = "linux")]
pub(crate) type RawSockLen = u32;
#[cfg(target_os = "linux")]
#[repr(C)]
pub(crate) struct RawSockAddr {
    pub(crate) sa_family: u16,
    pub(crate) sa_data: [u8; 14],
}
#[cfg(target_os = "linux")]
#[repr(C)]
pub(crate) struct RawSockAddrIn {
    pub(crate) sin_family: u16,
    pub(crate) sin_port: u16,
    pub(crate) sin_addr: u32,
    pub(crate) sin_zero: [u8; 8],
}
#[cfg(target_os = "linux")]
impl RawSockAddrIn {
    pub(crate) fn new(dst: StdIpv4Addr, port: u16) -> Self {
        Self {
            sin_family: AF_INET as u16,
            sin_port: port.to_be(),
            sin_addr: u32::from_ne_bytes(dst.octets()),
            sin_zero: [0; 8],
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) const AF_INET: i32 = 2;
#[cfg(target_os = "linux")]
pub(crate) const AF_INET: i32 = 2;
#[cfg(unix)]
pub(crate) const SOCK_STREAM: i32 = 1;
#[cfg(target_os = "macos")]
pub(crate) const SOCK_DGRAM: i32 = 2;
#[cfg(target_os = "macos")]
pub(crate) const IPPROTO_ICMP: i32 = 1;
#[cfg(unix)]
pub(crate) const F_GETFL: i32 = 3;
#[cfg(unix)]
pub(crate) const F_SETFL: i32 = 4;
#[cfg(target_os = "macos")]
pub(crate) const O_NONBLOCK: i32 = 0x0004;
#[cfg(target_os = "linux")]
pub(crate) const O_NONBLOCK: i32 = 0o4000;
#[cfg(target_os = "macos")]
pub(crate) const ERRNO_EINPROGRESS: i32 = 36;
#[cfg(target_os = "linux")]
pub(crate) const ERRNO_EINPROGRESS: i32 = 115;
#[cfg(target_os = "macos")]
pub(crate) const ERRNO_EALREADY: i32 = 37;
#[cfg(target_os = "linux")]
pub(crate) const ERRNO_EALREADY: i32 = 114;
#[cfg(target_os = "macos")]
pub(crate) const ERRNO_EWOULDBLOCK: i32 = 35;
#[cfg(target_os = "linux")]
pub(crate) const ERRNO_EWOULDBLOCK: i32 = 11;

#[cfg(unix)]
unsafe extern "C" {
    fn socket(domain: i32, ty: i32, protocol: i32) -> RawFd;
    fn connect(fd: RawFd, addr: *const RawSockAddr, len: RawSockLen) -> i32;
    // C fcntl is variadic; Apple arm64 passes variadic arguments on the stack.
    fn fcntl(fd: RawFd, cmd: i32, ...) -> i32;
    fn close(fd: RawFd) -> i32;
    #[cfg(target_os = "macos")]
    fn sendto(
        fd: RawFd,
        buf: *const std::ffi::c_void,
        len: usize,
        flags: i32,
        addr: *const RawSockAddr,
        addr_len: RawSockLen,
    ) -> isize;
    #[cfg(target_os = "macos")]
    fn recvfrom(
        fd: RawFd,
        buf: *mut std::ffi::c_void,
        len: usize,
        flags: i32,
        addr: *mut RawSockAddr,
        addr_len: *mut RawSockLen,
    ) -> isize;
}
