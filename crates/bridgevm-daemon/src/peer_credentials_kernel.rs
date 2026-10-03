//! Platform-specific kernel connection credentials; unavailable identities fail closed.

use super::PeerCredentials;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;

#[cfg(not(target_os = "linux"))]
extern "C" {
    fn getpeereid(fd: i32, euid: *mut u32, egid: *mut u32) -> i32;
}

#[cfg(not(target_os = "linux"))]
pub(super) fn read(stream: &UnixStream) -> Option<PeerCredentials> {
    let mut uid = 0u32;
    let mut gid = 0u32;
    // SAFETY: the stream keeps the descriptor valid; both output locals are live.
    let status = unsafe { getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) };
    (status == 0).then_some(PeerCredentials { uid, gid })
}

#[cfg(target_os = "linux")]
pub(super) fn read(stream: &UnixStream) -> Option<PeerCredentials> {
    let mut credentials = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let expected = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let mut received = expected;
    // SAFETY: the stream keeps the descriptor valid. The initialized ucred
    // buffer and length live through the call, with capacity exactly expected.
    let status = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credentials as *mut libc::ucred).cast(),
            &mut received,
        )
    };
    validated(
        status,
        received,
        expected,
        PeerCredentials {
            uid: credentials.uid,
            gid: credentials.gid,
        },
    )
}

#[cfg(any(target_os = "linux", test))]
fn validated(
    status: libc::c_int,
    received: libc::socklen_t,
    expected: libc::socklen_t,
    peer: PeerCredentials,
) -> Option<PeerCredentials> {
    (status == 0 && received == expected).then_some(peer)
}

#[cfg(test)]
#[path = "peer_credentials_kernel_tests.rs"]
mod tests;
