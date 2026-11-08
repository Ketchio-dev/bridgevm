//! A new session (a VM start or guest restart) sends nothing for files both
//! sides already hold, and still sends or fetches what changed while the VM
//! was off.

use super::*;

#[test]
fn a_new_session_sends_only_what_changed_while_the_vm_was_off() {
    assert_eq!(dotnet_time(1_794_096_312_123), "2026-11-08T00:05:12.1230000Z");
    let mut rig = Rig::new("new-session");
    let chunked = vec![5u8; SHARE_PUT_CHUNK_BYTES + 3];
    for (name, bytes) in [("a.txt", &b"alpha"[..]), ("same-size.txt", b"12345"), ("big.bin", &chunked)] {
        std::fs::write(rig.dir.join(name), bytes).unwrap();
    }
    rig.run(16);
    assert_eq!(rig.guest.puts.len(), 3, "{:?}", rig.guest.puts);

    // With the VM off a Mac file changes at its size, and the guest gained a file.
    let changed = rig.dir.join("same-size.txt");
    let later = std::fs::metadata(&changed).unwrap().modified().unwrap() + Duration::from_secs(2);
    std::fs::write(&changed, b"54321").unwrap();
    std::fs::File::options().write(true).open(&changed).unwrap().set_modified(later).unwrap();
    rig.guest.write("guest-only.txt", b"g");
    rig.h.share = Some(ShareState::new(ShareSync::new(512), rig.dir.clone(), ROOT.into(), Duration::from_millis(500)));
    let sent = rig.guest.puts.len();
    rig.run(16);
    assert_eq!(rig.guest.puts[sent..], ["same-size.txt"]);
    assert_eq!(rig.guest.content("same-size.txt"), Some(&b"54321"[..]));
    assert_eq!(rig.guest.gets, ["guest-only.txt"]);
    assert_eq!(rig.host("guest-only.txt").as_deref(), Some(&b"g"[..]));
    rig.assert_settled();

    // A file recorded this way syncs as before: a Mac edit is sent.
    std::fs::write(rig.dir.join("a.txt"), b"alpha 2").unwrap();
    rig.run(8);
    assert_eq!(rig.guest.content("a.txt"), Some(&b"alpha 2"[..]));
}

#[test]
fn a_failed_first_listing_still_lets_the_mac_files_go() {
    let mut rig = Rig::new("first-listing-fails");
    rig.guest.ls_error = true;
    std::fs::write(rig.dir.join("a.txt"), b"alpha").unwrap();
    rig.run(8);
    assert_eq!(rig.guest.puts, ["a.txt"]);
}

#[test]
fn a_mac_file_deleted_while_it_waits_for_the_first_listing_releases_its_guest_copy() {
    let mut rig = Rig::new("left-while-waiting");
    rig.guest.write("left.txt", b"guest copy");
    std::fs::write(rig.dir.join("left.txt"), b"mac copy, never sent").unwrap();
    // One tick: the host scan holds the file back, the first listing sees the guest copy held.
    rig.run(1);
    assert!(rig.guest.puts.is_empty() && rig.guest.gets.is_empty());
    std::fs::remove_file(rig.dir.join("left.txt")).unwrap();
    rig.run(8);
    assert_eq!(rig.guest.gets, ["left.txt"], "the guest copy stayed held");
    assert_eq!(rig.host("left.txt").as_deref(), Some(&b"guest copy"[..]));
    // And it syncs from then on: a guest edit reaches the Mac.
    rig.guest.write("left.txt", b"guest edit");
    rig.run(8);
    assert_eq!(rig.host("left.txt").as_deref(), Some(&b"guest edit"[..]));
}

#[test]
fn a_mac_file_deleted_and_restored_with_its_time_is_sent_again_not_adopted() {
    let mut rig = Rig::new("restored");
    let path = rig.dir.join("kept.txt");
    std::fs::write(&path, b"precious").unwrap();
    rig.run(8);
    assert_eq!(rig.guest.content("kept.txt"), Some(&b"precious"[..]));
    let mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
    // Moved out of the share: the guest copy is deleted.
    std::fs::rename(&path, rig.dir.with_extension("aside")).unwrap();
    for _ in 0..8 {
        rig.run(1);
        if rig.guest.content("kept.txt").is_none() {
            break;
        }
    }
    assert_eq!(rig.guest.content("kept.txt"), None);
    // Moved back with its time before the next listing.
    std::fs::rename(rig.dir.with_extension("aside"), &path).unwrap();
    std::fs::File::options().write(true).open(&path).unwrap().set_modified(mtime).unwrap();
    rig.run(12);
    assert_eq!(rig.host("kept.txt").as_deref(), Some(&b"precious"[..]), "the restored Mac file was deleted");
    assert_eq!(rig.guest.content("kept.txt"), Some(&b"precious"[..]));
}

#[test]
fn only_the_first_listing_is_adopted_from_and_never_a_copy_deleted_since() {
    let time = "2026-11-08T00:05:12.1230000Z";
    let listed = || LsEntry { name: "x.txt".into(), size: 3, is_dir: false, mtime: time.into() };
    let session = || {
        let mut engine = ShareSync::new(1024);
        let host = HostFile { name: "x.txt".into(), size: 3, mtime_ms: 1_794_096_312_123 };
        assert_eq!(engine.on_host_scan_normalized([host]).len(), 1);
        // The waiting Mac file holds the listed copy: nothing to fetch.
        assert!(engine.on_guest_listing_normalized([listed()]).is_empty());
        engine
    };
    let mut first = session();
    assert!(first.adopt_listed_copy("x.txt", b"abc", 1_794_096_312_123));
    // Deleted in the guest by the host's DEL, then the Mac file comes back: send it, never adopt it.
    first.on_guest_deleted("x.txt");
    assert!(!first.adopt_listed_copy("x.txt", b"abc", 1_794_096_312_123));
    let mut later = session();
    assert!(later.on_guest_listing_normalized([listed()]).is_empty());
    assert!(!later.adopt_listed_copy("x.txt", b"abc", 1_794_096_312_123), "adopted from a second listing");
}
