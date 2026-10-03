use super::{validated, PeerCredentials};

#[test]
fn complete_kernel_credentials_preserve_both_identity_fields() {
    let peer = PeerCredentials { uid: 502, gid: 30 };
    assert_eq!(validated(0, 12, 12, peer), Some(peer));
}

#[test]
fn failed_kernel_queries_cannot_authorize_initialized_credentials() {
    let peer = PeerCredentials { uid: 0, gid: 0 };
    for status in [-1, 1] {
        assert_eq!(validated(status, 12, 12, peer), None);
    }
}

#[test]
fn partial_or_oversized_kernel_credentials_fail_closed() {
    let peer = PeerCredentials { uid: 502, gid: 30 };
    for received in [0, 11, 13] {
        assert_eq!(validated(0, received, 12, peer), None);
    }
}
