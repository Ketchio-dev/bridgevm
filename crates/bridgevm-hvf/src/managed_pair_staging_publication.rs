//! Retain this attempt's directory identity across publication and uncertainty.
//! Before publication, failure can clear that exact directory. After a rename
//! or exchange, it must keep any previous generation now at the staging name.

use super::StageOwner;
use std::{io, path::Path};

impl StageOwner {
    pub(in crate::snapshot_pair::managed::restore) fn publish(
        &self,
        destination: &Path,
        publish: impl FnOnce(&Path, &Path) -> io::Result<()>,
    ) -> io::Result<()> {
        publish(&self.path, destination).inspect_err(|_| self.clear())
    }
}
#[cfg(test)]
#[path = "managed_pair_staging_publication_tests.rs"]
mod tests;
