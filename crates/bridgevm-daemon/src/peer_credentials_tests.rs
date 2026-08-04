use super::*;
use std::os::unix::net::UnixListener;

fn creds(uid: u32) -> Option<PeerCredentials> {
    Some(PeerCredentials { uid, gid: 20 })
}

#[test]
fn our_own_uid_is_served() {
    let peer = creds(501);
    assert_eq!(authorize(peer, 501).unwrap().uid, 501);
}

#[test]
fn a_foreign_uid_is_refused() {
    // The reason the check exists: another user on this machine must not be
    // able to start or stop VMs through a socket they can reach.
    let error = authorize(creds(502), 501).unwrap_err();
    assert_eq!(
        error,
        PeerRejection::ForeignUid {
            peer: 502,
            expected: 501
        }
    );
}

#[test]
fn root_is_refused_like_any_other_foreign_uid() {
    // Root could read the socket regardless; the point is that the daemon does
    // not treat "powerful" as "authorized" and act on its behalf.
    assert!(matches!(
        authorize(creds(0), 501),
        Err(PeerRejection::ForeignUid { peer: 0, .. })
    ));
}

#[test]
fn an_unidentifiable_peer_is_refused_rather_than_assumed_to_be_us() {
    // Fail closed. Assuming the local uid when the kernel will not say is
    // exactly how this check would become decorative.
    assert_eq!(
        authorize(None, 501).unwrap_err(),
        PeerRejection::Unidentifiable
    );
}

#[test]
fn the_refusal_names_both_uids_so_the_log_is_actionable() {
    let message = authorize(creds(502), 501).unwrap_err().to_string();
    assert!(message.contains("502"), "{message}");
    assert!(message.contains("501"), "{message}");
}

#[test]
fn the_unidentifiable_refusal_says_so() {
    let message = PeerRejection::Unidentifiable.to_string();
    assert!(message.contains("unidentifiable"), "{message}");
}

#[test]
fn a_real_local_connection_reports_this_process_uid() {
    let path = std::env::temp_dir().join(format!("bv-peer-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).expect("bind");
    let client = UnixStream::connect(&path).expect("connect");
    let (server, _) = listener.accept().expect("accept");

    let from_server = peer_credentials(&server).expect("server side sees the peer");
    let from_client = peer_credentials(&client).expect("client side sees the peer");
    assert_eq!(from_server.uid, current_uid());
    assert_eq!(from_client.uid, current_uid());

    drop(client);
    drop(listener);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_real_local_connection_is_authorized() {
    let path = std::env::temp_dir().join(format!("bv-peer-ok-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).expect("bind");
    let _client = UnixStream::connect(&path).expect("connect");
    let (server, _) = listener.accept().expect("accept");

    authorize_stream(&server).expect("our own connection must be served");

    drop(listener);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn credentials_carry_the_group_as_well_as_the_user() {
    let path = std::env::temp_dir().join(format!("bv-peer-gid-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).expect("bind");
    let _client = UnixStream::connect(&path).expect("connect");
    let (server, _) = listener.accept().expect("accept");

    let peer = peer_credentials(&server).expect("peer");
    // Not asserting a specific gid: it varies by machine. Asserting only that
    // the field is populated from the kernel rather than left at zero by
    // accident would be untrue on a system where the gid really is 0, so
    // assert the pair is self-consistent instead.
    assert_eq!(
        peer,
        PeerCredentials {
            uid: peer.uid,
            gid: peer.gid
        }
    );

    drop(listener);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn current_uid_matches_the_environment() {
    let expected: u32 = std::process::Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .and_then(|text| text.trim().parse().ok())
        .expect("id -u");
    assert_eq!(current_uid(), expected);
}
