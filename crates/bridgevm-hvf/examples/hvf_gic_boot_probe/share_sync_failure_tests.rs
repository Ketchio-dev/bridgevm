//! Failed guest-to-host writes must not become destructive reverse transfers.

use super::*;

fn guest() -> Vec<LsEntry> {
    vec![LsEntry { name: "a.txt".into(), size: 3, is_dir: false, mtime: "g1".into() }]
}

fn host(size: u64) -> Vec<HostFile> {
    vec![HostFile { name: "a.txt".into(), size, mtime_ms: 10 }]
}

fn failed() -> ShareSync {
    let mut sync = ShareSync::new(1024);
    sync.on_guest_listing(guest());
    assert!(matches!(sync.on_guest_file("a.txt".into(), b"new".to_vec(), None),
                     GuestFileOutcome::WriteHost(_)));
    sync.on_host_write_failed("a.txt");
    sync
}

#[test]
fn failed_write_never_uploads_old_or_partial_host_contents() {
    for bytes in [b"old".as_slice(), b"n".as_slice()] {
        let mut sync = failed();
        assert!(sync.on_host_scan(host(bytes.len() as u64)).is_empty());
        // Also guard a host-read action planned before the failure.
        assert!(sync.on_host_file("a.txt".into(), bytes.to_vec(), 10).is_none());
        assert!(sync.on_host_scan(Vec::new()).is_empty());
    }
}

#[test]
fn failed_write_retries_unchanged_guest_version_and_recovers() {
    let mut sync = failed();
    assert_eq!(sync.on_guest_listing(guest()), vec![SyncAction::Get { name: "a.txt".into() }]);
    assert!(matches!(sync.on_guest_file("a.txt".into(), b"new".to_vec(), None),
                     GuestFileOutcome::WriteHost(_)));
    sync.note_host_stat("a.txt", 10);
    assert!(sync.on_guest_listing(guest()).is_empty());
    assert!(sync.on_host_scan(host(3)).is_empty());
    assert_eq!(sync.on_host_scan(Vec::new()), vec![SyncAction::DeleteGuest { name: "a.txt".into() }]);
}

#[test]
fn unlisted_failed_write_does_not_resurrect_guest_from_stale_host_copy() {
    let mut sync = failed();
    assert!(sync.on_host_scan(host(3)).is_empty());
    assert!(sync.on_guest_listing(Vec::new()).is_empty());
    assert!(sync.on_host_scan(host(3)).is_empty());
    assert!(sync.on_host_file("a.txt".into(), b"old".to_vec(), 10).is_none());
    assert!(sync.on_host_scan(Vec::new()).is_empty());
    assert!(sync.on_guest_listing(Vec::new()).is_empty());
    sync.forget_absent_failed(|_| true);
    assert!(sync.records.is_empty());
}

#[test]
fn omitted_host_scan_does_not_prove_failed_destination_absent() {
    let mut sync = failed();
    sync.on_guest_listing(Vec::new());
    sync.on_host_scan(Vec::new());
    sync.forget_absent_failed(|_| false); // unreadable, symlinked or merely skipped
    assert!(sync.on_host_scan(host(3)).is_empty());
    assert!(sync.on_host_file("a.txt".into(), b"old".to_vec(), 10).is_none());
}
