//! Preserve the original sparse-file preallocation refusal on the snapshot path.

use super::*;

#[test]
fn share_file_read_rejects_oversized_file_before_allocation() {
    let mut rig = Rig::new("host-snapshot-sparse-cap");
    rig.h.share.as_mut().unwrap().engine = ShareSync::new(4);
    let file = std::fs::File::create(rig.dir.join("big.bin")).unwrap();
    file.set_len(512 * 1024 * 1024).unwrap();
    let error = read_share_host_snapshot(rig.h.share.as_mut().unwrap(), "big.bin")
        .err()
        .unwrap();
    assert_eq!(error, ShareFileReadError::TooLarge(512 * 1024 * 1024));
}
