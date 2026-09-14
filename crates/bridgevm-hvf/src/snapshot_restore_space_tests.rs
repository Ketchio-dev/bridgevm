//! Snapshot copy independence and refusal cleanup.

use super::snapshot_pair_tests::Scratch;
use super::*;

#[test]
fn a_failed_restore_leaves_no_staging_file_behind() {
    let s = Scratch::new("no-debris");
    let disk = s.write("disk", b"live disk");
    let vars = s.write("vars", b"live vars");
    create_snapshot(&disk, &vars, &s.path("snap"), "vm", false, QUOTA).unwrap();
    // Remove the snapshot's disk so the staging copy fails after verification
    // would have passed.
    fs::remove_file(s.path("snap").join(DISK_NAME)).unwrap();

    assert!(restore_snapshot(&s.path("snap"), &disk, &vars, false).is_err());
    let debris: Vec<_> = fs::read_dir(&s.0)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".restore"))
        .collect();
    assert!(
        debris.is_empty(),
        "a failed restore left staging files behind"
    );
}

#[test]
fn a_clone_reproduces_the_bytes_and_an_independent_write_does_not_disturb_it() {
    // Copy-on-write is only safe here because the clone stops sharing the
    // moment either side is written. If it did not, restoring a snapshot and
    // then booting the VM would rewrite the snapshot too.
    let s = Scratch::new("clone");
    let src = s.write("src", b"original bytes");
    let dst = s.path("dst");
    if super::free_space::clone_file(&src, &dst).is_none() {
        return; // not APFS; the copy path covers this case
    }
    assert_eq!(fs::read(&dst).unwrap(), b"original bytes");
    fs::write(&dst, b"rewritten!!!!!").unwrap();
    assert_eq!(fs::read(&src).unwrap(), b"original bytes");
}

#[test]
fn cloning_refuses_rather_than_overwriting_an_existing_file() {
    let s = Scratch::new("clone-exists");
    let src = s.write("src", b"source");
    let dst = s.write("dst", b"must survive");
    assert!(super::free_space::clone_file(&src, &dst).is_none());
    assert_eq!(fs::read(&dst).unwrap(), b"must survive");
}
