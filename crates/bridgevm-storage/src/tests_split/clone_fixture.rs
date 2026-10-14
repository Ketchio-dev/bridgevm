//! Small valid raw sources for clone copy and graph tests.

use super::helpers::{manifest, temp_store};
use crate::*;
use bridgevm_config::VmManifest;
use std::fs;

pub(super) fn raw_manifest(name: &str) -> VmManifest {
    let mut manifest = manifest(name);
    manifest.storage.primary.format = "raw".into();
    manifest.storage.primary.size = "4KiB".into();
    manifest
}

pub(super) fn source(relative: &str, levels: usize) -> VmStore {
    let store = temp_store();
    let mut manifest = raw_manifest("source");
    manifest.storage.primary.path = relative.into();
    let bundle = store.create_vm(&manifest).unwrap();
    fs::create_dir_all(bundle.join(relative).parent().unwrap()).unwrap();
    fs::write(bundle.join(relative), vec![0x6du8; 4096]).unwrap();
    for level in 0..levels {
        let name = format!("level-{level}");
        store
            .create_snapshot("source", &name, SnapshotKind::Disk)
            .unwrap();
        store.create_snapshot_disk("source", &name).unwrap();
    }
    store
}
