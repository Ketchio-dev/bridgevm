//! Bytes read before a replacement must not inherit its newer path timestamp.

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
    rig.h.enqueue_one_share_host_change_with(|share, name| {
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
