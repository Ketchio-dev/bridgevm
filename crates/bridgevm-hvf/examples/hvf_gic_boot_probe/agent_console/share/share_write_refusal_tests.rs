//! A guest file whose write the guest write policy refuses for good is not
//! fetched again while the guest lists it unchanged (follow-up of 2026-10-23).
//! Here the refusal is real: every file created in the share inherits an ACL
//! entry that denies writing extended attributes, so its quarantine mark
//! cannot be set. A new size or mtime fetches the file again, and so does any
//! other failure. At most 1024 refusals are remembered; the oldest goes first.

use super::*;
use crate::agent_console::share::share_host_write::get_test_fixture::fetched;

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

/// What every new temporary on this thread fails with from now on; None: nothing.
fn fail_temporaries(errno: Option<i32>) {
    crate::agent_console::share::host_destination::TEMPORARY_FAULT.with(|fault| fault.set(errno));
}

/// A small file skips the free-space precheck, and APFS refuses its write while it still reports
/// space free (s5, 2026-11-10: 4 MiB refused with 33.7 MB free). The app is told once per
/// version, the fetch is retried at each listing, and it lands once there is room.
#[test]
fn a_write_refused_for_want_of_space_is_told_low_space_once_per_version() {
    let mut rig = Rig::new("low-space");
    let told = |rig: &Rig| rig.h.share.as_ref().unwrap().low_space_seen.clone();
    let version = |mtime: &str| ("a.txt".to_string(), Some(mtime.to_string()));
    // Any other failure is no space problem.
    fail_temporaries(Some(libc::EACCES));
    rig.guest.write("a.txt", b"guest");
    rig.run(12);
    assert!(!rig.guest.gets.is_empty());
    assert!(told(&rig).is_empty(), "a refused permission was told as low space");
    fail_temporaries(Some(libc::ENOSPC));
    let gets = rig.guest.gets.len();
    rig.run(24);
    assert!(rig.guest.gets.len() > gets + 1, "the write was not retried at each listing");
    assert_eq!(told(&rig), HashSet::from([version("m1")]), "told at every retry, or never");
    // A new version is told again.
    rig.guest.write("a.txt", b"GUEST");
    rig.run(12);
    assert_eq!(told(&rig), HashSet::from([version("m1"), version("m2")]));
    // Room again: the next listing's fetch lands.
    fail_temporaries(None);
    rig.run(12);
    assert_eq!(rig.host("a.txt").as_deref(), Some(&b"GUEST"[..]));
}
