use super::*;
use crate::snapshot_pair::creation::staging_debris::clear_staging_then;

/// exFAT keeps each written file's extended attributes in a "._" AppleDouble
/// file beside it, so an interrupted export there leaves those in staging too.
#[test]
fn apple_double_companions_in_staging_are_cleared_for_retry() {
    let cases: [&[&str]; 4] = [
        &["._disk.raw"],
        &[DISK_NAME, "._disk.raw"],
        &[
            VARS_NAME,
            "._vars.fd",
            ".manifest.json.tmp",
            "._.manifest.json.tmp",
        ],
        &[DISK_NAME, "._disk.raw", MANIFEST_NAME, "._manifest.json"],
    ];
    for (index, names) in cases.into_iter().enumerate() {
        let s = Scratch::new(&format!("debris-apple-double-{index}"));
        let (disk, vars) = sources(&s);
        let dest = s.path("snapshot");
        let staging = staging_path(&dest);
        fs::create_dir(&staging).unwrap();
        for name in names {
            fs::write(staging.join(name), b"partial").unwrap();
        }
        let created = create_snapshot(&disk, &vars, &dest, "vm", false, QUOTA)
            .unwrap_or_else(|error| panic!("{names:?}: {error}"));
        assert_eq!(verify_snapshot(&dest).unwrap(), created, "{names:?}");
        assert!(!staging.exists(), "{names:?}: staging was not cleared");
    }
}

#[test]
fn staging_companions_that_are_not_debris_are_refused_and_left_intact() {
    for case in ["other-name", "linked", "directory"] {
        let s = Scratch::new(&format!("debris-not-companion-{case}"));
        let (disk, vars) = sources(&s);
        let dest = s.path("snapshot");
        let staging = staging_path(&dest);
        fs::create_dir(&staging).unwrap();
        let outside = s.write("outside", b"outside data");
        match case {
            "other-name" => fs::write(staging.join("._keep.txt"), b"user data").unwrap(),
            "linked" => symlink(&outside, staging.join("._disk.raw")).unwrap(),
            "directory" => {
                fs::create_dir(staging.join("._disk.raw")).unwrap();
                fs::write(staging.join("._disk.raw/keep.txt"), b"nested").unwrap();
            }
            _ => unreachable!(),
        }
        let before = (tree(&staging), tree(&outside));
        let result = create_uncopied(&disk, &vars, &dest, "vm");
        assert_eq!((tree(&staging), tree(&outside)), before, "{case}: changed");
        assert_refused(result, case);
        assert!(!dest.exists(), "{case}: refusal published a snapshot");
    }
}

/// exFAT unlinks a file's "._" companion with it, after staging was listed.
#[test]
fn a_companion_that_left_with_its_file_is_not_an_error() {
    let s = Scratch::new("debris-companion-gone");
    let staging = s.path(".snapshot.staging");
    fs::create_dir(&staging).unwrap();
    for name in [DISK_NAME, "._disk.raw"] {
        fs::write(staging.join(name), b"partial").unwrap();
    }
    let unlink = || fs::remove_file(staging.join("._disk.raw")).unwrap();
    clear_staging_then(&staging, unlink).unwrap();
    assert!(!staging.exists());
}

#[test]
fn an_entry_that_arrives_after_listing_is_kept() {
    let s = Scratch::new("debris-late-arrival");
    let staging = s.path(".snapshot.staging");
    fs::create_dir(&staging).unwrap();
    fs::write(staging.join(DISK_NAME), b"partial").unwrap();
    let arrive = || fs::write(staging.join("keep.txt"), b"arrived after listing").unwrap();
    assert!(clear_staging_then(&staging, arrive).is_err());
    let kept = fs::read(staging.join("keep.txt")).ok();
    assert_eq!(kept.as_deref(), Some(&b"arrived after listing"[..]));
    assert!(!staging.join(DISK_NAME).exists());
}
