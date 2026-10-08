//! Share sync planning: guest listings, host scans, conflicts and skips.

use super::*;

fn ls_file(name: &str, size: u64, mtime: &str) -> LsEntry {
    LsEntry {
        name: name.into(),
        size,
        is_dir: false,
        mtime: mtime.into(),
    }
}

fn host_file(name: &str, size: u64, mtime_ms: u128) -> HostFile {
    HostFile {
        name: name.into(),
        size,
        mtime_ms,
    }
}

#[test]
fn parse_ls_handles_lines_empty_pipe_names_and_normalizes_guest_rels() {
    assert!(parse_ls("").is_empty());
    let entries = parse_ls(
        "a.txt|3|0|2026-01-01T00:00:00.0000000Z\n\
         sub\\dir|0|1|2026-01-01T00:00:01.0000000Z\n\
         sub\\odd|name.txt|4|0|2026-01-01T00:00:02.0000000Z\n",
    );
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].name, "a.txt");
    assert_eq!(entries[0].size, 3);
    assert!(!entries[0].is_dir);
    assert_eq!(entries[1].name, "sub/dir");
    assert!(entries[1].is_dir);
    assert_eq!(entries[2].name, "sub/odd|name.txt");
}

#[test]
fn parse_ls_into_reuses_output_vec_and_clears_old_entries() {
    let mut entries = Vec::with_capacity(4);
    entries.push(ls_file("old.txt", 1, "old"));
    let capacity = entries.capacity();

    parse_ls_into(
        "a.txt|3|0|2026-01-01T00:00:00.0000000Z\n\
         malformed\n\
         sub\\dir|0|1|2026-01-01T00:00:01.0000000Z\n",
        &mut entries,
    );

    assert_eq!(entries.capacity(), capacity);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "a.txt");
    assert_eq!(entries[1].name, "sub/dir");
    assert!(entries[1].is_dir);
}

#[test]
fn rel_path_helpers_round_trip_guest_and_host_forms() {
    assert_eq!(
        from_guest_rel("sub\\dir\\file.txt").as_deref(),
        Some("sub/dir/file.txt")
    );
    assert_eq!(to_guest_rel("sub/dir/file.txt"), "sub\\dir\\file.txt");
    let mut prefixed = String::from("C:\\share\\");
    append_guest_rel_into("./sub//./dir\\file.txt", &mut prefixed);
    assert_eq!(prefixed, "C:\\share\\sub\\dir\\file.txt");
    assert_eq!(
        from_guest_rel(&to_guest_rel("sub/dir/file.txt")).as_deref(),
        Some("sub/dir/file.txt")
    );
    assert_eq!(normalize_rel("./sub//./dir\\file.txt"), "sub/dir/file.txt");
    assert_eq!(to_guest_rel("./sub//./dir\\file.txt"), "sub\\dir\\file.txt");
}

#[test]
fn guest_listing_entries_cannot_name_paths_outside_the_share() {
    let entries = parse_ls(
        "..\\escaped.txt|3|0|2026-01-01T00:00:00.0000000Z\n\
         sub\\..\\..\\x|3|0|2026-01-01T00:00:00.0000000Z\n\
         ../up|0|1|2026-01-01T00:00:00.0000000Z\n\
         \\\\abs\\..ok.txt|3|0|2026-01-01T00:00:00.0000000Z\n",
    );
    let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(names, ["abs/..ok.txt"]);

    let mut sync = ShareSync::new(1024);
    let actions = sync.on_guest_listing_normalized(parse_ls(
        "..\\escaped.txt|3|0|2026-01-01T00:00:00.0000000Z\n",
    ));
    assert!(actions.is_empty());
}

#[test]
fn fnv1a_known_vectors() {
    assert_eq!(fnv1a64(b""), 0xcbf29ce484222325);
    assert_eq!(fnv1a64(b"a"), 0xaf63dc4c8601ec8c);
}

