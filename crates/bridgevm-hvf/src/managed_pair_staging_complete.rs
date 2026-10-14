//! Seal a completed attempt durably before its verification read can be observed.

use super::super::{fill, seal};
use super::StageOwner;
use crate::snapshot_pair::{verify_snapshot, SnapshotError, SnapshotManifest};
use std::{fs::File, io, path::Path};

impl StageOwner {
    pub(in crate::snapshot_pair::managed::restore::staging) fn fill(
        &mut self,
        snapshot: &Path,
        manifest: &SnapshotManifest,
        copy: impl FnMut(&Path, &Path) -> io::Result<u64>,
    ) -> Result<SnapshotManifest, SnapshotError> {
        let result = (|| {
            fill::copy_files(snapshot, &self.path, copy)?;
            self.record = seal::create(
                self.path.parent().unwrap(),
                &self.path,
                self.identity,
                manifest,
            )?;
            let copied = verify_snapshot(&self.path)?;
            if &copied != manifest {
                return Err(io::Error::other("snapshot changed while staging restore").into());
            }
            File::open(&self.path)?.sync_all()?;
            Ok(copied)
        })();
        result.inspect_err(|_| self.clear())
    }
}
