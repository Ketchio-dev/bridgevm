use crate::snapshot_pair::creation::create_snapshot_using;
use crate::snapshot_pair::creation::stage::CreateStage;
use crate::snapshot_pair::{
    create_snapshot, snapshot_pair_tests::Scratch, verify_snapshot, SnapshotError, DISK_NAME,
    VARS_NAME,
};
use std::fs;

#[test]
fn replacing_a_snapshot_refuses_while_its_members_are_being_copied() {
    let scratch = Scratch::new("create-source-member-replacement");
    let old_disk = scratch.write("old-disk", b"old-disk");
    let old_vars = scratch.write("old-vars", b"old-vars");
    let new_disk = scratch.write("new-disk", b"new-disk");
    let new_vars = scratch.write("new-vars", b"new-vars");
    let source = scratch.path("S");
    let original = create_snapshot(&old_disk, &old_vars, &source, "old", false, 1024).unwrap();
    // Separate parents prevent the existing parent lease from masking missing
    // ownership of S's members when another create replaces S itself.
    let target = scratch.path("export-parent/T");
    let mut replacement = None;
    let exported = create_snapshot_using(
        &source.join(DISK_NAME),
        &source.join(VARS_NAME),
        &target,
        "export",
        false,
        1024,
        |stage| {
            if stage == CreateStage::DiskSynced {
                assert!(replacement.is_none(), "disk hook must run exactly once");
                replacement = Some(create_snapshot(
                    &new_disk, &new_vars, &source, "new", false, 1024,
                ));
            }
        },
    )
    .expect("export from the leased source must finish");
    let replacement = replacement.expect("nested create must run after the disk copy");

    // Verify and read T before asserting refusal: a baseline failure must show
    // whether the real nested create produced a manifest-valid mixed pair.
    assert_eq!(verify_snapshot(&target).unwrap(), exported);
    let disk = fs::read(target.join(DISK_NAME)).unwrap();
    let vars = fs::read(target.join(VARS_NAME)).unwrap();
    let mixed_pair = disk == b"old-disk" && vars == b"new-vars";
    if let Ok(replaced) = &replacement {
        assert_eq!(verify_snapshot(&source).unwrap(), *replaced);
        assert!(
            mixed_pair,
            "nested create succeeded but mixed-pair hypothesis was not reproduced: \
             disk={disk:?}, vars={vars:?}"
        );
        eprintln!("nested create replaced S; T's manifest verifies old-disk/new-vars");
    }
    assert!(
        matches!(&replacement, Err(SnapshotError::Io(error))
            if error.kind() == std::io::ErrorKind::WouldBlock),
        "replacement must refuse leased destination members: {replacement:?}; \
         T manifest verified, mixed_old_disk_new_vars={mixed_pair}, disk={:?}, vars={:?}",
        String::from_utf8_lossy(&disk),
        String::from_utf8_lossy(&vars),
    );
    assert_eq!(disk, b"old-disk");
    assert_eq!(vars, b"old-vars");
    assert_eq!(verify_snapshot(&source).unwrap(), original);
}
