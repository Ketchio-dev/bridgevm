//! Host names that Windows stores as one name (case twins on a case-sensitive
//! volume, `foo` next to `foo.` or `foo `) and names below a folder Win32 trims:
//! none is uploaded, no guest state for that name reaches the host, one
//! diagnostic per name, sync resumes once the host names are told apart, and a
//! guest edit made meanwhile is never deleted.

use super::*;

fn skips(actions: &[SyncAction]) -> Vec<&str> {
    let mut names = Vec::new();
    for action in actions {
        if let SyncAction::Skip { name, .. } = action {
            names.push(name.as_str());
        }
    }
    names
}

fn gets(actions: &[SyncAction]) -> Vec<&str> {
    let mut names = Vec::new();
    for action in actions {
        if let SyncAction::Get { name } = action {
            names.push(name.as_str());
        }
    }
    names.sort_unstable();
    names
}

#[test]
fn twins_are_never_uploaded_and_guest_edits_or_deletes_never_reach_the_host() {
    for (kept, twin) in [
        ("a.txt", "A.txt"),
        ("foo", "foo."),
        ("bar", "bar "),
        ("d/x", "D/X"),
    ] {
        let mut sync = synced(kept, b"old");
        let scan = || vec![host_file(kept, 3, 10), host_file(twin, 4, 20)];
        let first = sync.on_host_scan(scan());
        assert_eq!(
            (skips(&first).len(), first.len()),
            (1, 1),
            "{twin}: {first:?}"
        );
        for _ in 0..3 {
            assert_eq!(sync.on_host_scan(scan()), Vec::new(), "{twin}: repeated");
            let upper = kept.to_uppercase();
            for listing in [
                vec![ls_file(kept, 9, "g2")],
                vec![ls_file(&upper, 9, "g3")],
                vec![],
            ] {
                assert_eq!(sync.on_guest_listing(listing), Vec::new(), "{twin}");
            }
        }
        // A host edit of either twin is not uploaded either.
        let edited = vec![host_file(kept, 8, 40), host_file(twin, 6, 50)];
        assert_eq!(sync.on_host_scan(edited), Vec::new(), "{twin}");
    }
}

#[test]
fn twins_without_records_are_held_and_other_names_still_sync() {
    // As after a restart: no records, and the guest holds older copies.
    let mut sync = ShareSync::new(512);
    let scan = || {
        let names = ["Report.txt", "REPORT.TXT", "foo", "foo.", "other.txt"];
        names.map(|name| host_file(name, 1, 10)).to_vec()
    };
    let first = sync.on_host_scan(scan());
    assert_eq!(skips(&first).len(), 2, "{first:?}");
    assert_eq!(gets(&first), ["other.txt"]);
    let listing = || vec![ls_file("report.txt", 7, "g1"), ls_file("FOO", 7, "g1")];
    for _ in 0..3 {
        assert_eq!(gets(&sync.on_host_scan(scan())), ["other.txt"]);
        assert_eq!(sync.on_guest_listing(listing()), Vec::new());
        assert_eq!(sync.on_guest_listing(Vec::new()), Vec::new());
    }
}

#[test]
fn name_below_a_folder_windows_trims_is_never_touched_by_the_guest_name() {
    for (host, guest) in [("dir./x", "dir/x"), ("a/b /x", "a/b/x")] {
        let mut sync = ShareSync::new(512);
        let first = sync.on_host_scan(vec![host_file(host, 2, 20)]);
        assert_eq!(first.len(), 1, "{host:?}: {first:?}");
        assert_eq!(skips(&first), [host]);
        for _ in 0..3 {
            assert_eq!(sync.on_host_scan(vec![host_file(host, 2, 20)]), Vec::new());
            // An independent guest file of the trimmed name, edited then deleted.
            assert_eq!(
                sync.on_guest_listing(vec![ls_file(guest, 5, "g1")]),
                Vec::new()
            );
            assert_eq!(sync.on_guest_listing(Vec::new()), Vec::new(), "{host:?}");
        }
    }
    // A lone last-component name is not held: it maps to the guest's `foo`.
    for lone in ["foo.", "foo ", "dir/foo."] {
        let actions = ShareSync::new(512).on_host_scan(vec![host_file(lone, 2, 20)]);
        assert_eq!(actions, vec![SyncAction::Get { name: lone.into() }]);
    }
}

