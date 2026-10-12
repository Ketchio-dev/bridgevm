//! Host names Win32 stores as one guest name (`foo` next to `foo.` or `foo `)
//! against a real host directory: neither host file is ever uploaded over,
//! overwritten or deleted because of the guest's single file, and a guest edit
//! made while they are held survives the hold.

use super::*;

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
