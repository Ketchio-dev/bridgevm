//! Host names Win32 stores as one guest name (`foo` next to `foo.` or `foo `)
//! against a real host directory: neither host file is ever uploaded over,
//! overwritten or deleted because of the guest's single file. A guest edit made
//! while a name is held (an alias, or unseen) survives the hold; a guest copy a
//! PUT may have truncated never counts as such an edit.

use super::*;

/// One service step in which the guest agent restarts part-way through a PUT
/// (after the first chunk of a chunked one), having truncated the file first:
/// `bvagent.ps1` writes with `FileMode.Create`.
fn restart_mid_put(rig: &mut Rig) {
    rig.now += Duration::from_secs(1);
    rig.h.service_enqueue(rig.now);
    let queue = &mut rig.h.queue;
    let at = queue
        .iter()
        .position(|req| matches!(req, ServiceReq::SharePut { .. }));
    let put = queue.remove(at.expect("a PUT is queued")).unwrap();
    if let ServiceReq::SharePut { name, .. } = &put {
        rig.guest.write(name, b"");
    }
    rig.h.in_flight = Some((put, rig.now));
    for line in ["OK PUTBEG", "READY GUEST"] {
        rig.h.handle_service_reply(line, None, None, rig.now);
    }
}

#[test]
fn trailing_dot_or_space_twin_never_overwrites_or_deletes_either_host_file() {
    for (label, twin) in [("alias-dot", "foo."), ("alias-space", "foo ")] {
        let mut rig = Rig::synced(label, "foo", b"FOO ORIGINAL");
        let puts = rig.guest.puts.len();
        std::fs::write(rig.dir.join(twin), b"TWIN").unwrap();
        let hosts_intact = |rig: &Rig| {
            assert_eq!(
                rig.host("foo").as_deref(),
                Some(&b"FOO ORIGINAL"[..]),
                "{label}"
            );
            assert_eq!(rig.host(twin).as_deref(), Some(&b"TWIN"[..]), "{label}");
        };
        rig.run(24);
        hosts_intact(&rig);
        assert_eq!(
            rig.guest.content("foo"),
            Some(&b"FOO ORIGINAL"[..]),
            "{label}"
        );
        assert_eq!(rig.guest.puts.len(), puts, "{label}: {:?}", rig.guest.puts);
        // The guest edits, then deletes, the one file it has for both names.
        rig.guest.write("foo", b"guest edit");
        rig.run(24);
        hosts_intact(&rig);
        rig.guest.files.clear();
        rig.run(24);
        hosts_intact(&rig);
        assert!(rig.guest.gets.is_empty(), "{label}: {:?}", rig.guest.gets);
        assert_eq!(rig.host_names(), ["foo", twin]);
        rig.assert_settled();
    }
}

#[test]
fn renaming_the_twin_apart_resumes_sync_without_losing_either_side() {
    let mut rig = Rig::synced("alias-resolve", "foo", b"FOO ORIGINAL");
    std::fs::write(rig.dir.join("foo."), b"TWIN").unwrap();
    rig.run(24);
    rig.guest.write("foo", b"guest edit");
    rig.run(24);
    assert_eq!(rig.host("foo").as_deref(), Some(&b"FOO ORIGINAL"[..]));
    std::fs::rename(rig.dir.join("foo."), rig.dir.join("foo2")).unwrap();
    rig.run(24);
    // The held guest edit reaches the unchanged host file; the twin syncs as itself.
    assert_eq!(rig.host("foo").as_deref(), Some(&b"guest edit"[..]));
    assert_eq!(rig.host("foo2").as_deref(), Some(&b"TWIN"[..]));
    assert_eq!(rig.guest.content("foo"), Some(&b"guest edit"[..]));
    assert_eq!(rig.guest.content("foo2"), Some(&b"TWIN"[..]));
    assert_eq!(rig.host_names(), ["foo", "foo2"]);
    rig.assert_settled();
}