#[test]
fn host_guest_round_trip_does_not_ping_pong() {
    let mut sync = ShareSync::new(512);
    assert_eq!(
        sync.on_host_scan(vec![host_file("sub/x.txt", 5, 10)]),
        vec![SyncAction::Get {
            name: "sub/x.txt".into()
        }]
    );
    let push = sync
        .on_host_file("sub/x.txt".into(), b"hello".to_vec(), 10)
        .expect("host edit pushes guest");
    sync.on_put_ok("sub/x.txt".into(), push.bytes.len() as u64, push.hash);

    let actions = sync.on_guest_listing(vec![ls_file("sub\\x.txt", 5, "guest-1")]);
    assert!(actions.is_empty(), "PUT landing mtime is only stamped");
    assert!(sync
        .on_guest_listing(vec![ls_file("sub\\x.txt", 5, "guest-1")])
        .is_empty());

    assert_eq!(
        sync.on_guest_listing(vec![ls_file("sub\\x.txt", 6, "guest-2")]),
        vec![SyncAction::Get {
            name: "sub/x.txt".into()
        }]
    );
    match sync.on_guest_file("sub\\x.txt".into(), b"world!".to_vec(), None) {
        GuestFileOutcome::WriteHost(bytes) => assert_eq!(bytes, b"world!"),
        GuestFileOutcome::AlreadySynced => panic!("guest edit must write host"),
    }
    sync.note_host_stat("sub/x.txt", 20);
    assert!(sync
        .on_host_file("sub/x.txt".into(), b"world!".to_vec(), 20)
        .is_none());
}

#[test]
fn guest_oversize_skips_are_deduped_by_name_mtime_kind_and_dirs_are_ignored() {
    let mut sync = ShareSync::new(1);
    let entries = vec![
        ls_file("big.bin", 2048, "m1"),
        LsEntry {
            name: "sub".into(),
            size: 0,
            is_dir: true,
            mtime: "d1".into(),
        },
    ];
    assert_eq!(
        sync.on_guest_listing(entries.clone()),
        vec![SyncAction::Skip {
            name: "big.bin".into(),
            reason: SkipReason::TooLarge { size: 2048 },
        }]
    );
    assert!(sync.on_guest_listing(entries).is_empty());
    assert_eq!(
        sync.on_guest_listing(vec![ls_file("big.bin", 2048, "m2")]),
        vec![SyncAction::Skip {
            name: "big.bin".into(),
            reason: SkipReason::TooLarge { size: 2048 },
        }]
    );
}

#[test]
fn recursive_host_scan_detects_nested_changes() {
    let mut sync = ShareSync::new(512);
    assert_eq!(
        sync.on_host_scan(vec![host_file("sub/dir/a.txt", 1, 10)]),
        vec![SyncAction::Get {
            name: "sub/dir/a.txt".into()
        }]
    );
    let push = sync
        .on_host_file("sub/dir/a.txt".into(), b"a".to_vec(), 10)
        .unwrap();
    sync.on_put_ok("sub/dir/a.txt".into(), 1, push.hash);
    sync.on_guest_listing(vec![ls_file("sub\\dir\\a.txt", 1, "g1")]);
    sync.note_host_stat("sub/dir/a.txt", 10);
    assert!(sync
        .on_host_scan(vec![host_file("sub/dir/a.txt", 1, 10)])
        .is_empty());
    assert_eq!(
        sync.on_host_scan(vec![host_file("sub/dir/a.txt", 2, 11)]),
        vec![SyncAction::Get {
            name: "sub/dir/a.txt".into()
        }]
    );
}

#[test]
fn tombstone_lifecycle_host_delete_then_confirm_removes_record() {
    let mut sync = ShareSync::new(512);
    let push = sync
        .on_host_file("gone.txt".into(), b"gone".to_vec(), 10)
        .unwrap();
    sync.on_put_ok("gone.txt".into(), 4, push.hash);
    sync.on_guest_listing(vec![ls_file("gone.txt", 4, "g1")]);
    sync.note_host_stat("gone.txt", 10);

    assert_eq!(
        sync.on_host_scan(Vec::new()),
        vec![SyncAction::DeleteGuest {
            name: "gone.txt".into()
        }]
    );
    sync.on_guest_deleted("gone.txt");
    assert_eq!(
        sync.on_guest_listing(vec![ls_file("gone.txt", 4, "g1")]),
        vec![SyncAction::Get {
            name: "gone.txt".into()
        }]
    );
}

