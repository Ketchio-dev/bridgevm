#![cfg(unix)]

use super::super::{raw_nonblocking_tcp_socket, RawSockAddrIn};
use std::io::ErrorKind;
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::time::Duration;

/// Reserve a port with an established four-tuple but no listening socket, so
/// a new SYN to it matches no PCB and is refused. A bound, unconnected Darwin
/// socket drops SYNs instead, and the connect ends in ETIMEDOUT, not RST.
pub(super) fn reserved_refusing_endpoint() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let reserved =
        TcpStream::connect_timeout(&listener.local_addr().unwrap(), Duration::from_secs(2))
            .unwrap();
    let (peer, _) = listener.accept().unwrap();
    (reserved, peer)
}

#[test]
fn refusal_fixture_retains_port_ownership_and_returns_connection_refused() {
    let (reserved, _peer) = reserved_refusing_endpoint();
    let address = reserved.local_addr().unwrap();
    let raw = raw_nonblocking_tcp_socket().unwrap();
    // SAFETY: the new socket descriptor is transferred exactly once.
    let competitor = unsafe { OwnedFd::from_raw_fd(raw) };
    let endpoint = RawSockAddrIn::new(Ipv4Addr::LOCALHOST, address.port());
    // An ordinary bind, without TcpListener's SO_REUSEADDR, must see the port held.
    // SAFETY: endpoint is a native IPv4 sockaddr with the exact passed size.
    let result = unsafe {
        libc::bind(
            competitor.as_raw_fd(),
            (&endpoint as *const RawSockAddrIn).cast(),
            std::mem::size_of_val(&endpoint) as _,
        )
    };
    assert_eq!(result, -1);
    assert_eq!(std::io::Error::last_os_error().kind(), ErrorKind::AddrInUse);
    assert_eq!(
        TcpStream::connect_timeout(&address, Duration::from_secs(2))
            .unwrap_err()
            .kind(),
        ErrorKind::ConnectionRefused
    );
}
