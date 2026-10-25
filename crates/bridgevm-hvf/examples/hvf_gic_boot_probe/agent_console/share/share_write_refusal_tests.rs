//! A guest file whose write the guest write policy refuses for good is not
//! fetched again while the guest lists it unchanged (follow-up of 2026-10-23).
//! Here the refusal is real: every file created in the share inherits an ACL
//! entry that denies writing extended attributes, so its quarantine mark
//! cannot be set. A new size or mtime fetches the file again, and so does any
//! other failure. At most 1024 refusals are remembered; the oldest goes first.

use super::*;

/// From now on every file created in `dir` inherits "deny writeextattr".
fn deny_marks(dir: &Path) {
    let status = std::process::Command::new("/bin/chmod")
        .args(["+a", "everyone deny writeextattr,file_inherit"])
        .arg(dir)
        .status()
        .unwrap();
    assert!(status.success());
}

fn set_mode(dir: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// The names a guest listing of `names`, one byte each at mtime m1, fetches.
fn fetches(h: &mut AgentConsoleHarness, names: &[String]) -> Vec<String> {
    let entries = names.iter().map(|name| LsEntry {
        name: name.clone(),
        size: 1,
        is_dir: false,
        mtime: "m1".into(),
    });
    let engine = &mut h.share.as_mut().unwrap().engine;
    let mut gets: Vec<String> = engine
        .on_guest_listing_normalized(entries)
        .into_iter()
        .filter_map(|action| match action {
            SyncAction::Get { name } => Some(name),
            _ => None,
        })
        .collect();
    gets.sort();
    gets
}

/// Finish a share GET of `name` that returned `bytes`, as its GETEND does.
fn fetched(h: &mut AgentConsoleHarness, name: &str, bytes: &[u8]) {
    h.get_accum = Some(GetAccum {
        path: name.into(),
        total: bytes.len(),
        nchunks: 1,
        bytes: bytes.to_vec(),
        chunks_seen: 1,
    });
    h.handle_share_get_end(name, "", Instant::now());
}

#[test]
fn a_refused_guest_file_is_not_fetched_again_until_it_changes() {
    let mut rig = Rig::new("refused-unchanged");
    deny_marks(&rig.dir);
    rig.guest.write("a.txt", b"guest");
    rig.run(12);
    assert!(
        rig.host_names().is_empty(),
        "a refused guest file was published"
    );
    rig.run(24);
    let gets = rig.guest.gets.len();
    assert_eq!(
        gets, 1,
        "an unchanged refused file was fetched {gets} times"
    );
    // A new mtime with the same size is another version: fetched once.
    rig.guest.write("a.txt", b"GUEST");
    rig.run(24);
    assert_eq!(rig.guest.gets.len(), 2);
    // So is a new size with the same mtime.
    rig.guest.files[0].1 = b"guest, longer".to_vec();
    rig.run(24);
    assert_eq!(rig.guest.gets.len(), 3);
    assert!(rig.host_names().is_empty());
}

#[test]
fn any_other_write_failure_is_fetched_again() {
    let mut rig = Rig::new("refused-transient");
    // The temporary cannot be created: a failure before the policy runs.
    set_mode(&rig.dir, 0o555);
    rig.guest.write("a.txt", b"guest");
    rig.run(12);
    let gets = rig.guest.gets.len();
    assert!(gets >= 1);
    rig.run(12);
    assert!(
        rig.guest.gets.len() > gets,
        "a failed write was not retried"
    );
    set_mode(&rig.dir, 0o755);
    rig.run(12);
    assert_eq!(rig.host("a.txt").as_deref(), Some(&b"guest"[..]));
    rig.assert_settled();
}

#[test]
fn at_most_1024_refusals_are_remembered_and_the_oldest_is_fetched_again() {
    let mut rig = Rig::new("refused-bound");
    deny_marks(&rig.dir);
    let names: Vec<String> = (0..=1024).map(|i| format!("f{i:04}.txt")).collect();
    assert_eq!(fetches(&mut rig.h, &names).len(), 1025);
    for name in &names {
        fetched(&mut rig.h, name, b"x");
    }
    assert!(rig.host_names().is_empty());
    // The 1025th refusal evicted the first one, which alone is fetched again.
    let again = fetches(&mut rig.h, &names);
    assert_eq!(
        again.len(),
        1,
        "{} refused files fetched again",
        again.len()
    );
    assert_eq!(again[0], names[0]);
    // Refused again, it evicts the next oldest.
    fetched(&mut rig.h, &names[0], b"x");
    assert_eq!(fetches(&mut rig.h, &names), [names[1].clone()]);
}
