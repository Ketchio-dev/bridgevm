//! Peer credential check for accepted daemon connections.
//!
//! The socket is created 0600 inside a 0700 directory, which is necessary but
//! not sufficient. Permissions are checked by the kernel at `connect` time
//! against the path; they say nothing about a descriptor that was passed to
//! another process, inherited across a fork, or reached before the mode was
//! applied. Since the daemon starts and stops VMs and touches guest media, it
//! should not act on a request whose sender it never verified.
//!
//! `getpeereid` asks the kernel who is on the other end of *this* connection,
//! which is the question that actually matters. It is checked before the
//! request is decoded, so a foreign peer's bytes never reach the parser.

use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;

extern "C" {
    fn getpeereid(fd: i32, euid: *mut u32, egid: *mut u32) -> i32;
}

/// Who is on the other end of a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PeerCredentials {
    pub(crate) uid: u32,
    pub(crate) gid: u32,
}

/// Why a connection was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PeerRejection {
    /// The peer is a different user.
    ForeignUid { peer: u32, expected: u32 },
    /// The kernel would not report the peer at all. Fail closed: an
    /// unidentifiable peer is refused rather than assumed to be us.
    Unidentifiable,
}

impl std::fmt::Display for PeerRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PeerRejection::ForeignUid { peer, expected } => write!(
                f,
                "refusing request from uid {peer}; this daemon only serves uid {expected}"
            ),
            PeerRejection::Unidentifiable => {
                write!(f, "refusing request from an unidentifiable peer")
            }
        }
    }
}

/// Read the peer's credentials from an accepted stream.
pub(crate) fn peer_credentials(stream: &UnixStream) -> Option<PeerCredentials> {
    let mut uid = 0u32;
    let mut gid = 0u32;
    // SAFETY: `stream` owns a valid descriptor for the duration of the call,
    // and both out-parameters are live locals.
    let status = unsafe { getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) };
    (status == 0).then_some(PeerCredentials { uid, gid })
}

/// Decide whether to serve a peer. Pure, so the policy is testable without a
/// second user account.
pub(crate) fn authorize(
    peer: Option<PeerCredentials>,
    expected_uid: u32,
) -> Result<PeerCredentials, PeerRejection> {
    let Some(peer) = peer else {
        return Err(PeerRejection::Unidentifiable);
    };
    if peer.uid != expected_uid {
        return Err(PeerRejection::ForeignUid {
            peer: peer.uid,
            expected: expected_uid,
        });
    }
    Ok(peer)
}

/// Refuse a connection whose peer is not this user, before the request is
/// decoded. Socket permissions are checked against the path at connect time
/// and say nothing about an inherited or passed descriptor.
pub(crate) fn refuse_foreign_peer(stream: &UnixStream) -> anyhow::Result<()> {
    if let Err(rejection) = authorize_stream(stream) {
        eprintln!("bridgevmd {rejection}");
        anyhow::bail!("{rejection}");
    }
    Ok(())
}

/// Authorize an accepted stream against the uid this daemon runs as.
pub(crate) fn authorize_stream(stream: &UnixStream) -> Result<PeerCredentials, PeerRejection> {
    authorize(peer_credentials(stream), current_uid())
}

pub(crate) fn current_uid() -> u32 {
    // SAFETY: getuid cannot fail and takes no arguments.
    unsafe { libc_getuid() }
}

extern "C" {
    #[link_name = "getuid"]
    fn libc_getuid() -> u32;
}

#[cfg(test)]
#[path = "peer_credentials_tests.rs"]
mod tests;