#[test]
fn sync_resumes_without_loss_once_the_host_tells_the_twins_apart() {
    // Twin removed: the guest edit made meanwhile reaches the unchanged host file.
    let mut sync = synced("foo", b"old");
    let first = sync.on_host_scan(vec![host_file("foo", 3, 10), host_file("foo.", 4, 20)]);
    assert_eq!(skips(&first).len(), 1);
    assert_eq!(
        sync.on_guest_listing(vec![ls_file("foo", 5, "g2")]),
        Vec::new()
    );
    assert_eq!(sync.on_host_scan(vec![host_file("foo", 3, 10)]), Vec::new());
    let listing = sync.on_guest_listing(vec![ls_file("foo", 5, "g2")]);
    assert_eq!(listing, vec![SyncAction::Get { name: "foo".into() }]);
    // A new collision is reported again.
    let again = sync.on_host_scan(vec![host_file("foo", 3, 10), host_file("FOO ", 1, 30)]);
    assert_eq!(skips(&again).len(), 1);

    // Synced twin renamed away on a case-sensitive host: the remaining twin
    // takes over the guest file (host wins), the renamed one is new, and
    // nothing is deleted on either side.
    let mut sync = synced("a.txt", b"old");
    sync.on_host_scan(vec![host_file("a.txt", 3, 10), host_file("A.txt", 4, 20)]);
    let actions = sync.on_host_scan(vec![host_file("A.txt", 4, 20), host_file("b.txt", 3, 10)]);
    assert_eq!(actions.len(), 2, "{actions:?}");
    assert_eq!(gets(&actions), ["A.txt", "b.txt"]);
}

#[test]
fn guest_edit_made_during_a_hold_survives_when_the_held_names_leave() {
    // The hold ends with the record's host name gone: every twin deleted, the
    // record's twin deleted and the other renamed away, or a held folder
    // renamed. As for any host deletion of a guest-edited file, the guest
    // copy is kept and fetched rather than deleted.
    let twins = |name: &str, twin: &str| vec![host_file(name, 3, 10), host_file(twin, 4, 20)];
    let cases = [
        ("foo", twins("foo", "foo."), vec![]),
        ("a.txt", twins("a.txt", "A.txt"), vec![]),
        ("foo", twins("foo", "foo."), vec![host_file("bar", 4, 20)]),
        (
            "d/x",
            vec![host_file("d./x", 3, 10)],
            vec![host_file("d2/x", 3, 10)],
        ),
    ];
    for (name, held, gone) in cases {
        let mut sync = synced(name, b"old");
        assert_eq!(skips(&sync.on_host_scan(held)).len(), 1, "{name}");
        let edited = || vec![ls_file(name, 5, "g2")];
        assert_eq!(sync.on_guest_listing(edited()), Vec::new(), "{name}");
        let actions = sync.on_host_scan(gone);
        let delete = SyncAction::DeleteGuest { name: name.into() };
        assert!(!actions.contains(&delete), "{name}: {actions:?}");
        let get = vec![SyncAction::Get { name: name.into() }];
        assert_eq!(sync.on_guest_listing(edited()), get, "{name}");
    }
}

#[test]
fn refused_put_copy_is_never_kept_as_a_guest_edit_across_a_hold() {
    // The guest may have truncated its copy before refusing the PUT: it is
    // neither fetched nor spared as an edit when the hold ends.
    for exit in [vec![host_file("doc", 5, 20)], vec![]] {
        let mut sync = synced("doc", b"old");
        assert_eq!(
            gets(&sync.on_host_scan(vec![host_file("doc", 5, 20)])),
            ["doc"]
        );
        assert!(sync
            .on_host_file("doc".into(), b"newer".to_vec(), 20)
            .is_some());
        sync.on_put_rejected("doc");
        let twins = vec![host_file("doc", 5, 20), host_file("doc.", 1, 30)];
        assert_eq!(skips(&sync.on_host_scan(twins)).len(), 1);
        let partial = || vec![ls_file("doc", 2, "g-partial")];
        assert_eq!(sync.on_guest_listing(partial()), Vec::new());
        let deleted = exit.is_empty();
        let actions = sync.on_host_scan(exit);
        let delete = SyncAction::DeleteGuest { name: "doc".into() };
        assert_eq!(actions.contains(&delete), deleted, "{actions:?}");
        if deleted {
            sync.on_guest_deleted("doc"); // OK DEL
            assert_eq!(sync.on_guest_listing(Vec::new()), Vec::new());
        } else {
            // Unchanged host file: still refused, not retried, not fetched.
            assert_eq!(actions, Vec::new());
            assert_eq!(sync.on_guest_listing(partial()), Vec::new());
        }
    }
}
