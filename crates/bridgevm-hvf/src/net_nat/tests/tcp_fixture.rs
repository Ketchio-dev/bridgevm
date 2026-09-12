use super::super::super::raw_nonblocking_tcp_socket;
use std::net::{Ipv4Addr, TcpStream};
use std::os::fd::{AsRawFd, FromRawFd};

pub(super) fn reserved_nonlistening_socket() -> TcpStream {
    let fd = raw_nonblocking_tcp_socket().unwrap();
    // SAFETY: the newly created descriptor is uniquely transferred to TcpStream.
    let stream = unsafe { TcpStream::from_raw_fd(fd) };
    // SAFETY: sockaddr_in consists of integer fields; zero is a valid initializer.
    let mut address: libc::sockaddr_in = unsafe { std::mem::zeroed() };
    address.sin_family = libc::AF_INET as _;
    address.sin_addr.s_addr = u32::from_ne_bytes(Ipv4Addr::LOCALHOST.octets());
    #[cfg(target_os = "macos")]
    {
        address.sin_len = std::mem::size_of_val(&address) as _;
    }
    // SAFETY: address is initialized and its exact length is passed to bind.
    let result = unsafe {
        libc::bind(
            stream.as_raw_fd(),
            (&address as *const libc::sockaddr_in).cast(),
            std::mem::size_of_val(&address) as _,
        )
    };
    assert_eq!(result, 0, "bind: {}", std::io::Error::last_os_error());
    stream
}
