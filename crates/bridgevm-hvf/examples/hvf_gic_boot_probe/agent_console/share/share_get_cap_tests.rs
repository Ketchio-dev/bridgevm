//! A guest-to-host share GET is bounded by the share's size cap, not only by
//! the 64 MiB GET reassembly cap. A listing names each version with its size,
//! but the GET returns whatever the file holds by then. A fetched file over the
//! cap is not written, a host copy already synced is kept, and the listings
//! that still name that version do not fetch it again.

use super::*;

/// A harness whose share syncs a fresh host directory with a 1 KiB cap.
fn capped(label: &str) -> (AgentConsoleHarness, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bvagent-get-cap-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut h = harness();
    h.share = Some(ShareState {
        engine: ShareSync::new(1),
        host_dir: dir.clone(),
        guest_dir: ROOT.into(),
        interval: Duration::from_secs(1),
        last_poll: None,
        host_skip_seen: HashSet::new(),
        guest_ls_scratch: Vec::new(),
        host_scan_scratch: Vec::new(),
    });
    (h, dir)
}

/// The share actions for a guest listing of one file version.
fn list(h: &mut AgentConsoleHarness, name: &str, size: u64, mtime: &str) -> Vec<SyncAction> {
    let entry = LsEntry {
        name: name.into(),
        size,
        is_dir: false,
        mtime: mtime.into(),
    };
    h.share
        .as_mut()
        .unwrap()
        .engine
        .on_guest_listing_normalized([entry])
}

/// Finish a share GET of `name` that returned `bytes`, as its GETEND does.
fn fetched(h: &mut AgentConsoleHarness, name: &str, bytes: &[u8]) {
    let (total, bytes) = (bytes.len(), bytes.to_vec());
    h.get_accum = Some(GetAccum {
        path: name.into(),
        total,
        nchunks: 1,
        bytes,
        chunks_seen: 1,
    });
    h.handle_share_get_end(name, "", Instant::now());
}

fn get(name: &str) -> Vec<SyncAction> {
    vec![SyncAction::Get { name: name.into() }]
}

#[test]
fn a_fetched_guest_file_over_the_cap_is_not_written_or_fetched_again() {
    let (mut h, dir) = capped("new");
    // Listed within the 1 KiB cap; by the time it is fetched it holds one byte more.
    assert_eq!(list(&mut h, "grew.bin", 10, "m1"), get("grew.bin"));
    fetched(&mut h, "grew.bin", &[7; 1025]);
    assert!(
        !dir.join("grew.bin").exists(),
        "an over-cap guest file was written to the host"
    );
    assert!(
        list(&mut h, "grew.bin", 10, "m1").is_empty(),
        "the refused version was fetched again"
    );
    assert!(list(&mut h, "grew.bin", 10, "m1").is_empty());
    // A new version is fetched as before, and one exactly at the cap is written.
    assert_eq!(list(&mut h, "grew.bin", 1024, "m2"), get("grew.bin"));
    fetched(&mut h, "grew.bin", &[8; 1024]);
    assert_eq!(std::fs::read(dir.join("grew.bin")).unwrap(), [8; 1024]);
    assert!(list(&mut h, "grew.bin", 1024, "m2").is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_existing_host_copy_is_kept_when_its_next_guest_version_is_over_the_cap() {
    let (mut h, dir) = capped("existing");
    assert_eq!(list(&mut h, "keep.txt", 8, "m1"), get("keep.txt"));
    fetched(&mut h, "keep.txt", b"old copy");
    assert_eq!(list(&mut h, "keep.txt", 8, "m2"), get("keep.txt"));
    fetched(&mut h, "keep.txt", &[1; 4096]);
    let host = std::fs::read(dir.join("keep.txt")).unwrap();
    assert!(
        host == b"old copy",
        "the host copy became {} bytes",
        host.len()
    );
    assert!(
        list(&mut h, "keep.txt", 8, "m2").is_empty(),
        "the refused version was fetched again"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_refused_version_is_forgotten_once_the_guest_stops_listing_it() {
    let (mut h, dir) = capped("forgotten");
    assert_eq!(list(&mut h, "gone.bin", 10, "m1"), get("gone.bin"));
    fetched(&mut h, "gone.bin", &[0; 2048]);
    let engine = &mut h.share.as_mut().unwrap().engine;
    assert!(engine.on_guest_listing_normalized(Vec::new()).is_empty());
    assert_eq!(list(&mut h, "gone.bin", 10, "m1"), get("gone.bin"));
    let _ = std::fs::remove_dir_all(&dir);
}
