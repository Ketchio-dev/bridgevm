use super::*;
use crate::snapshot_pair::{copy_and_sync, create_snapshot, snapshot_pair_tests::Scratch};
use std::path::PathBuf;

pub(super) struct Fixture {
    pub(super) scratch: Scratch,
    pub(super) root: PathBuf,
    pub(super) stage: PathBuf,
    pub(super) identity: (u64, u64),
    pub(super) manifest: SnapshotManifest,
}

impl Fixture {
    pub(super) fn new(tag: &str) -> Self {
        let scratch = Scratch::new(tag);
        let disk = scratch.write("disk", b"verified snapshot disk");
        let vars = scratch.write("vars", b"verified snapshot vars");
        let snapshot = scratch.path("snapshot");
        let manifest = create_snapshot(&disk, &vars, &snapshot, "seal-test", false, 1024).unwrap();
        let root = scratch.path("managed");
        let stage = root.join("staging");
        private_directory(&root, true).unwrap();
        private_directory(&stage, true).unwrap();
        for name in ["disk.raw", "vars.fd", "manifest.json"] {
            copy_and_sync(&snapshot.join(name), &stage.join(name)).unwrap();
        }
        let id = Identity::from_metadata(&fs::symlink_metadata(&stage).unwrap());
        Self {
            scratch,
            root,
            stage,
            identity: (id.dev, id.ino),
            manifest,
        }
    }

    pub(super) fn seal(&self) -> Record {
        create(&self.root, &self.stage, self.identity, &self.manifest)
            .unwrap()
            .unwrap()
    }

    pub(super) fn assert_media(&self) {
        assert_eq!(
            crate::snapshot_pair::verify_snapshot(&self.stage).unwrap(),
            self.manifest
        );
    }
}
