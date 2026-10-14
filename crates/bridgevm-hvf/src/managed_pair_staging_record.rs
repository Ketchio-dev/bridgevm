//! A restore seal binds its own inode to the original copied generation.

use crate::snapshot_pair::SnapshotManifest;
use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};

pub(super) const ROOT_NAME: &str = ".restore-stage-owner";
pub(super) const PIN_NAME: &str = ".bridgevm-restore-owner";
const LIMIT: u64 = 256 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Identity {
    pub(super) dev: u64,
    pub(super) ino: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Member {
    pub(super) identity: Identity,
    pub(super) len: u64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    pub(super) version: u32,
    pub(super) root: Identity,
    pub(super) record: Identity,
    pub(super) stage: Identity,
    pub(super) disk: Member,
    pub(super) vars: Member,
    pub(super) manifest: Member,
    pub(super) source_manifest: String,
}

impl Identity {
    pub(super) fn from_metadata(metadata: &fs::Metadata) -> Self {
        use std::os::unix::fs::MetadataExt;
        Self {
            dev: metadata.dev(),
            ino: metadata.ino(),
        }
    }

    pub(super) fn matches(self, path: &Path) -> io::Result<bool> {
        Ok(Self::from_metadata(&fs::symlink_metadata(path)?) == self)
    }
}

impl Record {
    pub(super) fn encode(&self) -> io::Result<Vec<u8>> {
        let bytes = serde_json::to_vec(self).map_err(io::Error::other)?;
        if bytes.len() as u64 > LIMIT {
            return Err(io::Error::other(
                "restore ownership record exceeds its size limit",
            ));
        }
        Ok(bytes)
    }

    pub(super) fn capture(
        root: &Path,
        stage: &Path,
        expected: (u64, u64),
        manifest: &SnapshotManifest,
        record: Identity,
    ) -> io::Result<Self> {
        metadata::capture(root, stage, expected, manifest, record)
    }

    pub(super) fn read_pinned(root: &Path, candidate: &Path) -> io::Result<Self> {
        reading::read(root, candidate)
    }

    pub(super) fn matches_stage(&self, stage: &Path) -> io::Result<()> {
        metadata::matches_stage(self, stage)
    }

    pub(super) fn expected_manifest(&self) -> io::Result<SnapshotManifest> {
        let manifest =
            SnapshotManifest::from_json(&self.source_manifest).map_err(io::Error::other)?;
        if self.version != 1 || manifest.to_json() != self.source_manifest {
            return Err(io::Error::other(
                "invalid restore ownership manifest or version",
            ));
        }
        Ok(manifest)
    }
}

#[path = "managed_pair_staging_record_metadata.rs"]
mod metadata;
#[path = "managed_pair_staging_record_read.rs"]
mod reading;
