#![cfg(unix)]

use super::super::raw_nonblocking_tcp_socket;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

fn status_flags(fd: &OwnedFd) -> i32 {
    // SAFETY: fd owns a live descriptor; F_GETFL reads no third argument.
    let flags = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFL) };
    assert!(flags >= 0, "F_GETFL: {}", std::io::Error::last_os_error());
    flags
}

#[test]
fn raw_tcp_socket_adds_only_o_nonblocking_before_connect() {
    // SAFETY: socket takes integer constants; the result is checked below.
    let plain = unsafe { libc::socket(libc::AF_INET, libc::SOCK_STREAM, 0) };
    assert!(plain >= 0, "socket: {}", std::io::Error::last_os_error());
    // SAFETY: the new descriptor is transferred exactly once.
    let plain = unsafe { OwnedFd::from_raw_fd(plain) };
    let expected = status_flags(&plain) | libc::O_NONBLOCK;
    // Keep every descriptor open so each socket gets a new number; a
    // descriptor-dependent F_SETFL argument then cannot pass by chance.
    let mut sockets = Vec::new();
    for _ in 0..8 {
        let raw = raw_nonblocking_tcp_socket().expect("create TCP socket");
        // SAFETY: the successful constructor transfers one owned descriptor.
        let socket = unsafe { OwnedFd::from_raw_fd(raw) };
        let flags = status_flags(&socket);
        assert_eq!(
            flags, expected,
            "fd {raw} flags {flags:#x}, expected {expected:#x}"
        );
        sockets.push(socket);
    }
}

/// Windows' keepalives are answered by the NAT, so only the Mac's own can notice a connection
/// whose far end went away while it was idle (a network change, sleep): it must be on.
#[cfg(target_os = "macos")]
#[test]
fn outbound_connections_use_the_macs_tcp_keepalive() {
    use super::super::{
        nonblocking_tcp_connect, HOST_KEEPALIVE_IDLE_S, HOST_KEEPALIVE_INTERVAL_S,
        HOST_KEEPALIVE_PROBES,
    };
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let stream = nonblocking_tcp_connect(std::net::Ipv4Addr::LOCALHOST, port).unwrap();
    let option = |level, name| {
        let mut value: libc::c_int = -1;
        let mut len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
        // SAFETY: value and len are valid for the call; the fd is the stream's live socket.
        let rc = unsafe {
            libc::getsockopt(
                stream.as_raw_fd(),
                level,
                name,
                (&mut value as *mut libc::c_int).cast(),
                &mut len,
            )
        };
        assert_eq!(rc, 0, "getsockopt: {}", std::io::Error::last_os_error());
        value
    };
    assert_ne!(option(libc::SOL_SOCKET, libc::SO_KEEPALIVE), 0);
    assert_eq!(
        option(libc::IPPROTO_TCP, libc::TCP_KEEPALIVE),
        HOST_KEEPALIVE_IDLE_S
    );
    assert_eq!(
        option(libc::IPPROTO_TCP, libc::TCP_KEEPINTVL),
        HOST_KEEPALIVE_INTERVAL_S
    );
    assert_eq!(
        option(libc::IPPROTO_TCP, libc::TCP_KEEPCNT),
        HOST_KEEPALIVE_PROBES
    );
}