#[test]
fn guest_edit_made_while_twins_are_held_survives_when_both_leave() {
    let mut rig = Rig::synced("alias-exit", "foo", b"FOO ORIGINAL");
    std::fs::write(rig.dir.join("foo."), b"TWIN").unwrap();
    rig.run(24);
    rig.guest.write("foo", b"guest edit");
    rig.run(24);
    std::fs::remove_file(rig.dir.join("foo")).unwrap();
    std::fs::remove_file(rig.dir.join("foo.")).unwrap();
    rig.run(24);
    // Like any host deletion of a file the guest changed: the edit wins.
    assert_eq!(rig.guest.content("foo"), Some(&b"guest edit"[..]));
    assert_eq!(rig.host("foo").as_deref(), Some(&b"guest edit"[..]));
    assert_eq!(rig.host_names(), ["foo"]);
    rig.assert_settled();
}

#[test]
fn guest_edit_survives_a_trailing_space_rename_and_a_rename_away() {
    let mut rig = Rig::synced("alias-notes", "notes", b"NOTES");
    std::fs::rename(rig.dir.join("notes"), rig.dir.join("notes ")).unwrap();
    rig.run(24);
    rig.guest.write("notes", b"guest edit");
    rig.run(24);
    std::fs::rename(rig.dir.join("notes "), rig.dir.join("notes.txt")).unwrap();
    rig.run(24);
    assert_eq!(rig.host("notes.txt").as_deref(), Some(&b"guest edit"[..]));
    assert_eq!(rig.guest.content("notes.txt"), Some(&b"guest edit"[..]));
    assert_eq!(rig.guest.content("notes"), None);
    assert_eq!(rig.host_names(), ["notes.txt"]);
    rig.assert_settled();
}

#[test]
fn put_cut_short_by_an_agent_restart_never_returns_as_a_guest_edit() {
    // The name is then held (a twin appears, or it grows past the cap) and the
    // host deletes it: the possibly truncated guest copy must not come back.
    let (small, twin) = (b"FOO NEWER".to_vec(), b"TWIN".to_vec());
    let cases = [
        ("restart-twin", small.clone(), "foo.", twin.clone()),
        ("restart-chunked", vec![b'n'; 30_000], "foo.", twin),
        ("restart-big", small, "foo", vec![b'b'; 70_000]),
    ];
    for (label, newer, held, bytes) in cases {
        let mut rig = Rig::new(label);
        rig.h.share.as_mut().unwrap().engine = ShareSync::new(64);
        std::fs::write(rig.dir.join("foo"), b"FOO ORIGINAL").unwrap();
        rig.run(12);
        std::fs::write(rig.dir.join("foo"), &newer).unwrap();
        restart_mid_put(&mut rig);
        std::fs::write(rig.dir.join(held), bytes).unwrap();
        rig.run(24);
        let resent = rig.guest.content("foo").map(<[u8]>::to_vec);
        let _ = std::fs::remove_file(rig.dir.join("foo."));
        std::fs::remove_file(rig.dir.join("foo")).unwrap();
        rig.run(24);
        let back = rig.host("foo");
        assert!(
            back.is_none(),
            "{label}: deleted host file came back as {back:?}"
        );
        assert!(rig.guest.gets.is_empty(), "{label}: {:?}", rig.guest.gets);
        assert_eq!(rig.guest.content("foo"), None, "{label}");
        assert_eq!(resent, Some(newer), "{label}: PUT resent");
        rig.assert_settled();
    }
}

#[test]
fn unseen_file_edited_on_the_guest_then_deleted_on_the_host_is_fetched() {
    // As for a host deletion of any guest-edited file: the edit wins.
    let mut rig = Rig::new("unseen-then-gone");
    rig.h.share.as_mut().unwrap().engine = ShareSync::new(1);
    std::fs::write(rig.dir.join("log.txt"), b"small").unwrap();
    rig.run(12);
    std::fs::write(rig.dir.join("log.txt"), [b'b'; 2048]).unwrap();
    rig.run(24);
    rig.guest.write("log.txt", b"guest edit");
    rig.run(24);
    assert_eq!(rig.host("log.txt"), Some(vec![b'b'; 2048]));
    std::fs::remove_file(rig.dir.join("log.txt")).unwrap();
    rig.run(24);
    assert_eq!(rig.guest.content("log.txt"), Some(&b"guest edit"[..]));
    assert_eq!(rig.host("log.txt").as_deref(), Some(&b"guest edit"[..]));
    rig.assert_settled();
}
