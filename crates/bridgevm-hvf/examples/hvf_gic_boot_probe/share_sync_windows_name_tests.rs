//! Case-only renames are the same file on Windows: no deletion, no re-fetch.
//! A refused PUT is retried on a host change and never pulled back meanwhile.

use super::*;

fn ls_file(name: &str, size: u64, mtime: &str) -> LsEntry {
    LsEntry { name: name.into(), size, is_dir: false, mtime: mtime.into() }
}

fn host_file(name: &str, size: u64, mtime_ms: u128) -> HostFile {
    HostFile { name: name.into(), size, mtime_ms }
}

fn synced(name: &str, bytes: &[u8]) -> ShareSync {
    let mut sync = ShareSync::new(512);
    let push = sync.on_host_file(name.into(), bytes.to_vec(), 10).unwrap();
    sync.on_put_ok(name.into(), bytes.len() as u64, push.hash);
    assert!(sync.on_guest_listing(vec![ls_file(name, bytes.len() as u64, "g1")]).is_empty());
    sync.note_host_stat(name, 10);
    sync
}

#[test]
fn windows_key_folds_case_and_trims_only_the_last_component() {
    use share_win_names::windows_key;
    assert_eq!(windows_key("Sub/Report.TXT"), windows_key("sub/report.txt"));
    assert_eq!(windows_key("foo. ."), "FOO");
    assert_eq!(windows_key("dir./x"), "DIR./X");
    assert_ne!(windows_key("a.txt"), windows_key("b.txt"));
}

#[test]
fn host_case_only_rename_moves_the_record_without_get_or_delete() {
    let mut sync = synced("report.txt", b"DATA");
    assert!(sync.on_host_scan(vec![host_file("Report.txt", 4, 10)]).is_empty());
    // The guest keeps `report.txt`; it lists as the renamed record.
    assert!(sync.on_guest_listing(vec![ls_file("report.txt", 4, "g1")]).is_empty());
    // A later host edit is pushed once and its landing is only stamped.
    assert_eq!(
        sync.on_host_scan(vec![host_file("Report.txt", 9, 20)]),
        vec![SyncAction::Get { name: "Report.txt".into() }]
    );
    let push = sync.on_host_file("Report.txt".into(), b"host edit".to_vec(), 20).unwrap();
    sync.on_put_ok("Report.txt".into(), 9, push.hash);
    assert!(sync.on_guest_listing(vec![ls_file("report.txt", 9, "g2")]).is_empty());
    // As for any PUT, the next scan re-reads once and finds nothing to push.
    let actions = sync.on_host_scan(vec![host_file("Report.txt", 9, 20)]);
    assert_eq!(actions, vec![SyncAction::Get { name: "Report.txt".into() }]);
    assert!(sync.on_host_file("Report.txt".into(), b"host edit".to_vec(), 20).is_none());
    assert!(sync.on_host_scan(vec![host_file("Report.txt", 9, 20)]).is_empty());
}

#[test]
fn guest_case_only_rename_lists_as_the_record_without_get_or_delete() {
    let mut sync = synced("report.txt", b"DATA");
    assert!(sync.on_guest_listing(vec![ls_file("Report.txt", 4, "g1")]).is_empty());
    assert!(sync.on_host_scan(vec![host_file("report.txt", 4, 10)]).is_empty());
    // A guest edit is fetched under the host's name, not as a second file.
    assert_eq!(
        sync.on_guest_listing(vec![ls_file("Report.txt", 5, "g2")]),
        vec![SyncAction::Get { name: "report.txt".into() }]
    );
}

#[test]
fn lone_trailing_dot_name_is_the_guest_file_windows_stores() {
    let mut sync = ShareSync::new(512);
    let push = sync.on_host_file("foo.".into(), b"BB".to_vec(), 20).unwrap();
    sync.on_put_ok("foo.".into(), 2, push.hash);
    assert!(sync.on_guest_listing(vec![ls_file("foo", 2, "g1")]).is_empty());
    sync.note_host_stat("foo.", 20);
    assert!(sync.on_host_scan(vec![host_file("foo.", 2, 20)]).is_empty());
    assert_eq!(
        sync.on_guest_listing(Vec::new()),
        vec![SyncAction::DeleteHost { name: "foo.".into() }]
    );
}

#[test]
fn ambiguous_twins_are_not_paired_with_a_vanished_record() {
    let mut sync = synced("a.txt", b"A");
    sync.on_host_scan(vec![host_file("A.txt", 1, 10), host_file("A.TXT", 1, 10)]);
    assert!(sync.records.contains_key("a.txt"));
    assert_eq!(sync.records.len(), 1);
}

#[test]
fn refused_put_is_neither_retried_nor_pulled_back_until_the_host_changes() {
    let mut sync = synced("a.txt", b"old");
    let scan = |size, mtime| vec![host_file("a.txt", size, mtime)];
    assert_eq!(sync.on_host_scan(scan(3, 20)), vec![SyncAction::Get { name: "a.txt".into() }]);
    assert!(sync.on_host_file("a.txt".into(), b"new".to_vec(), 20).is_some());
    sync.on_put_rejected("a.txt"); // the guest may have truncated its copy first
    for _ in 0..3 {
        assert!(sync.on_host_scan(scan(3, 20)).is_empty());
        assert!(sync.on_guest_listing(vec![ls_file("a.txt", 1, "g-partial")]).is_empty());
    }
    assert_eq!(sync.on_host_scan(scan(3, 30)), vec![SyncAction::Get { name: "a.txt".into() }]);
    let push = sync.on_host_file("a.txt".into(), b"nu2".to_vec(), 30).unwrap();
    sync.on_put_ok("a.txt".into(), 3, push.hash);
    assert!(sync.put_rejected.is_empty());
    assert!(sync.on_guest_listing(vec![ls_file("a.txt", 3, "g3")]).is_empty());
    assert_eq!(
        sync.on_guest_listing(vec![ls_file("a.txt", 4, "g4")]),
        vec![SyncAction::Get { name: "a.txt".into() }]
    );
}
