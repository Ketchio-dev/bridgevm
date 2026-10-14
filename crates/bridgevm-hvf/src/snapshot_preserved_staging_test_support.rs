//! Simulate explicit operator resolution after an ambiguous completed swap.

use crate::snapshot_pair::managed::LockedPair;
use std::{fs, path::Path};

pub(super) fn retry(pair: &mut LockedPair, snapshot: &Path, root: &Path, sealed: bool) {
    let staged = root.join("staging");
    if !sealed {
        let read = || {
            ["disk.raw", "vars.fd", "manifest.json"].map(|name| fs::read(staged.join(name)).ok())
        };
        let before = read();
        let selected = pair.paths().unwrap();
        let refused = pair.restore(snapshot).unwrap_err();
        assert!(refused.to_string().contains("it was left intact"));
        assert_eq!(read(), before);
        assert_eq!(pair.paths().unwrap(), selected);
        let preserved = root.with_file_name("preserved-complete-staging");
        fs::rename(&staged, preserved).unwrap();
    }
    pair.restore(snapshot)
        .expect("retry after explicit staging resolution");
}
