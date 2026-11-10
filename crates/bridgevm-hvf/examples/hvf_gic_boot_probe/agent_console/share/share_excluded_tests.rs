//! OS metadata (share_excluded.rs) never moves either way: a copy already in
//! the guest is neither overwritten, fetched nor deleted, whatever the Mac does
//! and in a new session too, and a volume folder at the share root is not even
//! walked. Names that only look like metadata sync as any other.

use super::adopt_tests::{new_session, vars};
use super::*;

fn guest_names(rig: &Rig) -> Vec<String> {
    let mut names: Vec<String> = rig.guest.files.iter().map(|(name, _, _)| name.clone()).collect();
    names.sort();
    names
}

#[test]
fn os_metadata_in_the_guest_is_never_overwritten_fetched_or_deleted() {
    let mut rig = Rig::new("excluded");
    let vars = vars(&rig);
    new_session(&mut rig, Some(&vars));
    // Synced before names were excluded: Finder's files, and Explorer's own.
    for (name, bytes) in [
        (".DS_Store", &b"old"[..]),
        ("sub\\.DS_Store", b"sub old"),
        ("desktop.ini", b"[.ShellClassInfo]"),
        ("Thumbs.db", b"thumbs"),
        // Below a folder the Mac walk never enters.
        ("._folder\\notes.txt", b"below"),
    ] {
        rig.guest.write(name, bytes);
    }
    std::fs::write(rig.dir.join(".DS_Store"), b"newer").unwrap();
    std::fs::write(rig.dir.join("._a.txt"), b"appledouble").unwrap();
    std::fs::write(rig.dir.join("a.txt"), b"alpha").unwrap();
    // Windows would store it as the guest's desktop.ini.
    std::fs::write(rig.dir.join("desktop.ini."), b"mac").unwrap();
    rig.run(16);
    assert_eq!(rig.guest.puts, ["a.txt"]);
    assert_eq!(rig.guest.gets, [] as [String; 0]);
    assert_eq!(rig.guest.content(".DS_Store"), Some(&b"old"[..]), "the Mac's copy went over the guest's");
    assert_eq!(rig.guest.content("sub\\.DS_Store"), Some(&b"sub old"[..]));
    assert_eq!(rig.guest.content("desktop.ini"), Some(&b"[.ShellClassInfo]"[..]));
    assert_eq!(rig.guest.content("._folder\\notes.txt"), Some(&b"below"[..]));
    assert_eq!(rig.host(".DS_Store").as_deref(), Some(&b"newer"[..]));
    assert_eq!(rig.host_names(), [".DS_Store", "._a.txt", "a.txt", "desktop.ini."]);

    // Finder deletes its copy: no hold ends, so the guest's is not fetched back, nor deleted.
    std::fs::remove_file(rig.dir.join(".DS_Store")).unwrap();
    rig.run(16);
    assert_eq!(rig.guest.gets, [] as [String; 0]);
    assert_eq!(rig.guest.content(".DS_Store"), Some(&b"old"[..]));
    assert_eq!(rig.host_names(), ["._a.txt", "a.txt", "desktop.ini."]);

    // A new session with the saved state: no record holds the guest-only copies, yet none is
    // fetched, and a.txt is adopted rather than sent again.
    std::fs::write(rig.dir.join(".DS_Store"), b"while off").unwrap();
    new_session(&mut rig, Some(&vars));
    rig.run(16);
    assert_eq!(rig.guest.puts, ["a.txt"]);
    assert_eq!(rig.guest.gets, [] as [String; 0]);
    assert_eq!(rig.guest.content(".DS_Store"), Some(&b"old"[..]));
    assert_eq!(rig.guest.content("sub\\.DS_Store"), Some(&b"sub old"[..]));
    assert_eq!(rig.guest.content("._folder\\notes.txt"), Some(&b"below"[..]));
    assert_eq!(rig.host_names(), [".DS_Store", "._a.txt", "a.txt", "desktop.ini."]);

    // Deleted in Windows and rewritten by Finder: nothing moves either way.
    rig.guest.files.retain(|(name, _, _)| name == "a.txt");
    std::fs::write(rig.dir.join(".DS_Store"), b"rewritten").unwrap();
    rig.run(16);
    assert_eq!(rig.guest.puts, ["a.txt"]);
    assert_eq!(guest_names(&rig), ["a.txt"]);
    assert_eq!(rig.host(".DS_Store").as_deref(), Some(&b"rewritten"[..]));
    assert_eq!(rig.host("._a.txt").as_deref(), Some(&b"appledouble"[..]));
    assert_eq!(rig.host("desktop.ini.").as_deref(), Some(&b"mac"[..]));
    rig.assert_settled();
}

#[test]
fn a_volume_folder_at_the_share_root_is_not_walked_and_look_alikes_sync() {
    let mut rig = Rig::new("excluded-root");
    let mut child = std::process::Command::new("/usr/bin/true").spawn().unwrap();
    let dead = child.id();
    child.wait().unwrap();
    // The walk removes a dead process's temporary wherever it walks (share_host_scan_walk.rs).
    let stale = format!(".fseventsd/.bridgevm-sync-{dead}-3");
    let excluded = [".fseventsd/0000000000a1b2c3", ".Spotlight-V100/Store-V2/store.db", ".Trashes/501/old.txt", stale.as_str()];
    let shared = [".DS_Store.txt", ".fseventsd2/x.txt", "a.DS_Store", "a.txt", "sub/.fseventsd/notes.txt"];
    for name in excluded.iter().chain(&shared) {
        let path = rig.dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, name.as_bytes()).unwrap();
    }
    rig.guest.write(".fseventsd\\0000old", b"leftover");
    let share = rig.h.share.as_mut().unwrap();
    assert!(scan_share_host_dir(share, rig.now).is_some());
    let mut scanned: Vec<_> = share.host_scan_scratch.iter().map(|file| file.name.clone()).collect();
    scanned.sort();
    assert_eq!(scanned, shared);
    assert!(rig.dir.join(&stale).exists(), "the walk went into .fseventsd");

    rig.run(24);
    let mut puts = rig.guest.puts.clone();
    puts.sort();
    assert_eq!(puts, shared.map(|name| name.replace('/', "\\")));
    for name in shared {
        assert_eq!(rig.guest.content(&name.replace('/', "\\")), Some(name.as_bytes()), "{name}");
    }
    assert_eq!(rig.guest.gets, [] as [String; 0]);
    assert_eq!(rig.guest.content(".fseventsd\\0000old"), Some(&b"leftover"[..]));
    assert!(!rig.dir.join(".fseventsd/0000old").exists());
    rig.assert_settled();
}
