//! Publication failure must never clear the old generation after an exchange.

use crate::snapshot_pair::managed::LockedPair;
use crate::snapshot_pair::{create_snapshot, snapshot_pair_tests::Scratch};
use std::{fs, io, path::Path};

fn contents(directory: &Path) -> [Vec<u8>; 3] {
    ["disk.raw", "vars.fd", "manifest.json"].map(|name| fs::read(directory.join(name)).unwrap())
}

#[test]
fn error_after_exchange_keeps_previous_generation_at_staging() {
    let scratch = Scratch::new("restore-exchange-error-identity");
    let disk = scratch.write("disk", b"old disk");
    let vars = scratch.write("vars", b"old vars");
    let snapshot = scratch.path("snapshot");
    create_snapshot(&disk, &vars, &snapshot, "vm", false, 1024).unwrap();
    let mut pair = LockedPair::open(&disk, &vars).unwrap();
    pair.restore(&snapshot).unwrap();
    let current = pair.paths().unwrap().0.parent().unwrap().to_path_buf();
    fs::write(current.join("disk.raw"), b"guest writes").unwrap();
    let before = contents(&current);
    let staged = current.with_file_name("staging");
    let result = pair.restore_using(&snapshot, |new, old| {
        // Model the successful exchange boundary before directory sync fails.
        let parked = scratch.path("parked");
        fs::rename(old, &parked)?;
        fs::rename(new, old)?;
        fs::rename(parked, new)?;
        Err(io::Error::other("injected post-exchange sync failure"))
    });
    assert!(result.is_err());
    assert_eq!(contents(&staged), before);
    assert_eq!(contents(&current), contents(&snapshot));
    assert!(
        pair.restore(&snapshot).is_err(),
        "later attempts must preserve the ambiguous old generation"
    );
    assert_eq!(contents(&staged), before);
    // A crash during old-generation cleanup can leave the manifest even when
    // one media member is already gone; that still does not prove ownership.
    fs::remove_file(staged.join("disk.raw")).unwrap();
    assert!(pair.restore(&snapshot).is_err());
    assert_eq!(fs::read(staged.join("vars.fd")).unwrap(), before[1]);
    assert_eq!(fs::read(staged.join("manifest.json")).unwrap(), before[2]);
    assert!(!staged.join("disk.raw").exists());
    assert_eq!(contents(&current), contents(&snapshot));
}
