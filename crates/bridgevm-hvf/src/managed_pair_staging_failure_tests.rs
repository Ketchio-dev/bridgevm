use super::*;
use libc::ENOSPC;

/// A full volume ends the streaming copy after one chunk with an error, not
/// a crash, so restore itself must remove the partial copy.
#[test]
fn a_full_volume_mid_streaming_restore_keeps_selected_pair_and_no_partial_copy() {
    for selected in [false, true] {
        let tag = format!("managed-stream-full-{selected}");
        let (_scratch, disk, vars, snapshot, old, new) = fixture(&tag, selected);
        let mut pair = LockedPair::open(&disk, &vars).expect("own pair");
        let full = |src: &Path, dst: &Path| {
            stream_and_sync(src, dst, |_| Err(io::Error::from_raw_os_error(ENOSPC)))
        };
        let available = free_space::available_bytes;
        match pair.restore_copying(&snapshot, available, full, refuse_publication) {
            Err(SnapshotError::Io(error)) => assert_eq!(error.raw_os_error(), Some(ENOSPC)),
            other => panic!("expected the full-volume error, got {other:?}"),
        }
        assert_eq!(contents(&pair), old, "failed restore changed the selection");
        let staging = pair.root.join("staging");
        assert!(
            !staging.exists(),
            "failed restore stranded its partial copy"
        );
        let stream = |src: &Path, dst: &Path| stream_and_sync(src, dst, |_| Ok(()));
        let publish = crate::snapshot_pair::snapshot_publish::publish;
        pair.restore_copying(&snapshot, available, stream, publish)
            .expect("streaming retry after a full volume");
        assert_eq!(contents(&pair), new, "retry did not select the snapshot");
    }
}
