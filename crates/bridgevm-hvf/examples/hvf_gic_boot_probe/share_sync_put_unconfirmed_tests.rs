//! After a PUT that was not confirmed (the guest may have truncated its copy),
//! a host file matching the record's hash is re-sent, and the guest copy is
//! never pulled back until a PUT is confirmed.

use super::*;

#[test]
fn refused_put_then_host_restored_to_the_synced_bytes_is_resent_not_pulled() {
    let mut sync = synced("a.txt", b"old");
    let scan = |size, mtime| vec![host_file("a.txt", size, mtime)];
    let get = || {
        vec![SyncAction::Get {
            name: "a.txt".into(),
        }]
    };
    let partial = || vec![ls_file("a.txt", 2, "g-partial")];
    assert_eq!(sync.on_host_scan(scan(4, 20)), get());
    assert!(sync
        .on_host_file("a.txt".into(), b"new!".to_vec(), 20)
        .is_some());
    sync.on_put_rejected("a.txt");
    // The guest truncated its copy first. `git checkout` then restores the
    // last-synced bytes with a new mtime.
    assert_eq!(sync.on_host_scan(scan(3, 30)), get());
    let push = sync.on_host_file("a.txt".into(), b"old".to_vec(), 30);
    assert_eq!(
        push.map(|push| push.bytes),
        Some(b"old".to_vec()),
        "host wins"
    );
    assert_eq!(sync.on_guest_listing(partial()), Vec::new(), "unconfirmed");
    sync.on_put_rejected("a.txt");
    for _ in 0..3 {
        assert_eq!(sync.on_host_scan(scan(3, 30)), Vec::new(), "retried");
        assert_eq!(sync.on_guest_listing(partial()), Vec::new(), "pulled");
    }
    // Retried on the next host change; confirmed, the guest copy syncs again.
    assert_eq!(sync.on_host_scan(scan(3, 40)), get());
    let push = sync
        .on_host_file("a.txt".into(), b"old".to_vec(), 40)
        .unwrap();
    sync.on_put_ok("a.txt".into(), 3, push.hash);
    assert_eq!(
        sync.on_guest_listing(vec![ls_file("a.txt", 3, "g3")]),
        Vec::new()
    );
    assert_eq!(sync.on_host_scan(scan(3, 40)), get());
    assert!(sync
        .on_host_file("a.txt".into(), b"old".to_vec(), 40)
        .is_none());
    assert_eq!(
        sync.on_guest_listing(vec![ls_file("a.txt", 5, "g4")]),
        get()
    );
}

#[test]
fn refused_put_then_host_restored_with_the_synced_mtime_is_resent_once() {
    let mut sync = synced("a.txt", b"old");
    let scan = |size, mtime| vec![host_file("a.txt", size, mtime)];
    let get = vec![SyncAction::Get {
        name: "a.txt".into(),
    }];
    assert_eq!(sync.on_host_scan(scan(4, 20)), get);
    assert!(sync
        .on_host_file("a.txt".into(), b"new!".to_vec(), 20)
        .is_some());
    sync.on_put_rejected("a.txt");
    // `cp -p` restores the synced bytes and mtime before the next scan.
    assert_eq!(sync.on_host_scan(scan(3, 10)), get);
    let push = sync.on_host_file("a.txt".into(), b"old".to_vec(), 10);
    assert_eq!(push.map(|push| push.bytes), Some(b"old".to_vec()));
    sync.on_put_rejected("a.txt");
    for _ in 0..3 {
        assert_eq!(sync.on_host_scan(scan(3, 10)), Vec::new(), "retried");
        let partial = vec![ls_file("a.txt", 2, "g-partial")];
        assert_eq!(sync.on_guest_listing(partial), Vec::new(), "pulled");
        assert_eq!(sync.on_guest_listing(Vec::new()), Vec::new(), "deleted");
    }
}