#[test]
fn tombstone_lifecycle_guest_delete_then_confirm_removes_record() {
    let mut sync = ShareSync::new(512);
    let push = sync
        .on_host_file("gone.txt".into(), b"gone".to_vec(), 10)
        .unwrap();
    sync.on_put_ok("gone.txt".into(), 4, push.hash);
    sync.on_guest_listing(vec![ls_file("gone.txt", 4, "g1")]);
    sync.note_host_stat("gone.txt", 10);

    assert_eq!(
        sync.on_guest_listing(Vec::new()),
        vec![SyncAction::DeleteHost {
            name: "gone.txt".into()
        }]
    );
    sync.on_host_deleted("gone.txt");
    assert_eq!(
        sync.on_host_scan(vec![host_file("gone.txt", 4, 10)]),
        vec![SyncAction::Get {
            name: "gone.txt".into()
        }]
    );
}

#[test]
fn never_recorded_path_absence_never_deletes() {
    let mut sync = ShareSync::new(512);
    assert!(sync.on_host_scan(Vec::new()).is_empty());
    assert!(sync.on_guest_listing(Vec::new()).is_empty());
}

#[test]
fn listing_scratch_tables_reuse_capacity() {
    let mut sync = ShareSync::new(512);

    let _ = sync.on_guest_listing(vec![
        ls_file("a.txt", 1, "g1"),
        LsEntry {
            name: "dir".into(),
            size: 0,
            is_dir: true,
            mtime: "d1".into(),
        },
    ]);
    let guest_present_capacity = sync.present_scratch.capacity();
    let guest_entries_capacity = sync.guest_file_entries_scratch.capacity();
    assert!(guest_present_capacity > 0);
    assert!(guest_entries_capacity > 0);

    let _ = sync.on_guest_listing(Vec::new());
    assert_eq!(sync.present_scratch.capacity(), guest_present_capacity);
    assert_eq!(
        sync.guest_file_entries_scratch.capacity(),
        guest_entries_capacity
    );

    let _ = sync.on_host_scan_normalized(vec![host_file("b.txt", 1, 10)]);
    let host_files_capacity = sync.host_files_scratch.capacity();
    assert!(host_files_capacity > 0);

    let _ = sync.on_host_scan_normalized(Vec::new());
    assert_eq!(sync.host_files_scratch.capacity(), host_files_capacity);
}

#[test]
fn modification_wins_when_host_deleted_but_guest_changed() {
    let mut sync = ShareSync::new(512);
    let push = sync
        .on_host_file("race.txt".into(), b"old".to_vec(), 10)
        .unwrap();
    sync.on_put_ok("race.txt".into(), 3, push.hash);
    sync.on_guest_listing(vec![ls_file("race.txt", 3, "g1")]);
    sync.note_host_stat("race.txt", 10);

    assert_eq!(
        sync.on_guest_listing(vec![ls_file("race.txt", 4, "g2")]),
        vec![SyncAction::Get {
            name: "race.txt".into()
        }]
    );
}

#[test]
fn modification_wins_when_guest_deleted_but_host_changed() {
    let mut sync = ShareSync::new(512);
    let push = sync
        .on_host_file("race.txt".into(), b"old".to_vec(), 10)
        .unwrap();
    sync.on_put_ok("race.txt".into(), 3, push.hash);
    sync.on_guest_listing(vec![ls_file("race.txt", 3, "g1")]);
    sync.note_host_stat("race.txt", 10);

    assert_eq!(
        sync.on_host_scan(vec![host_file("race.txt", 4, 11)]),
        vec![SyncAction::Get {
            name: "race.txt".into()
        }]
    );
    assert!(sync.on_guest_listing(Vec::new()).is_empty());
}

#[test]
fn guest_skip_memory_holds_only_the_current_listing() {
    let mut sync = ShareSync::new(1);
    let listing = |mtime: &str| -> Vec<LsEntry> {
        (0..64)
            .map(|i| ls_file(&format!("big{i}.bin"), 4096, mtime))
            .collect()
    };
    for round in 0..8 {
        let actions = sync.on_guest_listing(listing(&format!("m{round}")));
        assert_eq!(actions.len(), 64, "each new oversized version is reported once");
    }
    assert_eq!(sync.guest_skip_seen.len(), 64);
    assert!(sync.on_guest_listing(listing("m7")).is_empty());

    assert!(sync.on_guest_listing(Vec::new()).is_empty());
    assert!(sync.guest_skip_seen.is_empty());
}

