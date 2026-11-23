//! Bytes read before a replacement must not inherit its newer path timestamp,
//! and the snapshot path refuses an oversized sparse file before allocating it.

use super::*;
use std::fs::File;
use std::io::Read;
use std::time::{Duration, UNIX_EPOCH};

struct OwnedHolder(PathBuf);

impl OwnedHolder {
    fn new() -> Self {
        for nonce in 0..128 {
            let path = std::env::temp_dir().join(format!(
                "bvagent-host-snapshot-{}-{nonce}",
                std::process::id()
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create owned replacement holder: {error}"),
            }
        }
        panic!("no fresh replacement holder available");
    }
}

impl Drop for OwnedHolder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_replacement_after_read_cannot_be_mistaken_for_the_synced_host_snapshot() {
    let mut rig = Rig::synced("host-snapshot-replaced", "a.txt", b"old-A");
    let path = rig.dir.join("a.txt");
    let holder = OwnedHolder::new();
    let replacement = holder.0.join("replacement");
    std::fs::write(&replacement, b"new-B").unwrap();
    let old_stamp = UNIX_EPOCH + Duration::from_secs(1_700_000_002);
    let new_stamp = UNIX_EPOCH + Duration::from_secs(1_700_000_003);
    File::open(&path).unwrap().set_modified(old_stamp).unwrap();
    File::open(&replacement)
        .unwrap()
        .set_modified(new_stamp)
        .unwrap();
    let mut reads = 0;
    rig.h.enqueue_one_share_host_change_with(rig.now, |share, name, _| {
        read_share_host_snapshot_with(share, name, |file, bytes, limit| {
            reads += 1;
            file.take(limit).read_to_end(bytes)?;
            std::fs::rename(&replacement, &path)
        })
    });
    assert_eq!(
        reads, 1,
        "the actual read/replacement interleaving was skipped"
    );
    assert_eq!(rig.host("a.txt").as_deref(), Some(&b"new-B"[..]));
    rig.run(24);
    assert_eq!(rig.host("a.txt").as_deref(), Some(&b"new-B"[..]));
    assert_eq!(rig.guest.content("a.txt"), Some(&b"new-B"[..]));
    rig.assert_settled();
}

#[test]
fn share_file_read_rejects_oversized_file_before_allocation() {
    let mut rig = Rig::new("host-snapshot-sparse-cap");
    rig.h.share.as_mut().unwrap().engine = ShareSync::new(4).into();
    let file = std::fs::File::create(rig.dir.join("big.bin")).unwrap();
    file.set_len(512 * 1024 * 1024).unwrap();
    let share = rig.h.share.as_mut().unwrap();
    let max_bytes = share.engine.max_bytes();
    let error = read_share_host_snapshot(share, "big.bin", max_bytes)
        .err()
        .unwrap();
    assert_eq!(error, ShareFileReadError::TooLarge(512 * 1024 * 1024));
}
