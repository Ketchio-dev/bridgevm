//! Split test module.

use crate::*;
use std::fs;

use super::helpers::*;

#[test]
fn creates_state_and_snapshot_metadata() {
    let store = temp_store();
    store.create_vm(&manifest("dev")).unwrap();

    let state = store.state("dev").unwrap();
    assert_eq!(state.state, VmRuntimeState::Stopped);
    let chain = store.snapshot_chain("dev").unwrap();
    assert_eq!(chain.active_disk.source, ActiveDiskSource::Primary);
    assert!(chain.disks.is_empty());
    let token = store.guest_tools_token("dev").unwrap();
    assert_eq!(token.token.len(), 64);
    assert!(token
        .token
        .chars()
        .all(|character| character.is_ascii_hexdigit()));
    assert_eq!(store.guest_tools_token("dev").unwrap(), token);
    assert!(store
        .bundle_path("dev")
        .join("metadata")
        .join("guest-tools-token.json")
        .exists());

    let state = store
        .transition_state("dev", VmRuntimeState::Running)
        .unwrap();
    assert_eq!(state.state, VmRuntimeState::Running);

    let snapshot = store
        .create_snapshot("dev", "before-upgrade", SnapshotKind::Disk)
        .unwrap();
    assert_eq!(snapshot.vm_state, VmRuntimeState::Running);
    assert_eq!(store.snapshots("dev").unwrap().len(), 1);
    let disk = store
        .snapshot_disk_metadata("dev", "before-upgrade")
        .unwrap()
        .expect("disk snapshot metadata");
    assert_eq!(disk.snapshot, "before-upgrade");
    assert_eq!(disk.overlay_format, "qcow2");
    assert!(!disk.overlay_exists);
    assert_eq!(disk.backing_format, "qcow2");
    assert!(disk
        .overlay_path
        .ends_with("disks/snapshots/before-upgrade.qcow2"));
    assert_eq!(
        disk.create_command[..7],
        ["qemu-img", "create", "-f", "qcow2", "-F", "qcow2", "-b"]
    );
    fs::write(&disk.backing_path, b"fake backing").unwrap();

    let restore = store.restore_snapshot("dev", "before-upgrade").unwrap();
    assert_eq!(restore.snapshot, "before-upgrade");
    assert_eq!(restore.restored_state, VmRuntimeState::Running);
    assert_eq!(
        restore.active_disk.as_ref().map(|disk| disk.source),
        Some(ActiveDiskSource::SnapshotBacking)
    );
    assert_eq!(store.last_restore("dev").unwrap(), Some(restore));
}
