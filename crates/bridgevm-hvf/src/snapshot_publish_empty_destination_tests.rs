//! Empty destinations need atomic rename, not directory exchange support.

use super::*;
use crate::snapshot_pair::{create_snapshot, verify_snapshot, SnapshotManifest};

fn snapshot(scratch: &Scratch, name: &str, bytes: &[u8]) -> SnapshotManifest {
    let disk = scratch.write(&format!("{name}-disk"), bytes);
    let vars = scratch.write(&format!("{name}-vars"), bytes);
    create_snapshot(&disk, &vars, &scratch.path(name), name, false, 1024).unwrap()
}

#[test]
fn empty_destination_does_not_require_exchange_support() {
    let scratch = Scratch::new("publish-empty-no-exchange");
    let stage = scratch.path("stage");
    let dest = scratch.path("dest");
    let new = snapshot(&scratch, "stage", b"new-pair");
    fs::create_dir(&dest).unwrap();
    let mut exchanged = false;
    let result = publish_with(&stage, &dest, |_, _| {
        exchanged = true;
        Err(io::Error::from_raw_os_error(libc::ENOTSUP))
    });
    result.expect("an admitted empty destination must not require exchange support");
    assert!(!exchanged);
    assert_eq!(verify_snapshot(&dest).unwrap(), new);
    assert!(!stage.exists());
}

#[test]
fn complete_destination_still_requires_exchange_support() {
    let scratch = Scratch::new("publish-complete-no-exchange");
    let old = snapshot(&scratch, "dest", b"old-pair");
    let new = snapshot(&scratch, "stage", b"new-pair");
    let stage = scratch.path("stage");
    let dest = scratch.path("dest");
    let result = publish_with(&stage, &dest, |_, _| {
        Err(io::Error::from_raw_os_error(libc::ENOTSUP))
    });
    assert_eq!(result.unwrap_err().raw_os_error(), Some(libc::ENOTSUP));
    assert_eq!(verify_snapshot(&dest).unwrap(), old);
    assert_eq!(verify_snapshot(&stage).unwrap(), new);
}

#[path = "snapshot_publish_empty_interruption_tests.rs"]
mod interruption_tests;
#[path = "snapshot_publish_empty_race_tests.rs"]
mod race_tests;
