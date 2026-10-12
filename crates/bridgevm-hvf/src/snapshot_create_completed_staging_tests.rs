//! Staging holding a published manifest may be a completed snapshot, such as a
//! user's export to `.NAME.staging`, so creation refuses rather than reclaim it.

use super::{Scratch, QUOTA};
use crate::snapshot_pair::*;
use std::ffi::OsString;
use std::fs;
use std::path::Path;

type Files = Option<Vec<(OsString, Vec<u8>)>>;

/// A directory's regular files and their bytes, or None when it is absent.
fn files(dir: &Path) -> Files {
    let mut files: Vec<_> = fs::read_dir(dir)
        .ok()?
        .map(|entry| {
            let entry = entry.unwrap();
            (entry.file_name(), fs::read(entry.path()).unwrap())
        })
        .collect();
    files.sort();
    Some(files)
}

fn assert_kept_and_named(result: Result<SnapshotManifest, SnapshotError>, staging: &Path) {
    let text = match result {
        Err(SnapshotError::Io(error)) => error.to_string(),
        other => panic!("expected a completed-staging refusal, got {other:?}"),
    };
    // Creation resolves the parent, so a linked TMPDIR is named canonically.
    let parent = fs::canonicalize(staging.parent().unwrap()).unwrap();
    let canonical = parent.join(staging.file_name().unwrap());
    let named = [staging, &canonical].map(|path| text.contains(&*path.to_string_lossy()));
    assert!(
        named.contains(&true),
        "refusal names neither {staging:?} nor {canonical:?}: {text}"
    );
    for wanted in ["holds a published manifest.json", "remove or rename it"] {
        assert!(text.contains(wanted), "refusal lacks {wanted:?}: {text}");
    }
}

/// Creation refuses while staging holds a published manifest and leaves both
/// outputs byte-identical; the user then removes staging as the refusal says.
pub fn refused_then_removed(disk: &Path, vars: &Path, dest: &Path) {
    let staging = staging_path(dest);
    let before = (files(&staging), files(dest));
    let result = create_snapshot(disk, vars, dest, "refused", false, QUOTA);
    let after = (files(&staging), files(dest));
    assert_eq!(after, before, "refusal changed an output");
    assert_kept_and_named(result, &staging);
    fs::remove_dir_all(&staging).unwrap();
}

/// Retry after an interruption. A `finished` attempt left a verified snapshot
/// in staging, which refuses the retry until the user removes it; a retry over
/// unfinished debris reclaims it directly.
pub fn retry_create(disk: &Path, vars: &Path, dest: &Path, finished: bool) -> SnapshotManifest {
    if finished {
        verify_snapshot(&staging_path(dest)).expect("finished staging verifies");
        refused_then_removed(disk, vars, dest);
    }
    let retried = create_snapshot(disk, vars, dest, "retry", false, QUOTA).expect("retry");
    assert!(!staging_path(dest).exists(), "retry left staging");
    retried
}

#[test]
fn an_export_at_a_sibling_staging_name_survives_the_sibling_export() {
    let s = Scratch::new("completed-staging-sibling");
    let disk = s.write("disk", b"AAAA");
    let vars = s.write("vars", b"VVVV");
    let first = s.path(".旅行.snapshot.staging");
    let second = s.path("旅行.snapshot");
    let exported = create_snapshot(&disk, &vars, &first, "vm", false, QUOTA).unwrap();
    let before = files(&first);
    fs::write(&disk, b"BBBB").unwrap();
    let result = create_snapshot(&disk, &vars, &second, "vm", false, QUOTA);
    assert_eq!(
        files(&first),
        before,
        "the completed export changed: {result:?}"
    );
    assert_eq!(verify_snapshot(&first).unwrap(), exported);
    assert_kept_and_named(result, &first);
    assert!(!second.exists(), "refusal published a snapshot");
    // Once the user moves the earlier export aside, the export proceeds.
    let moved = s.path("旅行-earlier.snapshot");
    fs::rename(&first, &moved).unwrap();
    let created = create_snapshot(&disk, &vars, &second, "vm", false, QUOTA).unwrap();
    assert_eq!(verify_snapshot(&second).unwrap(), created);
    assert_eq!(fs::read(second.join(DISK_NAME)).unwrap(), b"BBBB");
    assert_eq!(
        (verify_snapshot(&moved).unwrap(), files(&moved)),
        (exported, before)
    );
    assert!(!first.exists());
}

/// Validity is not consulted: a published manifest alone keeps the directory,
/// including with exFAT's "._" companions beside its files.
#[test]
fn staging_with_a_published_manifest_is_kept_with_the_destination() {
    let cases: [&[&str]; 3] = [
        &[DISK_NAME, VARS_NAME, MANIFEST_NAME],
        &[MANIFEST_NAME],
        &[DISK_NAME, "._disk.raw", MANIFEST_NAME, "._manifest.json"],
    ];
    for (index, names) in cases.into_iter().enumerate() {
        let s = Scratch::new(&format!("completed-staging-{index}"));
        let disk = s.write("disk", b"disk");
        let vars = s.write("vars", b"vars");
        let dest = s.path("snapshot");
        let old = create_snapshot(&disk, &vars, &dest, "old", false, QUOTA).unwrap();
        let staging = staging_path(&dest);
        fs::create_dir(&staging).unwrap();
        for name in names {
            fs::write(staging.join(name), b"finished").unwrap();
        }
        refused_then_removed(&disk, &vars, &dest);
        assert_eq!(verify_snapshot(&dest).unwrap(), old, "{names:?}");
        let new = create_snapshot(&disk, &vars, &dest, "new", false, QUOTA).unwrap();
        assert_eq!(verify_snapshot(&dest).unwrap(), new, "{names:?}");
        assert!(!staging.exists(), "{names:?}");
    }
}
