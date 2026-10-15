//! Host files the scan cannot read still exist: an over-cap, symlinked or
//! unlistable synced file keeps its guest copy and is never replaced from the
//! guest, while a genuine host deletion still reaches the guest.

use super::*;
use std::os::unix::fs::PermissionsExt;

/// Restores a directory's mode before the rig removes the share.
struct Unreadable(PathBuf);

impl Unreadable {
    fn new(path: PathBuf) -> Self {
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        Self(path)
    }
}

impl Drop for Unreadable {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
    }
}

fn rig_with(label: &str, max_kb: u64, files: &[(&str, &[u8])]) -> Rig {
    let mut rig = Rig::new(label);
    rig.h.share.as_mut().unwrap().engine = ShareSync::new(max_kb);
    for (name, bytes) in files {
        let path = rig.dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    rig.run(12);
    for (name, bytes) in files {
        assert_eq!(
            rig.guest.content(&name.replace('/', "\\")),
            Some(*bytes),
            "{name}"
        );
    }
    rig
}

#[test]
fn synced_file_growing_past_the_cap_keeps_its_guest_copy() {
    let mut rig = rig_with(
        "unseen-big",
        1,
        &[("log.txt", b"small"), ("gone.txt", b"G")],
    );
    std::fs::write(rig.dir.join("log.txt"), [b'b'; 2048]).unwrap();
    std::fs::remove_file(rig.dir.join("gone.txt")).unwrap();
    rig.run(24);
    assert_eq!(
        rig.guest.content("log.txt"),
        Some(&b"small"[..]),
        "guest copy deleted"
    );
    assert_eq!(
        rig.guest.content("gone.txt"),
        None,
        "a real host deletion still applies"
    );
    // A guest edit or deletion of the stale copy never touches the newer host file.
    rig.guest.write("log.txt", b"guest edit");
    rig.run(24);
    assert!(rig.guest.gets.is_empty(), "{:?}", rig.guest.gets);
    rig.assert_settled();
    rig.guest.files.retain(|(name, _, _)| name != "log.txt");
    rig.run(24);
    assert!(
        rig.host("log.txt") == Some(vec![b'b'; 2048]),
        "host file changed"
    );
    // Back under the cap the host version syncs again, and a deletion applies.
    std::fs::write(rig.dir.join("log.txt"), b"small again").unwrap();
    rig.run(24);
    assert_eq!(rig.guest.content("log.txt"), Some(&b"small again"[..]));
    std::fs::remove_file(rig.dir.join("log.txt")).unwrap();
    rig.run(24);
    assert_eq!(rig.guest.content("log.txt"), None);
    assert!(rig.guest.gets.is_empty(), "{:?}", rig.guest.gets);
}

#[test]
fn over_cap_host_file_is_never_replaced_by_a_guest_file_of_that_name() {
    // No records, as after a restart; the guest holds an older, smaller copy.
    let mut rig = rig_with("unseen-big-fresh", 1, &[]);
    std::fs::write(rig.dir.join("data.bin"), [b'b'; 2048]).unwrap();
    rig.guest.write("data.bin", b"older guest copy");
    rig.guest.write("new.txt", b"from guest");
    rig.run(24);
    let host = rig.host("data.bin").unwrap();
    assert_eq!(
        host.len(),
        2048,
        "host file replaced by {:?}",
        String::from_utf8_lossy(&host)
    );
    assert_eq!(rig.host("new.txt").as_deref(), Some(&b"from guest"[..]));
    assert_eq!(rig.guest.gets, ["new.txt"]);
    rig.assert_settled();
}

#[test]
fn unreadable_subdirectory_keeps_guest_copies_until_it_is_readable_again() {
    let mut rig = rig_with(
        "unseen-dir",
        512,
        &[("sub/x.txt", b"x"), ("gone.txt", b"G")],
    );
    {
        let _sub = Unreadable::new(rig.dir.join("sub"));
        std::fs::remove_file(rig.dir.join("gone.txt")).unwrap();
        rig.run(24);
        assert_eq!(
            rig.guest.content("sub\\x.txt"),
            Some(&b"x"[..]),
            "guest copy deleted"
        );
        assert_eq!(
            rig.guest.content("gone.txt"),
            None,
            "a real host deletion still applies"
        );
        rig.guest.write("sub\\x.txt", b"guest edit");
        rig.guest.write("sub\\new.txt", b"guest new");
        rig.run(24);
        assert!(rig.guest.gets.is_empty(), "{:?}", rig.guest.gets);
        rig.assert_settled();
    }
    // Readable again: the held guest changes reach the host as usual.
    rig.run(24);
    assert_eq!(rig.host("sub/x.txt").as_deref(), Some(&b"guest edit"[..]));
    assert_eq!(rig.host("sub/new.txt").as_deref(), Some(&b"guest new"[..]));
    rig.assert_settled();
}

#[test]
fn synced_file_replaced_by_a_symlink_keeps_its_guest_copy() {
    let mut rig = rig_with("unseen-link", 512, &[("a.txt", b"A"), ("gone.txt", b"G")]);
    let outside = rig.dir.with_extension("link-target");
    std::fs::write(&outside, b"outside the share").unwrap();
    std::fs::remove_file(rig.dir.join("a.txt")).unwrap();
    std::os::unix::fs::symlink(&outside, rig.dir.join("a.txt")).unwrap();
    std::fs::remove_file(rig.dir.join("gone.txt")).unwrap();
    rig.run(24);
    let kept = rig.guest.content("a.txt").map(<[u8]>::to_vec);
    let gone = rig.guest.content("gone.txt").is_none();
    rig.guest.write("a.txt", b"guest edit");
    rig.run(24);
    let gets = rig.guest.gets.clone();
    let target = std::fs::read(&outside);
    let _ = std::fs::remove_file(&outside);
    assert_eq!(kept.as_deref(), Some(&b"A"[..]), "guest copy deleted");
    assert!(gone, "a real host deletion still applies");
    assert!(gets.is_empty(), "{gets:?}");
    assert_eq!(std::fs::read_link(rig.dir.join("a.txt")).unwrap(), outside);
    assert_eq!(target.unwrap(), b"outside the share");
    rig.assert_settled();
}
