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
