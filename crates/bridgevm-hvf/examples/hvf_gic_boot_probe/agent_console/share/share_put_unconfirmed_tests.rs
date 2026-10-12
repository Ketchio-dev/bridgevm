//! A refused PUT may have truncated the guest copy (`bvagent.ps1` writes with
//! `FileMode.Create`), so the record's hash no longer describes that copy. A
//! host file later restored to the last-synced bytes is re-sent, and the
//! truncated copy is never pulled back over it.

use super::*;
use std::time::SystemTime;

/// How the host file gets the last-synced bytes back after the refused PUT.
#[derive(Clone, Copy, Debug)]
enum Restore {
    /// `git checkout` or an editor saving identical bytes, after later scans.
    NewMtime,
    /// `cp -p`, `rsync -a` or a Finder or Time Machine restore, before the
    /// next scan: the synced size and mtime, so the scan sees no change.
    SyncedMtime,
}

/// Synced `doc.bin` whose next PUT failed after truncating the guest copy;
/// the host file is then restored to the synced bytes.
fn refused_then_restored(
    label: &str,
    size: usize,
    restore: Restore,
    disk_freed: bool,
) -> (Rig, Vec<u8>) {
    let original = vec![b'o'; size];
    let mut rig = Rig::synced(label, "doc.bin", &original);
    let path = rig.dir.join("doc.bin");
    let synced_mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
    std::fs::write(&path, vec![b'n'; size + 7]).unwrap();
    rig.guest.failing.insert("doc.bin".into());
    match restore {
        Restore::NewMtime => rig.run(24),
        // Stop right after the refusal, before the next host scan.
        Restore::SyncedMtime => {
            for _ in 0..24 {
                if rig.guest.puts.len() == 2 {
                    break;
                }
                rig.run(1);
            }
        }
    }
    assert_eq!(rig.guest.puts.len(), 2, "{label}: {:?}", rig.guest.puts);
    let partial = rig.guest.content("doc.bin").unwrap();
    assert!(
        partial.len() < size && partial.iter().all(|b| *b == b'n'),
        "{label}"
    );
    std::fs::write(&path, &original).unwrap();
    let mtime = match restore {
        Restore::NewMtime => SystemTime::now() + Duration::from_secs(5),
        Restore::SyncedMtime => synced_mtime,
    };
    let file = std::fs::File::options().write(true).open(&path).unwrap();
    file.set_modified(mtime).unwrap();
    if disk_freed {
        rig.guest.failing.clear();
    }
    rig.run(24);
    let host = rig.host("doc.bin").unwrap();
    assert!(
        host == original,
        "{label}: host overwritten by the guest copy: {} bytes {:?}",
        host.len(),
        String::from_utf8_lossy(&host[..host.len().min(8)])
    );
    assert!(rig.guest.gets.is_empty(), "{label}: {:?}", rig.guest.gets);
    assert_eq!(
        rig.guest.puts.len(),
        3,
        "{label}: re-sent once {:?}",
        rig.guest.puts
    );
    (rig, original)
}

/// 100 B uses legacy PUT (ERR PUT); 30000 B uses PUTBEG..PUTEND (ERR PUTEND short).
fn cases(name: &str) -> Vec<(usize, String, Restore)> {
    let restores = [Restore::NewMtime, Restore::SyncedMtime];
    let sizes = [(100, "put"), (30_000, "putend")];
    let label = |kind, restore| format!("{name}-{kind}-{restore:?}");
    (restores.iter())
        .flat_map(|&r| {
            sizes
                .iter()
                .map(move |&(size, kind)| (size, label(kind, r), r))
        })
        .collect()
}

#[test]
fn host_restored_to_the_synced_bytes_after_a_refused_put_is_resent_not_pulled() {
    for (size, label, restore) in cases("restore") {
        let (mut rig, original) = refused_then_restored(&label, size, restore, true);
        assert_eq!(rig.guest.content("doc.bin"), Some(&original[..]), "{label}");
        rig.assert_settled();
        // A guest edit after the confirmed PUT still reaches the host.
        rig.guest.write("doc.bin", b"guest edit");
        rig.run(24);
        assert_eq!(
            rig.host("doc.bin").as_deref(),
            Some(&b"guest edit"[..]),
            "{label}"
        );
        assert_eq!(rig.guest.gets, ["doc.bin"], "{label}");
        rig.assert_settled();
    }
}

#[test]
fn restored_host_file_still_refused_waits_for_the_next_host_change() {
    for (size, label, restore) in cases("restore-full") {
        let (mut rig, original) = refused_then_restored(&label, size, restore, false);
        assert_ne!(rig.guest.content("doc.bin"), Some(&original[..]), "{label}");
        // Refused again: neither retried every tick nor pulled back.
        rig.assert_settled();
        // Deleting the truncated guest copy leaves the host file alone.
        rig.guest.files.retain(|(name, _, _)| name != "doc.bin");
        rig.run(24);
        assert_eq!(rig.host("doc.bin"), Some(original), "{label}");
        rig.guest.failing.clear();
        let changed = vec![b'c'; size + 3];
        std::fs::write(rig.dir.join("doc.bin"), &changed).unwrap();
        rig.run(24);
        assert_eq!(rig.guest.content("doc.bin"), Some(&changed[..]), "{label}");
        assert_eq!(rig.host("doc.bin"), Some(changed), "{label}");
        assert!(rig.guest.gets.is_empty(), "{label}: {:?}", rig.guest.gets);
        rig.assert_settled();
    }
}
