use super::*;
use crate::snapshot_pair::{
    copy_and_sync, create_snapshot,
    managed::{init, LockedPair},
    snapshot_pair_tests::Scratch,
    SnapshotManifest,
};
use std::path::PathBuf;

pub(super) struct Fixture {
    pub(super) scratch: Scratch,
    pub(super) disk: PathBuf,
    pub(super) vars: PathBuf,
    pub(super) snapshot: PathBuf,
    pub(super) root: PathBuf,
    pub(super) stage: PathBuf,
    pub(super) selected: (PathBuf, PathBuf),
    pub(super) old: [Vec<u8>; 2],
    pub(super) source: [Vec<u8>; 3],
    pub(super) manifest: SnapshotManifest,
}

pub(super) fn contents(path: &Path) -> [Vec<u8>; 3] {
    ["disk.raw", "vars.fd", "manifest.json"].map(|name| fs::read(path.join(name)).unwrap())
}

impl Fixture {
    pub(super) fn new(tag: &str, selected: bool, sealed: bool) -> Self {
        let scratch = Scratch::new(tag);
        let disk = scratch.write("logical-disk", b"original disk");
        let vars = scratch.write("logical-vars", b"original vars");
        let source_disk = scratch.write("source-disk", b"snapshot disk");
        let source_vars = scratch.write("source-vars", b"snapshot vars");
        let snapshot = scratch.path("snapshot");
        let manifest =
            create_snapshot(&source_disk, &source_vars, &snapshot, "source", false, 1024).unwrap();
        let initial = scratch.path("initial");
        if selected {
            create_snapshot(&disk, &vars, &initial, "initial", false, 1024).unwrap();
        }
        let mut pair = LockedPair::open(&disk, &vars).unwrap();
        if selected {
            pair.restore(&initial).unwrap();
            let current = pair.paths().unwrap();
            fs::write(&current.0, b"current guest disk").unwrap();
            fs::write(&current.1, b"current guest vars").unwrap();
        }
        let selected = pair.paths().unwrap();
        let old = [
            fs::read(&selected.0).unwrap(),
            fs::read(&selected.1).unwrap(),
        ];
        let root = pair.root.clone();
        init::initialize(&root, |_| {}).unwrap();
        let stage = root.join("staging");
        if sealed {
            // Use the product producer; dropping its lease models a completed
            // process exit without the publisher's same-attempt error cleanup.
            let (owner, copied) = pair
                .stage_restore(&snapshot, &manifest, copy_and_sync)
                .unwrap();
            assert_eq!(copied, manifest);
            drop(owner);
            Record::read_pinned(&root, &stage)
                .unwrap()
                .matches_stage(&stage)
                .unwrap();
        }
        drop(pair);
        let source = contents(&snapshot);
        Self {
            scratch,
            disk,
            vars,
            snapshot,
            root,
            stage,
            selected,
            old,
            source,
            manifest,
        }
    }

    pub(super) fn pair(&self) -> LockedPair {
        LockedPair::open(&self.disk, &self.vars).unwrap()
    }

    pub(super) fn complete_unsealed(&self) {
        private_directory(&self.stage, true).unwrap();
        for name in ["disk.raw", "vars.fd", "manifest.json"] {
            copy_and_sync(&self.snapshot.join(name), &self.stage.join(name)).unwrap();
        }
    }

    pub(super) fn assert_inputs(&self) {
        assert_eq!(contents(&self.snapshot), self.source);
        assert_eq!(fs::read(&self.disk).unwrap(), b"original disk");
        assert_eq!(fs::read(&self.vars).unwrap(), b"original vars");
        assert_eq!(
            [
                fs::read(&self.selected.0).unwrap(),
                fs::read(&self.selected.1).unwrap()
            ],
            self.old
        );
    }

    pub(super) fn assert_refused(&self) {
        let before = namespace::capture(&self.scratch.0);
        for _ in 0..2 {
            let mut pair = self.pair();
            assert!(pair.restore(&self.snapshot).is_err());
            assert_eq!(pair.paths().unwrap(), self.selected);
            self.assert_inputs();
            assert_eq!(namespace::capture(&self.scratch.0), before);
        }
    }
}

#[path = "managed_pair_staging_recovery_namespace.rs"]
pub(super) mod namespace;
